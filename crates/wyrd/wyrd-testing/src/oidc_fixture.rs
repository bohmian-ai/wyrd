//! Real OIDC provider fixture for identity e2e tests.
//!
//! `OidcIssuerFixture` wraps a running Keycloak or Dex container and provides
//! helpers to drive human login (Keycloak only), mint workload tokens, and
//! force signing-key rotation (Keycloak only).  The harness smoke test (commit
//! 08) verifies discovery; the journey tests (commit 09) drive the full flows.

use url::Url;
use wyrd_auth_oidc::{OidcProvider, ProviderMetadata};
use wyrd_spec::auth::IssuerUrl;

/// Keycloak-specific admin context needed for privileged operations.
#[derive(Debug, Clone)]
pub struct KeycloakAdmin {
    /// Base URL of the Keycloak server (e.g. `http://localhost:8080`).
    pub base_url: String,
    /// Admin username.
    pub username: String,
    /// Admin password.
    pub password: String,
    /// Realm name.
    pub realm: String,
}

/// Result of a driven human login: the `code` and `state` query parameters
/// delivered to the redirect URI.
#[derive(Debug, Clone)]
pub struct LoginResult {
    /// Authorization code returned by the IdP.
    pub code: String,
    /// State value echoed back from the authorization request.
    pub state: String,
}

/// Connected OIDC provider fixture.
///
/// Construct via [`OidcIssuerFixture::connect`].  Use
/// [`OidcIssuerFixture::keycloak`] when Keycloak admin operations are needed.
pub struct OidcIssuerFixture {
    /// Normalized issuer URL (used for `TrustedIssuer.issuer`).
    pub issuer: IssuerUrl,
    provider: OidcProvider,
    http: reqwest::Client,
    admin: Option<KeycloakAdmin>,
}

impl OidcIssuerFixture {
    /// Connect to an OIDC provider by resolving its discovery document.
    ///
    /// `issuer_url` must point to the provider's base URL.  Discovery is
    /// fetched from `{issuer_url}/.well-known/openid-configuration`.
    ///
    /// # Panics
    /// Panics when discovery fails.
    pub async fn connect(issuer_base: &str) -> Self {
        let http = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .expect("reqwest client builds");

        let provider = {
            let discover_client = reqwest::Client::new();
            OidcProvider::discover(
                issuer_base.parse().expect("issuer URL parses"),
                discover_client,
            )
            .await
            .unwrap_or_else(|e| panic!("OIDC discovery failed for {issuer_base}: {e}"))
        };

        let issuer = IssuerUrl::new_for_tests(issuer_base);

        Self {
            issuer,
            provider,
            http,
            admin: None,
        }
    }

    /// Attach Keycloak admin credentials to enable privileged operations.
    #[must_use]
    pub fn with_keycloak_admin(mut self, admin: KeycloakAdmin) -> Self {
        self.admin = Some(admin);
        self
    }

    /// Discovery metadata from the provider.
    pub fn metadata(&self) -> &ProviderMetadata {
        &self.provider.metadata
    }

    /// Drive a full human login against a Keycloak HTML login form.
    ///
    /// The method:
    /// 1. Builds the authorization URL with the given params.
    /// 2. GETs it (without following redirects) to reach the Keycloak login page.
    /// 3. Parses the HTML form `action` URL.
    /// 4. POSTs the user credentials.
    /// 5. Follows the 302 to `redirect_uri` and captures `code` and `state`.
    ///
    /// Only works with Keycloak; panics if called against a non-Keycloak issuer.
    ///
    /// # Panics
    /// Panics when the login flow cannot be completed.
    // justification: test fixture mirrors the OIDC authorization-code login flow inputs (client_id, username, password, redirect_uri, state, code_challenge, nonce) 1:1; wrapping in a struct would add indirection for a single call site
    #[allow(clippy::too_many_arguments)]
    pub async fn human_login(
        &self,
        client_id: &str,
        username: &str,
        password: &str,
        redirect_uri: &Url,
        state: &str,
        code_challenge: &str,
        nonce: &str,
    ) -> LoginResult {
        let authz_url = self.provider.metadata.authorization_endpoint.clone();

        let authz_url = {
            let mut u = authz_url;
            u.query_pairs_mut()
                .append_pair("response_type", "code")
                .append_pair("client_id", client_id)
                .append_pair("redirect_uri", redirect_uri.as_str())
                .append_pair("scope", "openid email profile")
                .append_pair("state", state)
                .append_pair("nonce", nonce)
                .append_pair("code_challenge", code_challenge)
                .append_pair("code_challenge_method", "S256");
            u
        };

        // Step 1: GET authorization URL → Keycloak returns HTML login page
        let resp = self
            .http
            .get(authz_url.clone())
            .send()
            .await
            .expect("GET authorization URL succeeds");

        let (html, cookie) = if resp.status().is_redirection() {
            // Follow the redirect manually (Keycloak may redirect to the login page)
            let location = resp
                .headers()
                .get(reqwest::header::LOCATION)
                .expect("redirect has Location header")
                .to_str()
                .expect("Location is valid UTF-8")
                .to_owned();
            let session_cookie = extract_session_cookie(&resp);
            let mut req = self.http.get(&location);
            if let Some(ref cookie) = session_cookie {
                req = req.header(reqwest::header::COOKIE, cookie.as_str());
            }
            let login_page = req.send().await.expect("GET login page succeeds");
            let session_cookie_final =
                session_cookie.or_else(|| extract_session_cookie(&login_page));
            let body = login_page.text().await.expect("login page body is text");
            (body, session_cookie_final)
        } else {
            let session_cookie = extract_session_cookie(&resp);
            let body = resp.text().await.expect("login page body is text");
            (body, session_cookie)
        };

        // Step 2: Parse the form action URL from the HTML
        let form_action = parse_form_action(&html)
            .unwrap_or_else(|| panic!("could not find login form action in HTML:\n{html}"));

        // Step 3: POST credentials
        let mut post_req = self
            .http
            .post(&form_action)
            .form(&[("username", username), ("password", password)]);
        if let Some(ref c) = cookie {
            post_req = post_req.header(reqwest::header::COOKIE, c.as_str());
        }
        let post_resp = post_req
            .send()
            .await
            .expect("POST login credentials succeeds");

        // Step 4: Follow the redirect to the redirect_uri and capture code+state
        let location = post_resp
            .headers()
            .get(reqwest::header::LOCATION)
            .unwrap_or_else(|| {
                panic!(
                    "login POST returned {} without Location header",
                    post_resp.status()
                )
            })
            .to_str()
            .expect("Location header is valid UTF-8")
            .to_owned();

        let callback_url: Url = location.parse().expect("callback URL parses");
        let code = callback_url
            .query_pairs()
            .find(|(k, _)| k == "code")
            .map(|(_, v)| v.into_owned())
            .expect("callback URL contains 'code' param");
        let returned_state = callback_url
            .query_pairs()
            .find(|(k, _)| k == "state")
            .map(|(_, v)| v.into_owned())
            .expect("callback URL contains 'state' param");

        LoginResult {
            code,
            state: returned_state,
        }
    }

    /// Mint a workload token using the client-credentials grant.
    ///
    /// `client_id` and `client_secret` identify the registered workload client.
    /// `audience` is the value the minted token should carry in its `aud` claim
    /// (for providers that support audience scoping via request params).
    ///
    /// Returns the raw access-token string.
    ///
    /// # Panics
    /// Panics when the token endpoint request fails.
    pub async fn workload_token(
        &self,
        client_id: &str,
        client_secret: &str,
        _audience: &str,
    ) -> String {
        let token_url = self
            .provider
            .metadata
            .token_endpoint
            .as_ref()
            .expect("provider exposes a token endpoint");

        let resp = self
            .http
            .post(token_url.as_str())
            .form(&[
                ("grant_type", "client_credentials"),
                ("client_id", client_id),
                ("client_secret", client_secret),
            ])
            .send()
            .await
            .expect("workload token request succeeds");

        let status = resp.status();
        let body: serde_json::Value = resp.json().await.expect("token response is JSON");
        assert!(
            status.is_success(),
            "workload token request failed: status={status}, body={body}"
        );

        body["access_token"]
            .as_str()
            .expect("access_token present in response")
            .to_owned()
    }

    /// Force a Keycloak signing-key rotation via the Keycloak admin REST API.
    ///
    /// After this call, tokens signed by the old `kid` will be rejected on the
    /// next JWKS refresh (the verifier refetches on unknown kid).  Only works
    /// when this fixture was constructed with [`Self::with_keycloak_admin`].
    ///
    /// # Panics
    /// Panics when admin credentials are absent or the rotation API fails.
    pub async fn rotate_signing_key(&self) {
        let admin = self
            .admin
            .as_ref()
            .expect("call with_keycloak_admin before rotate_signing_key");

        // Obtain an admin access token
        let token_url = format!(
            "{}/realms/master/protocol/openid-connect/token",
            admin.base_url
        );
        let token_resp: serde_json::Value = self
            .http
            .post(&token_url)
            .form(&[
                ("grant_type", "password"),
                ("client_id", "admin-cli"),
                ("username", admin.username.as_str()),
                ("password", admin.password.as_str()),
            ])
            .send()
            .await
            .expect("Keycloak admin token request")
            .json()
            .await
            .expect("admin token response is JSON");

        let admin_token = token_resp["access_token"]
            .as_str()
            .expect("admin token present");

        // Trigger key rotation: create a new RSA key and activate it
        let keys_url = format!("{}/admin/realms/{}/components", admin.base_url, admin.realm);
        let new_key_body = serde_json::json!({
            "name": "rsa-generated-rotate",
            "providerId": "rsa-generated",
            "providerType": "org.keycloak.keys.KeyProvider",
            "parentId": admin.realm,
            "config": {
                "priority": ["200"],
                "enabled": ["true"],
                "active": ["true"],
                "algorithm": ["RS256"],
                "keySize": ["2048"]
            }
        });

        let resp = self
            .http
            .post(&keys_url)
            .bearer_auth(admin_token)
            .json(&new_key_body)
            .send()
            .await
            .expect("key rotation POST succeeds");

        assert!(
            resp.status().is_success() || resp.status() == reqwest::StatusCode::CREATED,
            "key rotation failed: status={}",
            resp.status()
        );
    }
}

/// Extract the first `KC_RESTART` or `AUTH_SESSION_ID` cookie from a response.
fn extract_session_cookie(resp: &reqwest::Response) -> Option<String> {
    resp.headers()
        .get_all(reqwest::header::SET_COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .find(|v| v.starts_with("AUTH_SESSION_ID") || v.starts_with("KC_RESTART"))
        .map(|v| v.split(';').next().unwrap_or(v).to_owned())
}

/// Parse the `action` attribute of the first `<form>` element in an HTML page.
fn parse_form_action(html: &str) -> Option<String> {
    // Keycloak login form: <form id="kc-form-login" ... action="...">
    let marker = "action=\"";
    let start = html.find(marker)?;
    let rest = &html[start + marker.len()..];
    let end = rest.find('"')?;
    let raw = &rest[..end];
    // Keycloak HTML-encodes `&` as `&amp;` in form actions
    Some(raw.replace("&amp;", "&"))
}
