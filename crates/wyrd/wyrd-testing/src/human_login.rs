//! Provider-backed human login steps shared by the CLI and SDK journeys.
//!
//! Every journey that needs a saved Wyrd user login drives the same served
//! path a person does: a tenant administrator stages, tests, and activates the
//! public Keycloak `wyrd-human` connection; the CLI handoff begins at
//! `POST /auth/cli-handoffs`; the fixture user signs in at the returned
//! provider URL; the provider's return reaches the server's common callback;
//! and the handoff is claimed with its verifier. [`HumanSso::save_login`]
//! then stores the claimed credential exactly as `wyrd auth login` does, so
//! the Rust, Python, and TypeScript journeys exercise the CLI-established
//! record without each re-implementing the handoff.
//!
//! These helpers need the identity lane's Keycloak; every caller is an
//! ignored journey that lane selects.

use std::path::Path;
use std::time::Duration;

use serde_json::{Value, json};
use url::Url;
use wyrd_client::auth::TokenExchange;
use wyrd_client::saved_login::{SavedLogin, SavedLoginState, SavedLogins, canonical_origin};
use wyrd_spec::auth::{CliHandoffClaim, CliHandoffProof, CliLogin, SecretBearer};
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

    /// Complete one CLI handoff for `tenant` as `username`: begin it, sign in
    /// at its provider URL, and claim it with its verifier.
    ///
    /// # Panics
    /// Panics when any step fails or the claim never completes.
    pub async fn cli_login(&self, tenant: &str, username: &str, password: &str) -> CliLogin {
        let tenant: TenantSlug = tenant.parse().expect("tenant route key parses");
        let handoff = self
            .exchange
            .begin_cli_handoff(&tenant)
            .await
            .expect("the CLI handoff begins");
        let login_url: Url = handoff
            .login_url
            .as_str()
            .parse()
            .expect("login URL parses");
        self.sign_in(&login_url, username, password).await;
        let proof = CliHandoffProof {
            tenant_route_key: tenant,
            poll_verifier: handoff.poll_verifier,
        };
        for _ in 0..30 {
            match self
                .exchange
                .claim_cli_handoff(handoff.handoff_id, &proof)
                .await
                .expect("the handoff claims")
            {
                CliHandoffClaim::Complete(login) => return login,
                CliHandoffClaim::Pending { .. } => tokio::time::sleep(Duration::from_secs(1)).await,
            }
        }
        panic!("the CLI handoff never completed");
    }

    /// Log `username` in to `tenant` through the CLI handoff and save the
    /// credential under `config_home`, exactly as `wyrd auth login` does.
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
        let login = self.cli_login(tenant, username, password).await;
        let record = SavedLogin::from_cli_login(
            self.origin(),
            tenant.parse().expect("tenant route key parses"),
            login,
        );
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

    /// Make this server's saved login for `tenant` under `config_home` stale;
    /// returns the generation the renewal starts from.
    ///
    /// # Panics
    /// Panics when the login is missing or not ready, or cannot be saved.
    #[must_use]
    pub fn expire_saved(&self, config_home: &Path, tenant: &str) -> u64 {
        expire_saved_access(config_home, &self.origin(), tenant)
    }

    /// Generation of this server's saved login for `tenant` under
    /// `config_home`.
    ///
    /// # Panics
    /// Panics when the login is missing.
    #[must_use]
    pub fn saved_generation(&self, config_home: &Path, tenant: &str) -> u64 {
        saved_login(config_home, &self.origin(), tenant).generation
    }

    /// Revoke the server-side refresh chain of this server's saved login for
    /// `tenant` under `config_home`, keeping the record, as another device's
    /// logout would.
    ///
    /// # Panics
    /// Panics when the login is not ready or the server refuses.
    pub async fn revoke_saved(&self, config_home: &Path, tenant: &str) {
        let login = saved_login(config_home, &self.origin(), tenant);
        let SavedLoginState::Ready { refresh_token, .. } = login.state else {
            panic!("the saved login is ready: {:?}", login.summary());
        };
        self.revoke(&refresh_token).await;
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

/// The saved-login store under the Wyrd configuration directory
/// `config_home`, the one `WYRD_CONFIG_HOME` names.
#[must_use]
pub fn saved_logins(config_home: &Path) -> SavedLogins {
    SavedLogins::at(config_home.join("logins"))
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
/// renews it; returns the generation the renewal starts from.
///
/// # Panics
/// Panics when the record is missing or not ready, or cannot be saved.
#[must_use]
pub fn expire_saved_access(config_home: &Path, origin: &str, tenant: &str) -> u64 {
    let mut record = saved_login(config_home, origin, tenant);
    let SavedLoginState::Ready {
        access_expires_at, ..
    } = &mut record.state
    else {
        panic!("saved login is ready: {:?}", record.summary());
    };
    *access_expires_at = chrono::Utc::now() - chrono::Duration::minutes(1);
    saved_logins(config_home)
        .save(record)
        .expect("stale login saves");
    saved_login(config_home, origin, tenant).generation
}
