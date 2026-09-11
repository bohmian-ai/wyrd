//! Human OIDC login initiation and login-state storage.

use std::time::Duration;

use base64::Engine;
use chrono::Utc;
use rand::RngCore;
use secrecy::{ExposeSecret, SecretString};
use sha2::{Digest, Sha256};
use url::Url;
use wyrd_auth_oidc::{OidcProvider, TrustedIssuer};
use wyrd_spec::DataTenantId;
use wyrd_spec::auth::{AbsoluteUrl, IssuerUrl, LoginInitResponse};
use wyrd_spec::error::WyrdError;
use wyrd_sql::queries::auth::{insert_login_state, take_login_state};
use wyrd_sql::{SqlError, TenantConn};

/// Lifetime of a persisted login-state row: the browser must complete the `IdP`
/// round-trip and hit `/auth/callback` within this window or the state is gone.
const LOGIN_STATE_TTL: Duration = Duration::from_mins(5);

/// Short-lived state row stored server-side during the OIDC login flow.
#[derive(Debug, Clone)]
pub struct LoginStateEntry {
    /// Server-generated PKCE verifier.
    pub code_verifier: SecretString,
    /// Server-generated nonce.
    pub nonce: String,
    /// Trusted issuer URL.
    pub issuer: String,
    /// Callback redirect URI.
    pub redirect_uri: String,
}

/// Postgres-backed login-state store.
#[derive(Debug, Clone)]
pub struct PgLoginStateStore {
    pool: sqlx::PgPool,
}

impl PgLoginStateStore {
    /// Build a store from the runtime app pool.
    #[must_use]
    pub fn new(pool: sqlx::PgPool) -> Self {
        Self { pool }
    }

    /// Store one login-state row.
    ///
    /// # Panics
    ///
    /// Panics only if the provided TTL cannot fit in `chrono::Duration`.
    pub async fn put(
        &self,
        tenant: DataTenantId,
        state: &str,
        entry: LoginStateEntry,
        ttl: Duration,
    ) -> Result<(), SqlError> {
        let mut conn = TenantConn::acquire(&self.pool, tenant).await?;
        let expires_at = Utc::now()
            + chrono::Duration::from_std(ttl)
                .expect("login-state TTL is bounded and must fit chrono duration");
        insert_login_state(
            &mut conn,
            state,
            entry.code_verifier.expose_secret(),
            &entry.nonce,
            &entry.issuer,
            &entry.redirect_uri,
            expires_at,
        )
        .await?;
        conn.commit().await
    }

    /// Consume a login-state row exactly once.
    pub async fn take(
        &self,
        tenant: DataTenantId,
        state: &str,
    ) -> Result<Option<LoginStateEntry>, SqlError> {
        let mut conn = TenantConn::acquire(&self.pool, tenant).await?;
        let row = take_login_state(&mut conn, state).await?;
        conn.commit().await?;
        Ok(row.map(|row| LoginStateEntry {
            code_verifier: SecretString::from(row.code_verifier),
            nonce: row.nonce,
            issuer: row.issuer,
            redirect_uri: row.redirect_uri,
        }))
    }
}

/// Resolve the `IdP` authorization URL and persist login state for the callback.
pub async fn prepare_login(
    pool: &sqlx::PgPool,
    tenant_id: DataTenantId,
    trusted: &TrustedIssuer,
    issuer: &IssuerUrl,
    redirect_uri: String,
) -> Result<LoginInitResponse, WyrdError> {
    let authorization_endpoint = discover_authorization_endpoint(trusted).await?;
    let state_key = auth_state_key();
    let code_verifier = pkce_verifier();
    let nonce = auth_nonce();
    let authz_url = build_authorization_url(
        &authorization_endpoint,
        trusted,
        &redirect_uri,
        &state_key,
        code_verifier.expose_secret(),
        &nonce,
    );
    let init = LoginInitResponse {
        authorization_url: AbsoluteUrl::new(authz_url.as_str().to_owned())
            .map_err(|_| invalid_token("authorization URL is invalid"))?,
        state: state_key.clone(),
    };
    PgLoginStateStore::new(pool.clone())
        .put(
            tenant_id,
            &state_key,
            LoginStateEntry {
                code_verifier,
                nonce,
                issuer: issuer.to_string(),
                redirect_uri,
            },
            LOGIN_STATE_TTL,
        )
        .await
        .map_err(sql_error)?;
    Ok(init)
}

/// Resolve the trusted issuer's OIDC `authorization_endpoint` via discovery.
///
/// # Errors
///
/// Returns [`WyrdError::DiscoveryUnavailable`] when another Rustls provider
/// already owns the process or the issuer URL, metadata request, or response is
/// invalid. Cancellation can interrupt discovery without persisting state.
///
pub async fn discover_authorization_endpoint(trusted: &TrustedIssuer) -> Result<Url, WyrdError> {
    let issuer_url =
        Url::parse(trusted.issuer.as_str()).map_err(|_| WyrdError::DiscoveryUnavailable {
            message: "trusted issuer URL could not be parsed".to_owned(),
            details: serde_json::json!({}),
        })?;
    wyrd_tls::install_crypto_provider().map_err(|_| WyrdError::DiscoveryUnavailable {
        message: "OIDC TLS provider initialization failed".to_owned(),
        details: serde_json::json!({}),
    })?;
    let provider = OidcProvider::discover(issuer_url, reqwest::Client::new())
        .await
        .map_err(|error| {
            tracing::warn!(error = %error, "OIDC discovery failed");
            WyrdError::DiscoveryUnavailable {
                message: "OIDC discovery unavailable".to_owned(),
                details: serde_json::json!({}),
            }
        })?;
    Ok(provider.metadata.authorization_endpoint)
}

fn auth_state_key() -> String {
    random_b64url(32)
}

fn auth_nonce() -> String {
    random_b64url(32)
}

fn pkce_verifier() -> SecretString {
    SecretString::from(random_b64url(48))
}

fn pkce_challenge(verifier: &str) -> String {
    let digest = Sha256::digest(verifier.as_bytes());
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(digest)
}

fn random_b64url(bytes: usize) -> String {
    let mut buf = vec![0_u8; bytes];
    rand::rng().fill_bytes(&mut buf);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(buf)
}

fn build_authorization_url(
    authorization_endpoint: &Url,
    trusted: &TrustedIssuer,
    redirect_uri: &str,
    state: &str,
    code_verifier: &str,
    nonce: &str,
) -> Url {
    let mut url = authorization_endpoint.clone();
    let challenge = pkce_challenge(code_verifier);
    let mut query = url.query_pairs_mut();
    query.append_pair("response_type", "code");
    query.append_pair("client_id", &trusted.client_id);
    query.append_pair("redirect_uri", redirect_uri);
    query.append_pair("scope", "openid profile email");
    query.append_pair("code_challenge", &challenge);
    query.append_pair("code_challenge_method", "S256");
    query.append_pair("state", state);
    query.append_pair("nonce", nonce);
    drop(query);
    url
}

fn invalid_token(message: &str) -> WyrdError {
    WyrdError::InvalidToken {
        message: message.to_owned(),
        details: serde_json::json!({}),
    }
}

fn sql_error(error: impl Into<SqlError>) -> WyrdError {
    let error = error.into();
    tracing::warn!(error = %error, "OIDC login SQL unavailable");
    WyrdError::AuthVerifyUnavailable {
        message: "auth backend unavailable".to_owned(),
        details: serde_json::json!({ "retry_after_seconds": 1 }),
    }
}

#[cfg(test)]
mod pg_tests {
    use super::{LoginStateEntry, PgLoginStateStore};
    use secrecy::{ExposeSecret, SecretString};
    use std::time::Duration;

    #[tokio::test]
    async fn login_state_store_consumes_single_use_rows_once() {
        let fixture = wyrd_dev_fixtures::pg::PgFixture::start()
            .await
            .expect("fixture starts");
        let tenant = fixture.data_tenant_id();
        let store_a = PgLoginStateStore::new(fixture.app_pool().clone());
        let store_b = PgLoginStateStore::new(fixture.app_pool().clone());

        store_a
            .put(
                tenant,
                "state-1",
                LoginStateEntry {
                    code_verifier: SecretString::from("verifier-1".to_owned()),
                    nonce: "nonce-1".to_owned(),
                    issuer: "https://idp.example.com/realms/acme".to_owned(),
                    redirect_uri: "https://app.example.com/auth/callback".to_owned(),
                },
                Duration::from_mins(5),
            )
            .await
            .expect("state inserts");

        let consumed = store_b.take(tenant, "state-1").await.expect("state takes");
        let consumed = consumed.expect("state exists");
        assert_eq!(consumed.nonce, "nonce-1");
        assert_eq!(consumed.issuer, "https://idp.example.com/realms/acme");
        assert_eq!(
            consumed.redirect_uri,
            "https://app.example.com/auth/callback"
        );
        assert_eq!(consumed.code_verifier.expose_secret(), "verifier-1");

        let replay = store_a
            .take(tenant, "state-1")
            .await
            .expect("state replay reads");
        assert!(replay.is_none());
    }

    #[tokio::test]
    async fn expired_login_state_returns_none() {
        let fixture = wyrd_dev_fixtures::pg::PgFixture::start()
            .await
            .expect("fixture starts");
        let tenant = fixture.data_tenant_id();
        let store = PgLoginStateStore::new(fixture.app_pool().clone());

        store
            .put(
                tenant,
                "state-expired",
                LoginStateEntry {
                    code_verifier: SecretString::from("verifier-expired".to_owned()),
                    nonce: "nonce-expired".to_owned(),
                    issuer: "https://idp.example.com/realms/acme".to_owned(),
                    redirect_uri: "https://app.example.com/auth/callback".to_owned(),
                },
                Duration::from_secs(0),
            )
            .await
            .expect("state inserts");

        tokio::time::sleep(Duration::from_millis(50)).await;

        let consumed = store
            .take(tenant, "state-expired")
            .await
            .expect("expired state read succeeds");
        assert!(consumed.is_none());
    }
}
