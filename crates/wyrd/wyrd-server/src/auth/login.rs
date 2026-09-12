//! Human OIDC login initiation HTTP adapter.

use axum::extract::{Extension, Query, State};
use axum::http::{HeaderMap, header};
use axum::response::{IntoResponse, Redirect, Response};
use serde::Deserialize;
use wyrd_spec::DataTenantId;
use wyrd_spec::auth::IssuerUrl;
use wyrd_spec::error::WyrdError;
use wyrd_spec::ids::TenantSlug;
use wyrd_spec::request_id::RequestId;

use crate::http::error::WyrdErrorResponse;
use crate::state::AppState;

/// Login initiation query parameters.
#[derive(Debug, Clone, Deserialize)]
pub struct LoginQuery {
    /// Trusted issuer URL.
    pub issuer: IssuerUrl,
}

/// Handler for `GET /auth/login`.
///
/// Login initiation evaluates no principal permission — it answers "who is
/// calling?", not "may they do this?" — so it appends no canonical audit event.
/// A refused attempt is recorded as structured diagnostics instead, and the
/// durable record of the session it goes on to establish belongs to the
/// authentication tables, not to the authorization chain.
#[tracing::instrument(level = "debug", skip(state, headers, maybe_request_id), fields(issuer = %query.issuer))]
pub async fn login(
    State(state): State<AppState>,
    maybe_request_id: Option<Extension<RequestId>>,
    headers: HeaderMap,
    Query(query): Query<LoginQuery>,
) -> Result<Response, WyrdErrorResponse> {
    let request_id = maybe_request_id
        .map(|Extension(id)| id)
        .unwrap_or_else(RequestId::now_v7);
    let tenant_id = resolve_login_tenant(&state, &headers)
        .await
        .inspect_err(|_| {
            tracing::info!(
                request_id = %request_id,
                issuer = %query.issuer,
                "login attempt refused: tenant did not resolve"
            );
        })?;
    try_initiate_login(&state, &headers, &query, tenant_id)
        .await
        .inspect_err(|_| {
            tracing::info!(
                request_id = %request_id,
                issuer = %query.issuer,
                "login initiation refused"
            );
        })
}

async fn try_initiate_login(
    state: &AppState,
    headers: &HeaderMap,
    query: &LoginQuery,
    tenant_id: DataTenantId,
) -> Result<Response, WyrdErrorResponse> {
    let trusted = wyrd_auth::issuer::trusted_issuer(
        state.auth.trusted_issuer_resolver.as_deref(),
        tenant_id,
        &query.issuer,
    )
    .await
    .map_err(WyrdErrorResponse::from)?;
    let redirect_uri = callback_redirect_uri(headers)?;
    let init = wyrd_auth::login::prepare_login(
        state.postgres.app_pool(),
        tenant_id,
        &trusted,
        &query.issuer,
        redirect_uri,
    )
    .await
    .map_err(WyrdErrorResponse::from)?;

    if wants_json(headers) {
        Ok(axum::Json(init).into_response())
    } else {
        Ok(Redirect::to(init.authorization_url.as_str()).into_response())
    }
}

/// Whether the client prefers a JSON response (`Accept: application/json`) over
/// the default `302` redirect to the IdP.
fn wants_json(headers: &HeaderMap) -> bool {
    headers
        .get(header::ACCEPT)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.contains("application/json"))
}

/// Build the `redirect_uri` the IdP sends the browser back to, derived from the
/// request `Host` (and `X-Forwarded-Proto` when present). Must match the value
/// replayed at the token exchange, so it is stored with the login state.
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

/// Resolve the tenant for this login request from the request host subdomain,
/// failing closed with `InvalidToken` when no active tenant matches.
async fn resolve_login_tenant(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<DataTenantId, WyrdErrorResponse> {
    if let Some(slug) = tenant_slug_from_host(headers)
        && let Some(tenant) = resolve_tenant_slug(state.postgres.app_pool(), &slug).await?
    {
        return Ok(tenant);
    }

    Err(invalid_token(
        "tenant could not be resolved for auth request",
    ))
}

/// Extract the tenant slug from the leftmost host label (e.g. `acme` in
/// `acme.wyrd.cloud`). Returns `None` for `localhost`-style hosts that carry no
/// tenant subdomain.
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

/// Look up an active tenant id by slug through the `wyrd_app` pool, mapping a
/// store outage to a fail-closed `503`.
async fn resolve_tenant_slug(
    pool: &sqlx::PgPool,
    slug: &TenantSlug,
) -> Result<Option<DataTenantId>, WyrdErrorResponse> {
    wyrd_sql::queries::platform::tenant_resolver::resolve_by_slug_for_app(pool, slug)
        .await
        .map_err(sql_error)
}

/// Build a `401` [`WyrdError::InvalidToken`] for a login request that cannot be
/// trusted (missing host, unresolvable tenant, untrusted issuer).
fn invalid_token(message: &str) -> WyrdErrorResponse {
    WyrdErrorResponse::from(WyrdError::InvalidToken {
        message: message.to_owned(),
        details: serde_json::json!({}),
    })
}

fn sql_error(error: impl std::fmt::Display) -> WyrdErrorResponse {
    tracing::warn!(error = %error, "OIDC login SQL unavailable");
    WyrdErrorResponse::from(WyrdError::AuthVerifyUnavailable {
        message: "auth backend unavailable".to_owned(),
        details: serde_json::json!({ "retry_after_seconds": 1 }),
    })
}

#[cfg(test)]
mod pg_tests {
    use super::resolve_login_tenant;
    use axum::http::{HeaderMap, HeaderValue, header};
    use std::sync::Arc;
    use wyrd_storage::{BackendSigner, LocalSigner, StorageHandle};

    #[tokio::test]
    async fn login_tenant_resolution_rejects_non_tenant_host_even_with_query_fallback_context() {
        let fixture = wyrd_dev_fixtures::pg::PgFixture::start()
            .await
            .expect("fixture starts");
        let tempdir = tempfile::tempdir().expect("login storage tempdir");
        let storage_root = tempdir.path().join("storage");
        std::fs::create_dir_all(&storage_root).expect("storage root creates");
        let signer = LocalSigner::new(storage_root).expect("local signer creates");
        let postgres = Arc::new(crate::postgres::ServerPostgres::from_parts(
            fixture.wyrd_postgres().clone(),
            fixture.vala_postgres().clone(),
        ));
        let state = crate::test_support::test_app_state(
            postgres,
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
