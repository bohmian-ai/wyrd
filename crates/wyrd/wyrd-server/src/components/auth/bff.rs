//! The private BFF browser-session channel, `POST /internal/bff/v1/*`.
//!
//! Mounted only when the deployment configures BFF service-key hashes
//! (`WYRD_BFF_SERVICE_KEY_SHA256`). Every call carries the raw deployment key
//! in `x-wyrd-bff-key`; a missing or wrong key is refused with `401` before any
//! store read. The key authorizes only these session operations: tenant comes
//! from verified flow or session state, and every Wyrd API call the BFF makes
//! afterwards is authorized by the session's own access token. The deployment
//! gateway never routes `/internal/` publicly, and these routes are not part
//! of the public OpenAPI document.

use axum::body::Body;
use axum::extract::{Extension, Request, State};
use axum::http::StatusCode;
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use wyrd_auth::browser_sessions::{BrowserSessions, CreatedBrowserSession};
use wyrd_spec::auth::Sha256Hex;
use wyrd_spec::ids::TenantSlug;
use wyrd_spec::request_id::RequestId;

use crate::auth::invalid_token;
use crate::http::error::WyrdErrorResponse;
use crate::state::AppState;

/// Header carrying the raw deployment BFF service key.
const BFF_KEY_HEADER: &str = "x-wyrd-bff-key";

/// The browser-session owner plus the service-key hashes that admit the BFF.
#[derive(Clone, Debug)]
pub struct BffChannel {
    /// Owner of every browser-session record.
    pub sessions: BrowserSessions,
    /// SHA-256 of each accepted raw service key: one, or two during rotation.
    pub key_hashes: Vec<Sha256Hex>,
}

impl BffChannel {
    /// Whether `presented` is an accepted raw service key.
    fn admits(&self, presented: &[u8]) -> bool {
        let hash = Sha256Hex::digest(presented);
        self.key_hashes.contains(&hash)
    }
}

/// Build the private channel router.
///
/// Every route sits behind [`require_bff_key`].
pub fn bff_router(state: &AppState) -> Router<AppState> {
    Router::new()
        .route("/internal/bff/v1/login/options", post(login_options))
        .route("/internal/bff/v1/sessions/complete", post(complete))
        .route("/internal/bff/v1/sessions/api-key", post(api_key))
        .route("/internal/bff/v1/sessions/read", post(read))
        .route("/internal/bff/v1/sessions/authority", post(authority))
        .route("/internal/bff/v1/sessions/logout", post(logout))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            require_bff_key,
        ))
}

/// Refuse any call that does not present an accepted BFF service key, before
/// the handler (and so any store read) runs.
async fn require_bff_key(
    State(state): State<AppState>,
    request: Request<Body>,
    next: Next,
) -> Response {
    let admitted = state.auth.bff.as_ref().is_some_and(|channel| {
        request
            .headers()
            .get(BFF_KEY_HEADER)
            .is_some_and(|value| channel.admits(value.as_bytes()))
    });
    if !admitted {
        tracing::info!("bff channel call refused: missing or unknown service key");
        return invalid_token("the BFF service key is missing or not accepted").into_response();
    }
    next.run(request).await
}

/// The channel owner; present whenever the router is mounted.
///
/// # Errors
/// Returns `401` when the channel is not configured.
fn sessions(state: &AppState) -> Result<&BrowserSessions, WyrdErrorResponse> {
    state
        .auth
        .bff
        .as_ref()
        .map(|channel| &channel.sessions)
        .ok_or_else(|| invalid_token("the BFF channel is not configured"))
}

/// The request id the protected edge attached, or a fresh one.
fn request_id(id: Option<Extension<RequestId>>) -> RequestId {
    id.map_or_else(RequestId::now_v7, |Extension(id)| id)
}

/// `login/options` body.
#[derive(Deserialize)]
struct LoginOptionsRequest {
    /// Route key of the tenant the sign-in page is for.
    tenant_route_key: TenantSlug,
}

/// `login/options` reply.
#[derive(Serialize)]
struct LoginOptionsResponse {
    /// Whether the tenant offers SSO; an unknown tenant answers `false`.
    sso: bool,
}

/// `POST /internal/bff/v1/login/options` — whether a tenant offers SSO.
///
/// # Errors
/// Returns `503` when the store is unavailable.
#[tracing::instrument(level = "debug", skip_all, fields(tenant_route_key = %request.tenant_route_key))]
async fn login_options(
    State(state): State<AppState>,
    Json(request): Json<LoginOptionsRequest>,
) -> Result<Json<LoginOptionsResponse>, WyrdErrorResponse> {
    let sso = sessions(&state)?
        .sso_available(&request.tenant_route_key)
        .await?;
    Ok(Json(LoginOptionsResponse { sso }))
}

/// `sessions/complete` body.
#[derive(Deserialize)]
struct CompleteRequest {
    /// Raw value of the BFF's `HttpOnly` flow cookie.
    flow_id: String,
    /// Fresh random session CSRF token the BFF generated.
    csrf_token: String,
}

/// `sessions/api-key` body.
#[derive(Deserialize)]
struct ApiKeyRequest {
    /// Route key of the tenant the person is signing in to.
    tenant_route_key: TenantSlug,
    /// The existing tenant API key the person presented.
    api_key: String,
    /// Fresh random session CSRF token the BFF generated.
    csrf_token: String,
}

/// Reply to a created session; the raw id is returned exactly once.
#[derive(Serialize)]
struct CreatedResponse {
    /// Raw session id for the opaque session cookie.
    session_id: String,
    /// Route key of the session's tenant.
    tenant_key: String,
    /// Absolute session expiry, RFC 3339.
    expires_at: DateTime<Utc>,
}

impl From<CreatedBrowserSession> for CreatedResponse {
    fn from(created: CreatedBrowserSession) -> Self {
        Self {
            session_id: created.session_id.expose_secret().to_owned(),
            tenant_key: created.tenant_key.to_string(),
            expires_at: created.expires_at,
        }
    }
}

/// `POST /internal/bff/v1/sessions/complete` — redeem a completed SSO login
/// for this browser's flow cookie into a new session.
///
/// # Errors
/// Returns `400` for a missing, mismatched, expired, or replayed flow (no
/// session is created) and `503` when the store is unavailable.
#[tracing::instrument(level = "debug", skip_all)]
async fn complete(
    State(state): State<AppState>,
    Json(request): Json<CompleteRequest>,
) -> Result<(StatusCode, Json<CreatedResponse>), WyrdErrorResponse> {
    let created = sessions(&state)?
        .complete(
            &SecretString::from(request.flow_id),
            &SecretString::from(request.csrf_token),
        )
        .await?;
    Ok((StatusCode::CREATED, Json(created.into())))
}

/// `POST /internal/bff/v1/sessions/api-key` — exchange an existing tenant API
/// key for an OIDC-off session.
///
/// # Errors
/// Returns `401` for an unusable key or one of another tenant and `503` when
/// the store is unavailable.
#[tracing::instrument(level = "debug", skip_all, fields(tenant_route_key = %request.tenant_route_key))]
async fn api_key(
    State(state): State<AppState>,
    id: Option<Extension<RequestId>>,
    Json(request): Json<ApiKeyRequest>,
) -> Result<(StatusCode, Json<CreatedResponse>), WyrdErrorResponse> {
    let created = sessions(&state)?
        .exchange_api_key(
            &request.tenant_route_key,
            &SecretString::from(request.api_key),
            &SecretString::from(request.csrf_token),
            request_id(id).as_str(),
        )
        .await?;
    Ok((StatusCode::CREATED, Json(created.into())))
}

/// Body naming one session by its raw id.
#[derive(Deserialize)]
struct SessionRequest {
    /// Raw session id from the BFF's session cookie.
    session_id: String,
}

/// `sessions/read` reply: safe metadata plus the CSRF token the BFF renders.
#[derive(Serialize)]
struct ReadResponse {
    /// Route key of the session's tenant.
    tenant_key: String,
    /// Display name of the session's tenant.
    tenant_name: String,
    /// The principal the session acts as.
    principal_id: Uuid,
    /// Role names on the current access token.
    roles: Vec<String>,
    /// Permissions on the current access token.
    permissions: Vec<String>,
    /// Absolute session expiry.
    expires_at: DateTime<Utc>,
    /// The session CSRF token.
    csrf_token: String,
}

/// `POST /internal/bff/v1/sessions/read` — the safe projection of a session.
///
/// # Errors
/// Returns `401` for an unknown, revoked, expired, or no-longer-renewable
/// session and `503` when the store is unavailable.
#[tracing::instrument(level = "debug", skip_all)]
async fn read(
    State(state): State<AppState>,
    id: Option<Extension<RequestId>>,
    Json(request): Json<SessionRequest>,
) -> Result<Json<ReadResponse>, WyrdErrorResponse> {
    let view = sessions(&state)?
        .read(
            &SecretString::from(request.session_id),
            request_id(id).as_str(),
        )
        .await?;
    Ok(Json(ReadResponse {
        tenant_key: view.tenant_key.to_string(),
        tenant_name: view.tenant_name,
        principal_id: view.principal_id,
        roles: view.roles,
        permissions: view.permissions,
        expires_at: view.expires_at,
        csrf_token: view.csrf_token.expose_secret().to_owned(),
    }))
}

/// `sessions/authority` reply; used only server-side by the BFF.
#[derive(Serialize)]
struct AuthorityResponse {
    /// A current access token for the session's principal.
    access_token: String,
    /// Its expiry.
    access_expires_at: DateTime<Utc>,
}

/// `POST /internal/bff/v1/sessions/authority` — a current access token for
/// one BFF-side Wyrd API call.
///
/// # Errors
/// Returns `401` for an unknown, revoked, expired, or no-longer-renewable
/// session and `503` when the store is unavailable.
#[tracing::instrument(level = "debug", skip_all)]
async fn authority(
    State(state): State<AppState>,
    id: Option<Extension<RequestId>>,
    Json(request): Json<SessionRequest>,
) -> Result<Json<AuthorityResponse>, WyrdErrorResponse> {
    let authority = sessions(&state)?
        .authority(
            &SecretString::from(request.session_id),
            request_id(id).as_str(),
        )
        .await?;
    Ok(Json(AuthorityResponse {
        access_token: authority.access_token.expose_secret().to_owned(),
        access_expires_at: authority.access_expires_at,
    }))
}

/// `POST /internal/bff/v1/sessions/logout` — end a session; idempotent.
///
/// # Errors
/// Returns `503` when the store is unavailable.
#[tracing::instrument(level = "debug", skip_all)]
async fn logout(
    State(state): State<AppState>,
    Json(request): Json<SessionRequest>,
) -> Result<StatusCode, WyrdErrorResponse> {
    sessions(&state)?
        .logout(&SecretString::from(request.session_id))
        .await?;
    Ok(StatusCode::NO_CONTENT)
}
