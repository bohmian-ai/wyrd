//! Human OIDC login initiation HTTP adapter.

use axum::extract::{Extension, Query, State};
use axum::http::{HeaderMap, header};
use axum::response::{IntoResponse, Redirect, Response};
use serde::Deserialize;
use wyrd_spec::DataTenantId;
use wyrd_spec::auth::IssuerUrl;
use wyrd_spec::error::{WyrdError, WyrdProblem};
use wyrd_spec::ids::TenantSlug;
use wyrd_spec::request_id::RequestId;

use crate::auth::auth_not_configured;
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
#[utoipa::path(
    get,
    path = "/auth/login",
    params(("issuer" = String, Query, description = "Trusted issuer URL to begin login against")),
    responses(
        (status = 303, description = "Redirect to the trusted issuer's authorization endpoint"),
        (status = 400, description = "The deployment has no public origin, so no callback URL \
          exists (WYRD_SPEC_400_VALIDATION)", body = WyrdProblem),
        (status = 401, description = "The host names no tenant, or the issuer is not trusted by it \
          (WYRD_AUTH_401_INVALID_TOKEN)", body = WyrdProblem),
        (status = 503, description = "The auth backend is unavailable \
          (WYRD_AUTH_503_VERIFY_UNAVAILABLE)", body = WyrdProblem)
    ),
    // No session exists yet at this operation, so it clears the document-wide
    // requirement instead of inheriting it.
    security(()),
    tag = "Auth"
)]
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

/// Build the redirect for a login attempt, separated from the caller so every
/// refusal is logged once at one place.
///
/// The provider is sent back to the deployment's configured callback — the
/// URL tenants register and candidate testing proved — whatever `Host` or
/// `X-Forwarded-Proto` the request carried; the host only selects the tenant.
///
/// # Errors
/// Returns [`WyrdErrorResponse`] when auth is not configured, when the
/// deployment has no public origin, when the issuer is not the tenant's Active
/// human connection, when its authorization endpoint cannot be discovered, or
/// when the login state cannot be persisted.
async fn try_initiate_login(
    state: &AppState,
    headers: &HeaderMap,
    query: &LoginQuery,
    tenant_id: DataTenantId,
) -> Result<Response, WyrdErrorResponse> {
    let connections = state
        .auth
        .human_connections
        .as_ref()
        .ok_or_else(auth_not_configured)?;
    let init = connections
        .begin_login(tenant_id, &query.issuer)
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
    use super::{LoginQuery, resolve_login_tenant, try_initiate_login};
    use axum::body::to_bytes;
    use axum::http::{HeaderMap, HeaderValue, header};
    use std::sync::Arc;
    use url::Url;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};
    use wyrd_auth::connections::HumanConnections;
    use wyrd_auth_oidc::ScreenedHttp;
    use wyrd_dev_fixtures::pg::{PgFixture, seed_active_human_connection};
    use wyrd_spec::auth::IssuerUrl;
    use wyrd_storage::{BackendSigner, LocalSigner, StorageHandle};

    use crate::components::auth::ServerAuth;

    /// Login sends the provider back to the deployment's configured callback,
    /// never to one derived from the request's `Host` or `X-Forwarded-Proto`.
    ///
    /// # Panics
    /// Panics when the fixture cannot be seeded, login initiation fails, or
    /// the authorization URL carries any other `redirect_uri`.
    #[tokio::test]
    async fn login_uses_the_configured_callback_despite_request_headers() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let provider = MockServer::start().await;
        let issuer = provider.uri();
        Mock::given(method("GET"))
            .and(path("/.well-known/openid-configuration"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "issuer": issuer,
                "authorization_endpoint": format!("{issuer}/authorize"),
                "token_endpoint": format!("{issuer}/token"),
                "jwks_uri": format!("{issuer}/jwks"),
                "id_token_signing_alg_values_supported": ["EdDSA"],
            })))
            .mount(&provider)
            .await;
        let mut conn = fixture.tenant_conn().await.expect("tenant conn opens");
        let binding = seed_active_human_connection(&mut conn)
            .await
            .expect("connection seeds");
        conn.commit().await.expect("seed commits");
        sqlx::query(
            "UPDATE wyrd.auth_human_connections SET issuer_url = $1 WHERE connection_id = $2",
        )
        .bind(&issuer)
        .bind(binding.connection_id)
        .execute(
            &fixture
                .superuser_pool()
                .await
                .expect("superuser pool opens"),
        )
        .await
        .expect("connection points at the mock provider");
        let origin = Url::parse("https://wyrd.example.com").expect("origin parses");
        let tempdir = tempfile::tempdir().expect("login storage tempdir");
        let signer = LocalSigner::new(tempdir.path().to_owned()).expect("local signer creates");
        let state = crate::test_support::test_app_state(
            Arc::new(crate::postgres::ServerPostgres::from_parts(
                fixture.wyrd_postgres().clone(),
                fixture.vala_postgres().clone(),
            )),
            Arc::new(StorageHandle::new(BackendSigner::Local(signer))),
            crate::test_support::test_catalog().await,
        )
        .with_auth(ServerAuth {
            human_connections: Some(HumanConnections::new(
                fixture.wyrd_postgres().clone(),
                None,
                ScreenedHttp::allowing_internal(),
                Some(&origin),
            )),
            ..ServerAuth::default()
        });
        let mut headers = HeaderMap::new();
        headers.insert(
            header::HOST,
            HeaderValue::from_static("attacker.example.net"),
        );
        headers.insert("x-forwarded-proto", HeaderValue::from_static("http"));
        headers.insert(header::ACCEPT, HeaderValue::from_static("application/json"));

        let response = try_initiate_login(
            &state,
            &headers,
            &LoginQuery {
                issuer: IssuerUrl::new(&issuer).expect("issuer is valid"),
            },
            fixture.data_tenant_id(),
        )
        .await
        .expect("login initiates");

        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body reads");
        let init: serde_json::Value = serde_json::from_slice(&body).expect("body is JSON");
        let authorization_url = Url::parse(
            init["authorization_url"]
                .as_str()
                .expect("authorization URL present"),
        )
        .expect("authorization URL parses");
        let redirect_uris: Vec<String> = authorization_url
            .query_pairs()
            .filter(|(key, _)| key == "redirect_uri")
            .map(|(_, value)| value.into_owned())
            .collect();
        assert_eq!(
            redirect_uris,
            vec!["https://wyrd.example.com/auth/callback"]
        );
    }

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
