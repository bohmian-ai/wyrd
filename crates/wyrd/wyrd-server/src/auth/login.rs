//! Human OIDC login initiation and login-state storage.

use std::time::Duration;

use axum::extract::{Query, State};
use axum::http::{HeaderMap, header};
use axum::response::{IntoResponse, Redirect, Response};
use base64::Engine;
use chrono::Utc;
use rand::RngCore;
use secrecy::{ExposeSecret, SecretString};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use url::Url;
use wyrd_auth_oidc::{OidcProvider, TrustedIssuer};
use wyrd_spec::DataTenantId;
use wyrd_spec::auth::{AbsoluteUrl, IssuerUrl, LoginInitResponse};
use wyrd_spec::error::WyrdError;
use wyrd_spec::ids::TenantSlug;
use wyrd_sql::queries::auth::{insert_login_state, take_login_state};
use wyrd_sql::{SqlError, TenantConn};

use crate::error::WyrdErrorResponse;
use crate::state::AppState;

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

/// Login initiation query parameters.
#[derive(Debug, Clone, Deserialize)]
pub struct LoginQuery {
    /// Trusted issuer URL.
    pub issuer: IssuerUrl,
}

/// Handler for `GET /auth/login`.
#[tracing::instrument(level = "debug", skip(state, headers), fields(issuer = %query.issuer))]
pub async fn login(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<LoginQuery>,
) -> Result<Response, WyrdErrorResponse> {
    let tenant_id = resolve_login_tenant(&state, &headers).await?;
    let trusted = trusted_issuer(&state, tenant_id, &query.issuer)?;
    let provider = discover_provider(trusted).await?;
    let redirect_uri = callback_redirect_uri(&headers)?;
    let state_key = auth_state_key();
    let code_verifier = pkce_verifier();
    let nonce = auth_nonce();
    let authz_url = build_authorization_url(
        &provider,
        trusted,
        &redirect_uri,
        &state_key,
        code_verifier.expose_secret(),
        &nonce,
    )?;
    let init = LoginInitResponse {
        authorization_url: AbsoluteUrl::new(authz_url.as_str().to_owned())
            .map_err(|_| invalid_token("authorization URL is invalid"))?,
        state: state_key.clone(),
    };
    let store = PgLoginStateStore::new(state.pool.clone());
    store
        .put(
            tenant_id,
            &state_key,
            LoginStateEntry {
                code_verifier,
                nonce,
                issuer: query.issuer.to_string(),
                redirect_uri,
            },
            LOGIN_STATE_TTL,
        )
        .await
        .map_err(sql_error)?;

    if wants_json(&headers) {
        Ok(axum::Json(init).into_response())
    } else {
        Ok(Redirect::to(authz_url.as_str()).into_response())
    }
}

const LOGIN_STATE_TTL: Duration = Duration::from_secs(300);

fn wants_json(headers: &HeaderMap) -> bool {
    headers
        .get(header::ACCEPT)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.contains("application/json"))
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

fn callback_redirect_uri(headers: &HeaderMap) -> Result<String, WyrdErrorResponse> {
    let host = headers
        .get(header::HOST)
        .and_then(|value| value.to_str().ok())
        .ok_or_else(|| invalid_token("request host header is missing"))?;
    let host = host.split(':').next().unwrap_or(host);
    let scheme = headers
        .get("x-forwarded-proto")
        .and_then(|value| value.to_str().ok())
        .filter(|scheme| matches!(*scheme, "http" | "https"))
        .unwrap_or("http");
    Ok(format!("{scheme}://{host}/auth/callback"))
}

async fn resolve_login_tenant(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<DataTenantId, WyrdErrorResponse> {
    if let Some(slug) = tenant_slug_from_host(headers)
        && let Some(tenant) = resolve_tenant_slug(&state.pool, &slug).await?
    {
        return Ok(tenant);
    }

    Err(invalid_token(
        "tenant could not be resolved for auth request",
    ))
}

fn tenant_slug_from_host(headers: &HeaderMap) -> Option<TenantSlug> {
    let host = headers.get(header::HOST)?.to_str().ok()?;
    let host = host.split(':').next().unwrap_or(host);
    let mut segments = host.split('.').filter(|segment| !segment.is_empty());
    let first = segments.next()?;
    let second = segments.next()?;
    if first == "localhost" || second == "localhost" {
        return None;
    }
    TenantSlug::new(first.to_owned()).ok()
}

async fn resolve_tenant_slug(
    pool: &sqlx::PgPool,
    slug: &TenantSlug,
) -> Result<Option<DataTenantId>, WyrdErrorResponse> {
    wyrd_sql::queries::platform::tenant_resolver::resolve_by_slug_for_app(pool, slug)
        .await
        .map_err(sql_error)
}

fn trusted_issuer<'a>(
    state: &'a AppState,
    tenant_id: DataTenantId,
    issuer: &IssuerUrl,
) -> Result<&'a TrustedIssuer, WyrdErrorResponse> {
    let registry = state.trusted_issuer_registry.as_ref().ok_or_else(|| {
        WyrdErrorResponse::from(WyrdError::Internal {
            message: "trusted issuer registry is not configured".to_owned(),
            details: serde_json::json!({}),
        })
    })?;
    registry
        .get(&tenant_id, issuer)
        .ok_or_else(|| invalid_token("issuer is not trusted for the resolved tenant"))
}

async fn discover_provider(trusted: &TrustedIssuer) -> Result<Url, WyrdErrorResponse> {
    let issuer_url = Url::parse(trusted.issuer.as_str()).map_err(|_| {
        WyrdErrorResponse::from(WyrdError::DiscoveryUnavailable {
            message: "trusted issuer URL could not be parsed".to_owned(),
            details: serde_json::json!({}),
        })
    })?;
    let provider = OidcProvider::discover(issuer_url, reqwest::Client::new())
        .await
        .map_err(|error| {
            tracing::warn!(error = %error, "OIDC discovery failed");
            WyrdErrorResponse::from(WyrdError::DiscoveryUnavailable {
                message: "OIDC discovery unavailable".to_owned(),
                details: serde_json::json!({}),
            })
        })?;
    Ok(provider.metadata.authorization_endpoint)
}

fn build_authorization_url(
    authorization_endpoint: &Url,
    trusted: &TrustedIssuer,
    redirect_uri: &str,
    state: &str,
    code_verifier: &str,
    nonce: &str,
) -> Result<Url, WyrdErrorResponse> {
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
    Ok(url)
}

fn invalid_token(message: &str) -> WyrdErrorResponse {
    WyrdErrorResponse::from(WyrdError::InvalidToken {
        message: message.to_owned(),
        details: serde_json::json!({}),
    })
}

fn sql_error(error: impl Into<SqlError>) -> WyrdErrorResponse {
    let error = error.into();
    tracing::warn!(error = %error, "OIDC login SQL unavailable");
    WyrdErrorResponse::from(WyrdError::AuthVerifyUnavailable {
        message: "auth backend unavailable".to_owned(),
        details: serde_json::json!({ "retry_after_seconds": 1 }),
    })
}

#[cfg(test)]
mod tests {
    use super::{LoginStateEntry, PgLoginStateStore, resolve_login_tenant};
    use axum::http::{HeaderMap, HeaderValue, header};
    use secrecy::{ExposeSecret, SecretString};
    use std::sync::Arc;
    use std::time::Duration;
    use wyrd_storage::{BackendSigner, LocalSigner, StorageHandle};

    use crate::state::AppState;

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
                Duration::from_secs(300),
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

        let consumed = store
            .take(tenant, "state-expired")
            .await
            .expect("expired state read succeeds");
        assert!(consumed.is_none());
    }

    #[tokio::test]
    async fn login_tenant_resolution_rejects_non_tenant_host_even_with_query_fallback_context() {
        let fixture = wyrd_dev_fixtures::pg::PgFixture::start()
            .await
            .expect("fixture starts");
        let storage_root = fixture.tempdir_path().join("storage");
        std::fs::create_dir_all(&storage_root).expect("storage root creates");
        let signer = LocalSigner::new(storage_root).expect("local signer creates");
        let state = AppState::new(
            fixture.app_pool().clone(),
            None,
            Arc::new(StorageHandle::new(BackendSigner::Local(signer))),
            crate::test_support::test_catalog().await,
        );
        let mut headers = HeaderMap::new();
        headers.insert(header::HOST, HeaderValue::from_static("localhost"));

        let error = resolve_login_tenant(&state, &headers)
            .await
            .expect_err("localhost must not resolve by query fallback");

        assert_eq!(error.0.code(), "WYRD_AUTH_401_INVALID_TOKEN");
    }
}
