//! The deployment GitHub App's provider protocol.
//!
//! [`GithubApp`] holds the validated operator configuration and performs
//! every GitHub request Wyrd makes: the user OAuth code exchange, the user's
//! installation and repository access, the App's own installation record
//! (authenticated with an RS256 App JWT), and narrowed installation access
//! tokens. Each request goes through the deployment's [`ScreenedHttp`] with
//! a bounded body, and no token or key is logged or returned in an error.

use std::time::{SystemTime, UNIX_EPOCH};

use jsonwebtoken::{Algorithm, EncodingKey, Header};
use reqwest::{Method, RequestBuilder, StatusCode};
use secrecy::{ExposeSecret, SecretString};
use serde::Deserialize;
use serde::de::DeserializeOwned;
use url::Url;
use wyrd_auth_oidc::{ScreenedHttp, read_bounded_body};
use wyrd_spec::error::WyrdError;
use wyrd_spec::github_connection::GithubRepositoryAccess;

use crate::config::{ConfigError, GithubAppSettings};

/// GitHub REST API version every request pins.
const API_VERSION: &str = "2022-11-28";

/// Page size of every listed GitHub collection.
const PAGE_SIZE: usize = 100;

/// Pages read from one listed collection.
// ponytail: 50 pages (5,000 entries) per list; page further if a tenant's
// installation selection outgrows it.
const MAX_PAGES: usize = 50;

/// The App's installation as GitHub reports it to the App itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct AppInstallation {
    /// Installation id.
    pub(crate) id: u64,
    /// Whether GitHub suspended it.
    pub(crate) suspended: bool,
}

/// `GET /app/installations/{id}` body, as far as Wyrd reads it.
#[derive(Deserialize)]
struct InstallationBody {
    /// Installation id.
    id: u64,
    /// App the installation belongs to.
    app_id: u64,
    /// Suspension instant, when suspended.
    suspended_at: Option<String>,
}

/// `GET /user/installations` page.
#[derive(Deserialize)]
struct UserInstallationsPage {
    /// Total across pages.
    total_count: usize,
    /// This page.
    installations: Vec<InstallationBody>,
}

/// One repository as GitHub lists it.
#[derive(Deserialize)]
struct RepositoryBody {
    /// Stable repository id.
    id: u64,
    /// `owner/name`.
    full_name: String,
}

/// `GET /user/installations/{id}/repositories` page.
#[derive(Deserialize)]
struct RepositoriesPage {
    /// Total across pages.
    total_count: usize,
    /// This page.
    repositories: Vec<RepositoryBody>,
}

/// OAuth token endpoint body: a token or an error, both with status 200.
#[derive(Deserialize)]
struct TokenBody {
    /// User access token on success.
    access_token: Option<String>,
}

/// Installation access token body.
#[derive(Deserialize)]
struct InstallationTokenBody {
    /// The token.
    token: String,
}

/// App JWT claims (GitHub: `iat`, `exp` at most ten minutes later, `iss`).
#[derive(serde::Serialize)]
struct AppClaims {
    /// Issued at, backdated a minute for clock drift.
    iat: u64,
    /// Expiry, nine minutes after `iat`.
    exp: u64,
    /// The App id.
    iss: String,
}

/// The refusal for a GitHub failure Wyrd cannot attribute to the caller.
pub(crate) fn unavailable(operation: &str) -> WyrdError {
    WyrdError::GithubUnavailable {
        message: format!("GitHub did not complete {operation}"),
        details: serde_json::json!({ "operation": operation }),
    }
}

/// A connection refusal naming the failed check.
pub(crate) fn refused(reason: &str, message: &str) -> WyrdError {
    WyrdError::GithubConnectionRefused {
        message: message.to_owned(),
        details: serde_json::json!({ "reason": reason }),
    }
}

/// The deployment's configured GitHub App and its provider protocol.
pub struct GithubApp {
    /// Validated operator configuration.
    settings: GithubAppSettings,
    /// App JWT signing key parsed from the configured PEM.
    key: EncodingKey,
    /// Screened outbound HTTP for this deployment.
    http: ScreenedHttp,
}

impl std::fmt::Debug for GithubApp {
    /// Redacting debug: the settings' own redacted form only.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GithubApp")
            .field("settings", &self.settings)
            .finish_non_exhaustive()
    }
}

impl GithubApp {
    /// The App over validated `settings`, reaching GitHub through `http`.
    ///
    /// # Errors
    /// Returns [`ConfigError::Invalid`] when the private key is not an RSA
    /// private key PEM.
    pub fn new(settings: GithubAppSettings, http: ScreenedHttp) -> Result<Self, ConfigError> {
        let key = EncodingKey::from_rsa_pem(settings.private_key.expose_secret().as_bytes())
            .map_err(|_| ConfigError::Invalid {
                message: "github: the App private key is not an RSA private key PEM".to_owned(),
            })?;
        Ok(Self {
            settings,
            key,
            http,
        })
    }

    /// The validated configuration.
    pub(crate) const fn settings(&self) -> &GithubAppSettings {
        &self.settings
    }

    /// A URL under this deployment's public base.
    pub(crate) fn public(&self, path: &str) -> String {
        format!(
            "{}{path}",
            self.settings.public_url.as_str().trim_end_matches('/')
        )
    }

    /// The OAuth redirect URI GitHub returns the user to.
    pub(crate) fn callback_uri(&self) -> String {
        self.public("/v1/integrations/github/callback")
    }

    /// GitHub's install page for the App, carrying `state`.
    pub(crate) fn install_url(&self, state: &str) -> Url {
        let mut url = self.web(&format!("/apps/{}/installations/new", self.settings.slug));
        url.query_pairs_mut().append_pair("state", state);
        url
    }

    /// GitHub's OAuth authorization page for `state` and the S256
    /// `code_challenge`.
    pub(crate) fn authorize_url(&self, state: &str, code_challenge: &str) -> Url {
        let mut url = self.web("/login/oauth/authorize");
        url.query_pairs_mut()
            .append_pair("client_id", &self.settings.oauth_client_id)
            .append_pair("redirect_uri", &self.callback_uri())
            .append_pair("state", state)
            .append_pair("code_challenge", code_challenge)
            .append_pair("code_challenge_method", "S256");
        url
    }

    /// Whether `signature` (`X-Hub-Signature-256`) is the webhook secret's
    /// HMAC-SHA256 of `body`, compared in constant time.
    pub(crate) fn verify_webhook(&self, body: &[u8], signature: Option<&str>) -> bool {
        let Some(tag) = signature
            .and_then(|value| value.strip_prefix("sha256="))
            .and_then(|hex_tag| hex::decode(hex_tag).ok())
        else {
            return false;
        };
        wyrd_crypt::verify_hmac_sha256(
            self.settings.webhook_secret.expose_secret().as_bytes(),
            body,
            &tag,
        )
    }

    /// Exchange the user authorization `code` with its PKCE `code_verifier`
    /// for a transient user access token.
    ///
    /// # Errors
    /// Returns [`WyrdError::GithubConnectionRefused`] (`authorization_refused`)
    /// when GitHub refuses the code and [`WyrdError::GithubUnavailable`] when
    /// it cannot be reached or answers unexpectedly.
    pub(crate) async fn exchange_code(
        &self,
        code: &str,
        code_verifier: &SecretString,
    ) -> Result<SecretString, WyrdError> {
        let url = self.web("/login/oauth/access_token");
        let form = [
            ("client_id", self.settings.oauth_client_id.as_str()),
            (
                "client_secret",
                self.settings.oauth_client_secret.expose_secret(),
            ),
            ("code", code),
            ("redirect_uri", &self.callback_uri()),
            ("code_verifier", code_verifier.expose_secret()),
        ];
        let request = self.request(Method::POST, &url).await?.form(&form);
        let body: TokenBody = self.json(request, "the authorization exchange").await?;
        body.access_token
            .filter(|token| !token.is_empty())
            .map(SecretString::from)
            .ok_or_else(|| {
                refused(
                    "authorization_refused",
                    "GitHub refused the user authorization",
                )
            })
    }

    /// The App's own record of installation `id`; `None` when it is not an
    /// installation of this App.
    ///
    /// # Errors
    /// Returns [`WyrdError::GithubUnavailable`] when GitHub cannot be reached
    /// or answers unexpectedly.
    pub(crate) async fn installation(&self, id: u64) -> Result<Option<AppInstallation>, WyrdError> {
        let url = self.api(&format!("/app/installations/{id}"));
        let request = self
            .request(Method::GET, &url)
            .await?
            .bearer_auth(self.app_jwt()?.expose_secret());
        let response = request
            .send()
            .await
            .map_err(|_| unavailable("the installation read"))?;
        if response.status() == StatusCode::NOT_FOUND {
            return Ok(None);
        }
        let body: InstallationBody = Self::decode(response, "the installation read").await?;
        Ok(
            (body.id == id && body.app_id == self.settings.app_id).then_some(AppInstallation {
                id,
                suspended: body.suspended_at.is_some(),
            }),
        )
    }

    /// Ids of this App's installations the user behind `token` can access.
    ///
    /// # Errors
    /// Returns [`WyrdError::GithubUnavailable`] when GitHub cannot be reached
    /// or answers unexpectedly.
    pub(crate) async fn user_installations(
        &self,
        token: &SecretString,
    ) -> Result<Vec<u64>, WyrdError> {
        let mut ids = Vec::new();
        for page in 1..=MAX_PAGES {
            let url = self.page(&self.api("/user/installations"), page);
            let request = self
                .request(Method::GET, &url)
                .await?
                .bearer_auth(token.expose_secret());
            let body: UserInstallationsPage =
                self.json(request, "the user installation read").await?;
            let last = body.installations.len() < PAGE_SIZE || page * PAGE_SIZE >= body.total_count;
            ids.extend(
                body.installations
                    .into_iter()
                    .filter(|installation| installation.app_id == self.settings.app_id)
                    .map(|installation| installation.id),
            );
            if last {
                break;
            }
        }
        Ok(ids)
    }

    /// Repositories of installation `id` the user behind `token` can access:
    /// the installation's selected repositories, intersected with the user's.
    ///
    /// # Errors
    /// Returns [`WyrdError::GithubConnectionRefused`]
    /// (`installation_inaccessible`) when GitHub denies the user the
    /// installation and [`WyrdError::GithubUnavailable`] when it cannot be
    /// reached or answers unexpectedly.
    pub(crate) async fn user_repositories(
        &self,
        token: &SecretString,
        id: u64,
    ) -> Result<Vec<GithubRepositoryAccess>, WyrdError> {
        let mut repositories = Vec::new();
        for page in 1..=MAX_PAGES {
            let url = self.page(
                &self.api(&format!("/user/installations/{id}/repositories")),
                page,
            );
            let response = self
                .request(Method::GET, &url)
                .await?
                .bearer_auth(token.expose_secret())
                .send()
                .await
                .map_err(|_| unavailable("the repository read"))?;
            if matches!(
                response.status(),
                StatusCode::FORBIDDEN | StatusCode::NOT_FOUND
            ) {
                return Err(refused(
                    "installation_inaccessible",
                    "the authorizing GitHub user cannot access this installation",
                ));
            }
            let body: RepositoriesPage = Self::decode(response, "the repository read").await?;
            let last = body.repositories.len() < PAGE_SIZE || page * PAGE_SIZE >= body.total_count;
            repositories.extend(body.repositories.into_iter().map(|repository| {
                GithubRepositoryAccess {
                    repository_id: repository.id.to_string(),
                    full_name: repository.full_name,
                }
            }));
            if last {
                break;
            }
        }
        Ok(repositories)
    }

    /// A short-lived installation access token for installation `id`,
    /// narrowed to `repository_id` and to Contents read, Pull requests write
    /// and Checks write.
    ///
    /// # Errors
    /// Returns [`WyrdError::GithubRepositoryInaccessible`] when GitHub
    /// refuses the installation or repository and
    /// [`WyrdError::GithubUnavailable`] when it cannot be reached or answers
    /// unexpectedly.
    pub(crate) async fn installation_token(
        &self,
        id: u64,
        repository_id: u64,
    ) -> Result<SecretString, WyrdError> {
        let url = self.api(&format!("/app/installations/{id}/access_tokens"));
        let response = self
            .request(Method::POST, &url)
            .await?
            .bearer_auth(self.app_jwt()?.expose_secret())
            .json(&serde_json::json!({
                "repository_ids": [repository_id],
                "permissions": {
                    "contents": "read",
                    "pull_requests": "write",
                    "checks": "write",
                },
            }))
            .send()
            .await
            .map_err(|_| unavailable("the installation token request"))?;
        if matches!(
            response.status(),
            StatusCode::FORBIDDEN | StatusCode::NOT_FOUND | StatusCode::UNPROCESSABLE_ENTITY
        ) {
            return Err(WyrdError::GithubRepositoryInaccessible {
                message: "the installation can no longer reach this repository".to_owned(),
                details: serde_json::json!({ "repository_id": repository_id.to_string() }),
            });
        }
        let body: InstallationTokenBody =
            Self::decode(response, "the installation token request").await?;
        Ok(SecretString::from(body.token))
    }

    /// `path` under the web origin.
    fn web(&self, path: &str) -> Url {
        Self::join(&self.settings.web_origin, path)
    }

    /// `path` under the API base.
    fn api(&self, path: &str) -> Url {
        Self::join(&self.settings.api_origin, path)
    }

    /// `url` with the listing page parameters.
    fn page(&self, url: &Url, page: usize) -> Url {
        let mut url = url.clone();
        url.query_pairs_mut()
            .append_pair("per_page", &PAGE_SIZE.to_string())
            .append_pair("page", &page.to_string());
        url
    }

    /// `base` with `path` appended.
    ///
    /// # Panics
    /// Never: `base` is a validated http(s) URL and `path` an absolute path
    /// of URL-safe segments.
    fn join(base: &Url, path: &str) -> Url {
        Url::parse(&format!("{}{path}", base.as_str().trim_end_matches('/')))
            .expect("invariant: a configured origin joined with a fixed path is a URL")
    }

    /// A screened request to `url` with GitHub's JSON headers.
    ///
    /// # Errors
    /// Returns [`WyrdError::GithubUnavailable`] when screening refuses the
    /// URL or the client cannot be built.
    async fn request(&self, method: Method, url: &Url) -> Result<RequestBuilder, WyrdError> {
        let client = self
            .http
            .client_for(url)
            .await
            .map_err(|_| unavailable("a screened request"))?;
        Ok(client
            .request(method, url.clone())
            .header(reqwest::header::ACCEPT, "application/vnd.github+json")
            .header(reqwest::header::USER_AGENT, "wyrd")
            .header("X-GitHub-Api-Version", API_VERSION))
    }

    /// Send `request` and decode its successful JSON body.
    ///
    /// # Errors
    /// Returns [`WyrdError::GithubUnavailable`] for a transport failure, a
    /// non-success status, or an undecodable body.
    async fn json<T: DeserializeOwned>(
        &self,
        request: RequestBuilder,
        operation: &str,
    ) -> Result<T, WyrdError> {
        let response = request.send().await.map_err(|_| unavailable(operation))?;
        Self::decode(response, operation).await
    }

    /// Decode a successful bounded JSON body.
    ///
    /// # Errors
    /// Returns [`WyrdError::GithubUnavailable`] for a non-success status, an
    /// oversized body, or an undecodable one.
    async fn decode<T: DeserializeOwned>(
        response: reqwest::Response,
        operation: &str,
    ) -> Result<T, WyrdError> {
        if !response.status().is_success() {
            tracing::warn!(status = %response.status(), operation, "GitHub refused a request");
            return Err(unavailable(operation));
        }
        let body = read_bounded_body(response)
            .await
            .map_err(|_| unavailable(operation))?;
        serde_json::from_slice(&body).map_err(|_| unavailable(operation))
    }

    /// A fresh RS256 App JWT.
    ///
    /// # Errors
    /// Returns [`WyrdError::GithubUnavailable`] when the clock is before the
    /// Unix epoch or signing fails.
    fn app_jwt(&self) -> Result<SecretString, WyrdError> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| unavailable("App authentication"))?
            .as_secs();
        let claims = AppClaims {
            iat: now.saturating_sub(60),
            exp: now + 540,
            iss: self.settings.app_id.to_string(),
        };
        jsonwebtoken::encode(&Header::new(Algorithm::RS256), &claims, &self.key)
            .map(SecretString::from)
            .map_err(|_| unavailable("App authentication"))
    }
}
