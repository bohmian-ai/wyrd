//! Provider-backed human login steps shared by the CLI and SDK journeys.
//!
//! Every journey that needs a saved Wyrd user login drives the same served
//! path a person does: a tenant administrator stages, tests, and activates the
//! public Keycloak `wyrd-human` connection; the CLI's device login begins at
//! `POST /auth/device_authorization`; the fixture user approves the user code
//! on the verification page and signs in at the provider it redirects to;
//! the provider's return reaches the server's common callback; and the device
//! code is redeemed at `POST /auth/token`. [`HumanSso::save_login`] then
//! stores the credential exactly as `wyrd auth login` does, so the Rust,
//! Python, and TypeScript journeys exercise the CLI-established record
//! without each re-implementing the device flow.
//!
//! These helpers need the identity lane's Keycloak; every caller is an
//! ignored journey that lane selects.

use std::path::Path;
use std::time::Duration;

use serde_json::{Value, json};
use url::Url;
use wyrd_client::auth::{AuthError, TokenExchange};
use wyrd_client::saved_login::{SavedLogin, SavedLogins, canonical_origin};
use wyrd_spec::auth::{SecretBearer, TokenRequest, TokenResponse};
use wyrd_spec::ids::TenantSlug;

/// Public origin every human journey server is configured with; the Keycloak
/// fixture clients register `{origin}/auth/callback`.
pub const HUMAN_PUBLIC_ORIGIN: &str = "http://test-tenant-1.wyrd.test";

/// Route key of the harness's fixture tenant.
pub const FIXTURE_TENANT_SLUG: &str = "test-tenant-1";

/// The Keycloak issuer the identity lane serves, overridable through
/// `WYRD_KEYCLOAK_ISSUER`.
#[must_use]
pub fn keycloak_issuer() -> String {
    std::env::var("WYRD_KEYCLOAK_ISSUER")
        .unwrap_or_else(|_| "http://localhost:18080/realms/wyrd-test".to_owned())
}

/// Served human-login steps against one bound server.
pub struct HumanSso {
    /// The server's bound base URL.
    server: String,
    /// The server's unauthenticated `/auth` surface.
    exchange: TokenExchange,
    /// Plain client for administration calls and the provider callback.
    http: reqwest::Client,
}

impl HumanSso {
    /// Steps against the bound server at `server`.
    ///
    /// # Panics
    /// Panics when `server` is not a usable base URL.
    #[must_use]
    pub fn new(server: &str) -> Self {
        Self {
            server: server.trim_end_matches('/').to_owned(),
            exchange: TokenExchange::new(server, 30_000).expect("token exchange builds"),
            http: reqwest::Client::new(),
        }
    }

    /// Stage, test, and activate the public Keycloak `wyrd-human` connection
    /// for the tenant `admin_key` belongs to, mapping `wyrd-admins` to
    /// `admin` and `wyrd-viewers` to `reader`.
    ///
    /// The candidate test is a real sign-in as Keycloak's `alice`;
    /// `admin_key` is both the caller and the activation recovery key.
    ///
    /// # Panics
    /// Panics when any step fails.
    pub async fn activate_keycloak(&self, admin_key: &str) {
        let token = self.access_token(admin_key).await;
        let candidate = self
            .call(
                &token,
                reqwest::Method::PUT,
                "/v1/identity/oidc/candidate",
                json!({
                    "issuer": keycloak_issuer(),
                    "client_id": "wyrd-human",
                    "client_auth": "Public",
                    "claim_mapping": { "subject": "sub", "email": "email", "groups": "groups" },
                    "group_role_map": { "wyrd-admins": ["admin"], "wyrd-viewers": ["reader"] },
                }),
            )
            .await;
        let revision = candidate["revision"].clone();
        let begun = self
            .call(
                &token,
                reqwest::Method::POST,
                "/v1/identity/oidc/candidate/test",
                json!({ "expected_revision": revision }),
            )
            .await;
        let authorization_url: Url = begun["authorization_url"]
            .as_str()
            .expect("the test returns an authorization URL")
            .parse()
            .expect("authorization URL parses");
        let page = self
            .sign_in(&authorization_url, "alice", "alice-password")
            .await;
        assert!(
            page.contains("Connection test complete"),
            "the test sign-in completes: {page}"
        );
        self.call(
            &token,
            reqwest::Method::POST,
            "/v1/identity/oidc/candidate/activate",
            json!({ "expected_revision": revision, "recovery_api_key": admin_key }),
        )
        .await;
    }

    /// Sign `username` in at a provider authorization URL and deliver the
    /// provider's return to the server's common callback, returning the
    /// callback page.
    ///
    /// # Panics
    /// Panics when the URL names no callback, the provider sign-in fails, or
    /// the callback does not answer successfully.
    pub async fn sign_in(&self, authorization_url: &Url, username: &str, password: &str) -> String {
        let (_, callback) = authorization_url
            .query_pairs()
            .find(|(name, _)| name == "redirect_uri")
            .expect("the sign-in names the deployment callback");
        let returned =
            crate::provider_sign_in(authorization_url, username, password, &callback).await;
        let reply = self
            .http
            .get(format!(
                "{}/auth/callback?{}",
                self.server,
                returned.query().unwrap_or_default()
            ))
            .send()
            .await
            .expect("callback answers");
        let status = reply.status();
        let page = reply.text().await.unwrap_or_default();
        assert!(status.is_success(), "callback answered {status}: {page}");
        page
    }

    /// Complete one CLI device login for `tenant` as `username`: authorize a
    /// device code, approve its user code on the verification page, sign in
    /// at the provider it redirects to, and redeem the device code.
    ///
    /// # Panics
    /// Panics when any step fails or the device code is never redeemed.
    pub async fn cli_login(&self, tenant: &str, username: &str, password: &str) -> TokenResponse {
        let tenant: TenantSlug = tenant.parse().expect("tenant route key parses");
        let device = self
            .exchange
            .device_authorization(&tenant)
            .await
            .expect("the device login begins");
        let login_url = self.approve(&tenant, &device.user_code).await;
        self.sign_in(&login_url, username, password).await;
        let poll = TokenRequest::DeviceCode {
            device_code: device.device_code,
        };
        for _ in 0..10 {
            match self.exchange.exchange(&poll).await {
                Ok(token) => return token,
                Err(AuthError::Server(error))
                    if error.problem().details["error"] == "authorization_pending" =>
                {
                    tokio::time::sleep(Duration::from_secs(device.interval)).await;
                }
                Err(error) => panic!("the device code redeems: {error}"),
            }
        }
        panic!("the device login never completed");
    }

    /// Approve `user_code` for `tenant` on the verification page, posting
    /// from the deployment's origin as the page's own form does, and return
    /// the provider sign-in URL it redirects to.
    ///
    /// # Panics
    /// Panics when the page refuses the approval.
    pub async fn approve(&self, tenant: &TenantSlug, user_code: &str) -> Url {
        let page = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .expect("client builds");
        let reply = page
            .post(format!("{}/auth/device", self.server))
            .header(reqwest::header::ORIGIN, HUMAN_PUBLIC_ORIGIN)
            .form(&[
                ("tenant", tenant.as_str()),
                ("user_code", user_code),
                ("decision", "approve"),
            ])
            .send()
            .await
            .expect("verification page answers");
        assert_eq!(
            reply.status(),
            reqwest::StatusCode::SEE_OTHER,
            "approval redirects to sign-in"
        );
        reply
            .headers()
            .get(reqwest::header::LOCATION)
            .and_then(|location| location.to_str().ok())
            .expect("approval names the sign-in URL")
            .parse()
            .expect("sign-in URL parses")
    }

    /// Log `username` in to `tenant` through the CLI device login and save
    /// the credential under `config_home`, exactly as `wyrd auth login` does.
    ///
    /// # Panics
    /// Panics when the login or the save fails.
    pub async fn save_login(
        &self,
        config_home: &Path,
        tenant: &str,
        username: &str,
        password: &str,
    ) -> SavedLogin {
        let token = self.cli_login(tenant, username, password).await;
        let record = SavedLogin::from_token(
            self.origin(),
            tenant.parse().expect("tenant route key parses"),
            token,
        )
        .expect("the login carries a refresh token");
        saved_logins(config_home)
            .save(record.clone())
            .expect("the login saves");
        record
    }

    /// Revoke the refresh chain `refresh_token` belongs to on the server
    /// without touching any saved record, as another device's logout would.
    ///
    /// # Panics
    /// Panics when the server refuses the revocation.
    pub async fn revoke(&self, refresh_token: &SecretBearer) {
        self.exchange
            .revoke_refresh_token(refresh_token)
            .await
            .expect("the refresh chain revokes");
    }

    /// Make this server's saved login for `tenant` under `config_home` stale,
    /// so the next client renews it.
    ///
    /// # Panics
    /// Panics when the login is missing or cannot be saved.
    pub fn expire_saved(&self, config_home: &Path, tenant: &str) {
        expire_saved_access(config_home, &self.origin(), tenant);
    }

    /// This server's saved login for `tenant` under `config_home`.
    ///
    /// # Panics
    /// Panics when the login is missing.
    #[must_use]
    pub fn saved(&self, config_home: &Path, tenant: &str) -> SavedLogin {
        saved_login(config_home, &self.origin(), tenant)
    }

    /// Whether this server's saved login for `tenant` under `config_home`
    /// holds an expired access token; a journey checks it before and after a
    /// client call to prove the renewal was saved.
    ///
    /// # Panics
    /// Panics when the login is missing.
    #[must_use]
    pub fn saved_is_stale(&self, config_home: &Path, tenant: &str) -> bool {
        self.saved(config_home, tenant).access_expires_at <= chrono::Utc::now()
    }

    /// Revoke the server-side refresh chain of this server's saved login for
    /// `tenant` under `config_home`, keeping the record, as another device's
    /// logout would.
    ///
    /// # Panics
    /// Panics when the login is missing or the server refuses.
    pub async fn revoke_saved(&self, config_home: &Path, tenant: &str) {
        let login = saved_login(config_home, &self.origin(), tenant);
        self.revoke(&login.refresh_token).await;
    }

    /// The canonical origin this server's saved logins are keyed by.
    ///
    /// # Panics
    /// Panics when the server URL is not a usable origin.
    fn origin(&self) -> String {
        canonical_origin(&self.server).expect("server origin parses")
    }

    /// Exchange an API key for an access token.
    ///
    /// # Panics
    /// Panics when the exchange fails.
    async fn access_token(&self, api_key: &str) -> String {
        let body: Value = self
            .http
            .post(format!("{}/auth/token", self.server))
            .json(&json!({ "grant_type": "wyrd_api_key", "api_key": api_key }))
            .send()
            .await
            .expect("token endpoint answers")
            .json()
            .await
            .expect("token body is JSON");
        body["access_token"]
            .as_str()
            .unwrap_or_else(|| panic!("api key exchanges: {body}"))
            .to_owned()
    }

    /// Call one authenticated JSON route and return its body.
    ///
    /// # Panics
    /// Panics when the route does not answer successfully.
    async fn call(&self, token: &str, method: reqwest::Method, path: &str, body: Value) -> Value {
        let response = self
            .http
            .request(method, format!("{}{path}", self.server))
            .header("x-wyrd-access-token", format!("Bearer {token}"))
            .json(&body)
            .send()
            .await
            .expect("server answers");
        let status = response.status();
        let body: Value = response.json().await.unwrap_or(Value::Null);
        assert!(status.is_success(), "{path} returned {status}: {body}");
        body
    }
}

/// The saved logins in `credentials.toml` under the Wyrd configuration
/// directory `config_home`, the one `WYRD_CONFIG_HOME` names.
#[must_use]
pub fn saved_logins(config_home: &Path) -> SavedLogins {
    SavedLogins::at(config_home.to_path_buf())
}

/// The saved login for `tenant` at `origin` under `config_home`.
///
/// # Panics
/// Panics when the store refuses the selection or holds no such login.
#[must_use]
pub fn saved_login(config_home: &Path, origin: &str, tenant: &str) -> SavedLogin {
    saved_logins(config_home)
        .select(origin, Some(tenant))
        .expect("store selects")
        .expect("saved login exists")
}

/// Make the saved login for `tenant` at `origin` stale, so the next client
/// renews it. Saving it again makes it the newest login for `origin`.
///
/// # Panics
/// Panics when the record is missing or cannot be saved.
pub fn expire_saved_access(config_home: &Path, origin: &str, tenant: &str) {
    let mut record = saved_login(config_home, origin, tenant);
    record.access_expires_at = chrono::Utc::now() - chrono::Duration::minutes(1);
    saved_logins(config_home)
        .save(record)
        .expect("stale login saves");
}
