//! Real OIDC provider fixture for identity e2e tests.
//!
//! `OidcIssuerFixture` wraps a running Keycloak or Dex container and provides
//! helpers to drive human login (Keycloak and Dex), mint workload tokens, and
//! force signing-key rotation (Keycloak only).  The harness smoke test (commit
//! 08) verifies discovery; the journey tests (commit 09) drive the full flows.

use std::sync::Arc;

use url::Url;
use wiremock::MockServer;
use wyrd_auth_oidc::{ProviderMetadata, RelyingParty, ScreenedHttp};
use wyrd_spec::auth::IssuerUrl;

/// Keycloak-specific admin context needed for privileged operations.
#[derive(Debug, Clone)]
pub struct KeycloakAdmin {
    /// Base URL of the Keycloak server (e.g. `http://localhost:18080`).
    pub base_url: String,
    /// Admin username.
    pub username: String,
    /// Admin password.
    pub password: String,
    /// Realm name.
    pub realm: String,
}

/// Result of a driven human login: the `code`, `state`, and optional RFC 9207
/// `iss` query parameters delivered to the redirect URI.
#[derive(Debug, Clone)]
pub struct LoginResult {
    /// Authorization code returned by the IdP.
    pub code: String,
    /// State value echoed back from the authorization request.
    pub state: String,
    /// RFC 9207 authorization-response issuer, when the IdP sent one; a
    /// callback that binds the response issuer must forward it.
    pub iss: Option<String>,
}

/// Connected OIDC provider fixture.
///
/// Construct via [`OidcIssuerFixture::connect`].  Use
/// [`OidcIssuerFixture::with_keycloak_admin`] when Keycloak admin operations are needed.
pub struct OidcIssuerFixture {
    /// Normalized issuer URL (used for `TrustedIssuer.issuer`).
    pub issuer: IssuerUrl,
    provider: Arc<ProviderMetadata>,
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
    /// Panics when another Rustls provider already owns the process, the HTTP
    /// client cannot be built, or discovery fails. This deliberately
    /// infallible test-fixture API converts setup failures into test failures.
    pub async fn connect(issuer_base: &str) -> Self {
        wyrd_tls::install_crypto_provider()
            .expect("Wyrd's AWS-LC provider must own fixture TLS before client construction");
        let http = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .expect("reqwest client builds");

        let issuer = IssuerUrl::new_for_tests(issuer_base);
        let provider = RelyingParty::new(ScreenedHttp::allowing_internal())
            .discover(&issuer)
            .await
            .unwrap_or_else(|e| panic!("OIDC discovery failed for {issuer_base}: {e}"));

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
        &self.provider
    }

    /// Drive a full human login at this provider for a caller-built request.
    ///
    /// Builds the authorization URL from this provider's discovered
    /// endpoint and the given parameters, signs `username` in through
    /// [`provider_sign_in`], and returns the `code`, `state`, and RFC 9207
    /// `iss` the provider redirected to `redirect_uri` with.
    ///
    /// # Panics
    /// Panics when the login flow cannot be completed or the return omits
    /// `code` or `state`.
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
        let mut authz_url = self.provider.authorization_endpoint().url().clone();
        authz_url
            .query_pairs_mut()
            .append_pair("response_type", "code")
            .append_pair("client_id", client_id)
            .append_pair("redirect_uri", redirect_uri.as_str())
            .append_pair("scope", "openid email profile")
            .append_pair("state", state)
            .append_pair("nonce", nonce)
            .append_pair("code_challenge", code_challenge)
            .append_pair("code_challenge_method", "S256");
        let callback_url =
            provider_sign_in(&authz_url, username, password, redirect_uri.as_str()).await;
        let param = |name: &str| {
            callback_url
                .query_pairs()
                .find(|(key, _)| key == name)
                .map(|(_, value)| value.into_owned())
        };
        LoginResult {
            code: param("code").expect("callback URL contains 'code' param"),
            state: param("state").expect("callback URL contains 'state' param"),
            iss: param("iss"),
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
            .token_endpoint()
            .expect("provider exposes a token endpoint")
            .url();

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
        let admin_token = self.admin_token().await;

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
            .bearer_auth(&admin_token)
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

    /// Mint a Keycloak admin access token for one privileged REST call.
    ///
    /// Every privileged fixture operation authenticates the same way — the
    /// master realm's `admin-cli` password grant — so the credential handling
    /// lives here rather than in each operation. The token is deliberately not
    /// cached: it is short-lived, and a long journey that reused a stale one
    /// would fail as an unrelated authorization error.
    ///
    /// # Panics
    /// Panics when admin credentials were never attached with
    /// [`Self::with_keycloak_admin`], the token request fails, or the response
    /// carries no `access_token`.
    async fn admin_token(&self) -> String {
        let admin = self
            .admin
            .as_ref()
            .expect("call with_keycloak_admin before a privileged operation");
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

        token_resp["access_token"]
            .as_str()
            .expect("admin token present")
            .to_owned()
    }

    /// Add or remove `username`'s membership of the realm group `group`.
    ///
    /// The realm's `groups` protocol mapper puts a member's group names in the
    /// ID token, and a Wyrd issuer maps those names onto Wyrd roles. A journey
    /// that needs the provider to grant or withdraw authority therefore moves
    /// the membership the claim is derived from, which is the only lever a
    /// real identity administrator has.
    ///
    /// Keycloak offers no single "set membership" call, so the user and the
    /// group are each resolved by name and the membership is then `PUT` or
    /// `DELETE`d. Both directions are idempotent — a repeated add and a
    /// redundant removal are accepted — so a journey may restore the starting
    /// state unconditionally in its cleanup.
    ///
    /// # Panics
    /// Panics when admin credentials are absent, the user or the group does
    /// not exist in the realm, or the membership call is refused.
    pub async fn set_group_membership(&self, username: &str, group: &str, member: bool) {
        let admin = self
            .admin
            .as_ref()
            .expect("call with_keycloak_admin before set_group_membership");
        let admin_token = self.admin_token().await;
        let realm_url = format!("{}/admin/realms/{}", admin.base_url, admin.realm);

        let users: serde_json::Value = self
            .http
            .get(format!("{realm_url}/users"))
            .query(&[("username", username), ("exact", "true")])
            .bearer_auth(&admin_token)
            .send()
            .await
            .expect("Keycloak user lookup")
            .json()
            .await
            .expect("user lookup response is JSON");
        let user_id = users[0]["id"]
            .as_str()
            .unwrap_or_else(|| panic!("realm user {username} exists: {users}"));

        let groups: serde_json::Value = self
            .http
            .get(format!("{realm_url}/groups"))
            .query(&[("search", group)])
            .bearer_auth(&admin_token)
            .send()
            .await
            .expect("Keycloak group lookup")
            .json()
            .await
            .expect("group lookup response is JSON");
        let group_id = groups[0]["id"]
            .as_str()
            .unwrap_or_else(|| panic!("realm group {group} exists: {groups}"));

        let membership_url = format!("{realm_url}/users/{user_id}/groups/{group_id}");
        let request = if member {
            self.http.put(&membership_url)
        } else {
            self.http.delete(&membership_url)
        };
        let response = request
            .bearer_auth(&admin_token)
            .send()
            .await
            .expect("Keycloak group membership call");
        assert!(
            response.status().is_success(),
            "group membership change failed for {username}/{group}: status={}",
            response.status()
        );
    }
}

/// Sign `username` in at a real provider's HTML login form, starting from a
/// complete `authorization_url`, and return the URL the provider finally
/// redirects the browser to under `redirect_uri` — with every query parameter
/// it added (`code`, `state`, and any RFC 9207 `iss`).
///
/// Acts as a cookie-keeping browser that does not follow redirects on its
/// own: it follows each provider redirect, submits the first login form it
/// reaches once, and stops at the first redirect into `redirect_uri` without
/// requesting it. A provider that redirects there without a form (an existing
/// session or a mock) returns at once. Keycloak names the account field
/// `username` and Dex names it `login`; both are sent and each provider
/// ignores the other.
///
/// # Panics
/// Panics when a request fails, the provider answers with neither a redirect
/// nor a login form, shows the form again after the credentials (a refused
/// sign-in), or never redirects into `redirect_uri`.
pub async fn provider_sign_in(
    authorization_url: &Url,
    username: &str,
    password: &str,
    redirect_uri: &str,
) -> Url {
    wyrd_tls::install_crypto_provider()
        .expect("Wyrd's AWS-LC provider must own fixture TLS before client construction");
    let http = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .expect("reqwest client builds");
    let mut cookies = std::collections::BTreeMap::<String, String>::new();
    let mut current = authorization_url.clone();
    let mut request = http.get(current.clone());
    let mut credentials_sent = false;
    for _ in 0..20 {
        if !cookies.is_empty() {
            let header = cookies
                .iter()
                .map(|(name, value)| format!("{name}={value}"))
                .collect::<Vec<_>>()
                .join("; ");
            request = request.header(reqwest::header::COOKIE, header);
        }
        let response = request.send().await.expect("provider answers");
        for set in response
            .headers()
            .get_all(reqwest::header::SET_COOKIE)
            .iter()
            .filter_map(|value| value.to_str().ok())
        {
            let pair = set.split(';').next().unwrap_or(set);
            if let Some((name, value)) = pair.split_once('=') {
                cookies.insert(name.trim().to_owned(), value.trim().to_owned());
            }
        }
        if let Some(location) = response.headers().get(reqwest::header::LOCATION) {
            let next = current
                .join(location.to_str().expect("Location is valid UTF-8"))
                .expect("Location resolves");
            if next.as_str().starts_with(redirect_uri) {
                return next;
            }
            current = next;
            request = http.get(current.clone());
            continue;
        }
        let status = response.status();
        let html = response.text().await.expect("provider page is text");
        assert!(
            status.is_success() && !credentials_sent,
            "provider sign-in stopped with {status} at {current}:\n{html}"
        );
        let action = parse_form_action(&html)
            .unwrap_or_else(|| panic!("no login form at {current}:\n{html}"));
        current = current.join(&action).expect("form action resolves");
        request = http.post(current.clone()).form(&[
            ("username", username),
            ("login", username),
            ("password", password),
        ]);
        credentials_sent = true;
    }
    panic!("provider sign-in never returned to {redirect_uri}");
}

/// Parse the `action` attribute of the first `<form>` element in an HTML page.
fn parse_form_action(html: &str) -> Option<String> {
    // Keycloak: <form id="kc-form-login" ... action="...">; Dex: <form method="post" action="...">
    let marker = "action=\"";
    let start = html.find(marker)?;
    let rest = &html[start + marker.len()..];
    let end = rest.find('"')?;
    let raw = &rest[..end];
    // Both providers HTML-encode `&` as `&amp;` in form actions
    Some(raw.replace("&amp;", "&"))
}

/// A minimal OIDC discovery document served over HTTP.
///
/// Configuring a Wyrd OIDC connection resolves the issuer's discovery document
/// to derive its JWKS endpoint, under the deployment's address screening. A
/// test that wants to exercise that path needs a real HTTP issuer to point at,
/// and standing one up is test-support rather than something each suite should
/// carry its own mock dependency for.
///
/// The JWKS endpoint it advertises is deliberately derived from the server's
/// own address, so a caller that echoed a request field back instead of
/// resolving discovery would produce a different value.
pub struct DiscoveryFixture {
    /// The running mock issuer. Held so it outlives the fixture's users.
    server: MockServer,
}

impl DiscoveryFixture {
    /// Start an issuer serving its discovery document and an empty key set.
    pub async fn start() -> Self {
        let server = wiremock::MockServer::start().await;
        let issuer = server.uri();
        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .and(wiremock::matchers::path(
                "/.well-known/openid-configuration",
            ))
            .respond_with(
                wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "issuer": issuer,
                    "authorization_endpoint": format!("{issuer}/authorize"),
                    "token_endpoint": format!("{issuer}/token"),
                    "jwks_uri": format!("{issuer}/jwks"),
                    "response_types_supported": ["code"],
                    "subject_types_supported": ["public"],
                    "id_token_signing_alg_values_supported": ["RS256", "EdDSA"],
                })),
            )
            .mount(&server)
            .await;
        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .and(wiremock::matchers::path("/jwks"))
            .respond_with(
                wiremock::ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({ "keys": [] })),
            )
            .mount(&server)
            .await;
        Self { server }
    }

    /// The issuer URL a connection is configured against.
    #[must_use]
    pub fn issuer(&self) -> String {
        self.server.uri()
    }

    /// The JWKS endpoint this issuer's discovery document advertises.
    #[must_use]
    pub fn jwks_uri(&self) -> String {
        format!("{}/jwks", self.server.uri())
    }
}
