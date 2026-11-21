//! GitHub.com OAuth sign-in.
//!
//! GitHub is not an OpenID provider: it issues no ID token. [`GithubOAuth`]
//! runs the authorization code flow with S256 PKCE and state, redeems the code
//! for an access token, reads the authenticated user once, and drops the
//! token. The identity is GitHub's numeric account ID, never the mutable
//! login. No scope is requested, so the token reaches only public profile
//! data: no repository, organization or email API access.

use openidconnect::http::{HeaderValue, Method, StatusCode, header};
use openidconnect::{CsrfToken, PkceCodeChallenge};
use secrecy::{ExposeSecret as _, SecretString};
use serde::Deserialize;
use url::Url;

use crate::claims::MappedClaims;
use crate::relying_party::{Authorization, ProviderHttpError, RelyingPartyError};
use crate::screening::ScreenedHttp;

/// The fixed identity namespace every GitHub.com account is recorded under,
/// whatever endpoints a development deployment talks to.
pub const GITHUB_NAMESPACE: &str = "https://github.com";

/// GitHub's web and API endpoints.
///
/// Production uses GitHub.com; development and tests name controlled local
/// stand-ins through [`GithubEndpoints::at`].
#[derive(Debug, Clone)]
pub struct GithubEndpoints {
    /// Browser authorization endpoint.
    pub authorize: Url,
    /// Code-for-token endpoint.
    pub token: Url,
    /// Authenticated-user endpoint.
    pub user: Url,
}

impl GithubEndpoints {
    /// Endpoints under the web root `web` and the API root `api`, laid out
    /// as GitHub.com lays them out.
    ///
    /// # Errors
    /// Returns [`url::ParseError`] when either root cannot be joined.
    pub fn at(web: &Url, api: &Url) -> Result<Self, url::ParseError> {
        Ok(Self {
            authorize: web.join("login/oauth/authorize")?,
            token: web.join("login/oauth/access_token")?,
            user: api.join("user")?,
        })
    }
}

impl Default for GithubEndpoints {
    fn default() -> Self {
        let web = Url::parse("https://github.com/").expect("invariant: GitHub's web root parses");
        let api =
            Url::parse("https://api.github.com/").expect("invariant: GitHub's API root parses");
        Self::at(&web, &api).expect("invariant: GitHub's endpoints join")
    }
}

/// The GitHub.com OAuth client: authorization request and identity lookup.
#[derive(Debug, Clone)]
pub struct GithubOAuth {
    /// Screened transport both GitHub requests go through.
    http: ScreenedHttp,
    /// Where GitHub is.
    endpoints: GithubEndpoints,
}

/// The token endpoint's JSON answer; GitHub reports a refused code as `200`
/// with `error` set.
#[derive(Deserialize)]
struct TokenAnswer {
    /// The access token, when the code was redeemed.
    access_token: Option<SecretString>,
    /// GitHub's error code, when it was not.
    error: Option<String>,
}

/// The part of the authenticated user Wyrd reads.
#[derive(Deserialize)]
struct GithubUser {
    /// Stable numeric account ID.
    id: u64,
    /// Public profile email, when the person published one.
    email: Option<String>,
}

impl GithubOAuth {
    /// A client for `endpoints` whose requests go through `http`.
    #[must_use]
    pub const fn new(http: ScreenedHttp, endpoints: GithubEndpoints) -> Self {
        Self { http, endpoints }
    }

    /// Build one sign-in's authorization redirect for `client_id`, returning
    /// to `redirect_uri`, with fresh state and S256 PKCE and no scope.
    ///
    /// GitHub echoes no nonce; the returned one is random and nothing checks
    /// it.
    ///
    /// # Errors
    /// Returns [`RelyingPartyError::Screened`] when the authorization
    /// endpoint's scheme is refused.
    pub fn authorize(
        &self,
        client_id: &str,
        redirect_uri: &str,
    ) -> Result<Authorization, RelyingPartyError> {
        self.http
            .screen_scheme(&self.endpoints.authorize)
            .map_err(RelyingPartyError::Screened)?;
        let (challenge, verifier) = PkceCodeChallenge::new_random_sha256();
        let state = CsrfToken::new_random().into_secret();
        let mut url = self.endpoints.authorize.clone();
        url.query_pairs_mut()
            .append_pair("client_id", client_id)
            .append_pair("redirect_uri", redirect_uri)
            .append_pair("state", &state)
            .append_pair("code_challenge", challenge.as_str())
            .append_pair("code_challenge_method", "S256");
        Ok(Authorization {
            url,
            state,
            nonce: CsrfToken::new_random().into_secret(),
            code_verifier: SecretString::from(verifier.into_secret()),
        })
    }

    /// Redeem `code` and read the GitHub account it signed in.
    ///
    /// The access token lives only for the user lookup and is dropped on
    /// return.
    ///
    /// # Errors
    /// Returns [`RelyingPartyError::Screened`] when an endpoint is refused,
    /// [`RelyingPartyError::TokenEndpointUnavailable`] when GitHub cannot be
    /// reached or answers with a server error, and
    /// [`RelyingPartyError::TokenRejected`] when it refuses the code or the
    /// token, or answers unusably.
    pub async fn identity(
        &self,
        client_id: &str,
        client_secret: &SecretString,
        code: &SecretString,
        redirect_uri: &str,
        code_verifier: &SecretString,
    ) -> Result<MappedClaims, RelyingPartyError> {
        let form = url::form_urlencoded::Serializer::new(String::new())
            .append_pair("client_id", client_id)
            .append_pair("client_secret", client_secret.expose_secret())
            .append_pair("code", code.expose_secret())
            .append_pair("redirect_uri", redirect_uri)
            .append_pair("code_verifier", code_verifier.expose_secret())
            .finish();
        let request = openidconnect::http::Request::builder()
            .method(Method::POST)
            .uri(self.endpoints.token.as_str())
            .header(header::ACCEPT, HeaderValue::from_static("application/json"))
            .header(
                header::CONTENT_TYPE,
                HeaderValue::from_static("application/x-www-form-urlencoded"),
            )
            .body(form.into_bytes())
            .map_err(|error| RelyingPartyError::Configuration(error.to_string()))?;
        let answer: TokenAnswer = self.json(request).await?;
        let token = match (answer.access_token, answer.error) {
            (Some(token), None) => token,
            (_, error) => {
                return Err(RelyingPartyError::TokenRejected(error.unwrap_or_else(
                    || "the token answer carries no access token".to_owned(),
                )));
            }
        };
        let bearer = HeaderValue::from_str(&format!("Bearer {}", token.expose_secret()))
            .map_err(|_| RelyingPartyError::TokenRejected("unusable access token".to_owned()))?;
        let request = openidconnect::http::Request::builder()
            .method(Method::GET)
            .uri(self.endpoints.user.as_str())
            .header(header::AUTHORIZATION, bearer)
            .header(
                header::ACCEPT,
                HeaderValue::from_static("application/vnd.github+json"),
            )
            .header(header::USER_AGENT, HeaderValue::from_static("wyrd"))
            .body(Vec::new())
            .map_err(|error| RelyingPartyError::Configuration(error.to_string()))?;
        let user: GithubUser = self.json(request).await?;
        Ok(MappedClaims {
            subject: user.id.to_string(),
            email: user.email,
            groups: Vec::new(),
        })
    }

    /// Send `request` through the screened transport and decode a `200` JSON
    /// answer.
    ///
    /// # Errors
    /// As [`Self::identity`].
    async fn json<T: serde::de::DeserializeOwned>(
        &self,
        request: openidconnect::http::Request<Vec<u8>>,
    ) -> Result<T, RelyingPartyError> {
        let response = self.http.send(request).await.map_err(|error| match error {
            ProviderHttpError::Screened(error) => RelyingPartyError::Screened(error),
            error => RelyingPartyError::TokenEndpointUnavailable(error.to_string()),
        })?;
        if response.status() != StatusCode::OK {
            return Err(RelyingPartyError::TokenRejected(format!(
                "GitHub answered {}",
                response.status()
            )));
        }
        serde_json::from_slice(response.body())
            .map_err(|error| RelyingPartyError::TokenRejected(error.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// GitHub refuses a bad code with `200` and an `error` field; that is a
    /// rejection, and the user API is never called.
    #[tokio::test]
    async fn a_refused_code_reads_no_user() {
        let github = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::path("/login/oauth/access_token"))
            .respond_with(
                wiremock::ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({ "error": "bad_verification_code" })),
            )
            .mount(&github)
            .await;
        wiremock::Mock::given(wiremock::matchers::path("/user"))
            .respond_with(wiremock::ResponseTemplate::new(200))
            .expect(0)
            .mount(&github)
            .await;
        let root = Url::parse(&github.uri()).expect("mock url parses");
        let oauth = GithubOAuth::new(
            ScreenedHttp::allowing_internal(),
            GithubEndpoints::at(&root, &root).expect("endpoints join"),
        );
        let secret = SecretString::from("secret");
        let refused = oauth
            .identity("client", &secret, &secret, "http://wyrd.test/cb", &secret)
            .await;
        assert!(
            matches!(&refused, Err(RelyingPartyError::TokenRejected(reason)) if reason == "bad_verification_code"),
            "{refused:?}"
        );
    }
}
