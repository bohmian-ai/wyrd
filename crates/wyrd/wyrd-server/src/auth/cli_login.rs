//! CLI human login HTTP adapters: handoff begin, claim, and cancel, and
//! refresh-chain revocation at logout.
//!
//! Every route is anonymous: the CLI holds no Wyrd session yet, or is ending
//! one. Each handler composes [`CliLogins`] from the server's auth
//! configuration and maps its refusals to problem JSON.

use axum::Json;
use axum::extract::{Extension, Path, State};
use axum::http::StatusCode;
use uuid::Uuid;
use wyrd_auth::cli_logins::CliLogins;
use wyrd_spec::auth::{
    CliHandoff, CliHandoffClaim, CliHandoffProof, CreateCliHandoff, RevokeRefreshToken,
};
use wyrd_spec::error::WyrdProblem;
use wyrd_spec::request_id::RequestId;

use crate::auth::auth_not_configured;
use crate::http::error::WyrdErrorResponse;
use crate::state::AppState;

/// Compose the CLI login owner from the server's human-connection owner and
/// token verifier.
///
/// # Errors
/// Returns a `500` when either is not configured.
fn cli_logins(state: &AppState) -> Result<CliLogins, WyrdErrorResponse> {
    Ok(CliLogins::new(
        state
            .auth
            .human_connections
            .clone()
            .ok_or_else(auth_not_configured)?,
        state
            .auth
            .token_verifier
            .clone()
            .ok_or_else(auth_not_configured)?,
    ))
}

/// `POST /auth/cli-handoffs` — begin a CLI login.
///
/// Returns the handoff id, the provider URL to open in the system browser,
/// the one-time poll verifier, and the handoff expiry. Like `POST /auth/login`
/// it appends no audit event: it evaluates no principal permission.
///
/// # Errors
/// Returns the refusals of [`CliLogins::begin`] and a `500` when auth is not
/// configured.
#[utoipa::path(
    post,
    path = "/auth/cli-handoffs",
    request_body = CreateCliHandoff,
    responses(
        (status = 200, description = "Handoff begun; open `login_url` in the system browser and \
          poll the claim route with `poll_verifier`", body = CliHandoff),
        (status = 400, description = "The deployment has no public origin or sealing key \
          (WYRD_SPEC_400_VALIDATION)", body = WyrdProblem),
        (status = 401, description = "SSO login is not available for this tenant route key \
          (WYRD_AUTH_401_INVALID_TOKEN)", body = WyrdProblem),
        (status = 503, description = "The identity provider or auth backend is unavailable \
          (WYRD_AUTH_503_DISCOVERY_UNAVAILABLE, WYRD_AUTH_503_VERIFY_UNAVAILABLE)",
          body = WyrdProblem)
    ),
    // No session exists yet at this operation, so it clears the document-wide
    // requirement instead of inheriting it.
    security(()),
    tag = "Auth"
)]
#[tracing::instrument(
    level = "debug",
    skip(state, request),
    fields(tenant_route_key = %request.tenant_route_key)
)]
pub async fn begin_cli_handoff(
    State(state): State<AppState>,
    Json(request): Json<CreateCliHandoff>,
) -> Result<Json<CliHandoff>, WyrdErrorResponse> {
    cli_logins(&state)?
        .begin(&request.tenant_route_key)
        .await
        .map(Json)
        .map_err(WyrdErrorResponse::from)
}

/// `POST /auth/cli-handoffs/{handoff_id}/claim` — poll a CLI login.
///
/// Answers `pending` with a retry interval until the browser sign-in
/// completes, then hands the Wyrd user credential to the verifier holder
/// exactly once.
///
/// # Errors
/// Returns the refusals of [`CliLogins::claim`] and a `500` when auth is not
/// configured.
#[utoipa::path(
    post,
    path = "/auth/cli-handoffs/{handoff_id}/claim",
    params(("handoff_id" = Uuid, Path, description = "Handoff id from the begin response")),
    request_body = CliHandoffProof,
    responses(
        (status = 200, description = "`pending` with a retry interval, or `complete` with the \
          Wyrd user credential, returned once", body = CliHandoffClaim),
        (status = 400, description = "The handoff is unknown, expired, cancelled, or already \
          claimed, the verifier is wrong, or the route key names another tenant \
          (WYRD_AUTH_400_INVALID_STATE); or the deployment has no sealing key \
          (WYRD_SPEC_400_VALIDATION)", body = WyrdProblem),
        (status = 401, description = "The login went through a connection that is no longer the \
          handoff's (WYRD_AUTH_401_INVALID_TOKEN)", body = WyrdProblem),
        (status = 500, description = "The stored completion could not be opened \
          (WYRD_SPEC_500_INTERNAL)", body = WyrdProblem),
        (status = 503, description = "The auth backend or audit path is unavailable \
          (WYRD_AUTH_503_VERIFY_UNAVAILABLE, WYRD_AUDIT_503_UNAVAILABLE)", body = WyrdProblem)
    ),
    security(()),
    tag = "Auth"
)]
#[tracing::instrument(level = "debug", skip(state, request_id, proof))]
pub async fn claim_cli_handoff(
    State(state): State<AppState>,
    request_id: Option<Extension<RequestId>>,
    Path(handoff_id): Path<Uuid>,
    Json(proof): Json<CliHandoffProof>,
) -> Result<Json<CliHandoffClaim>, WyrdErrorResponse> {
    let request_id = request_id.map_or_else(RequestId::now_v7, |Extension(id)| id);
    cli_logins(&state)?
        .claim(
            &proof.tenant_route_key,
            handoff_id,
            &proof.poll_verifier,
            request_id.as_str(),
        )
        .await
        .map(Json)
        .map_err(WyrdErrorResponse::from)
}

/// `POST /auth/cli-handoffs/{handoff_id}/cancel` — abandon a CLI login.
///
/// Idempotent: an unknown handoff or wrong verifier also answers `204` and
/// changes nothing.
///
/// # Errors
/// Returns a `503` when the store fails and a `500` when auth is not
/// configured.
#[utoipa::path(
    post,
    path = "/auth/cli-handoffs/{handoff_id}/cancel",
    params(("handoff_id" = Uuid, Path, description = "Handoff id from the begin response")),
    request_body = CliHandoffProof,
    responses(
        (status = 204, description = "The handoff, if the verifier named one, no longer exists"),
        (status = 503, description = "The auth backend is unavailable \
          (WYRD_AUTH_503_VERIFY_UNAVAILABLE)", body = WyrdProblem)
    ),
    security(()),
    tag = "Auth"
)]
#[tracing::instrument(level = "debug", skip(state, proof))]
pub async fn cancel_cli_handoff(
    State(state): State<AppState>,
    Path(handoff_id): Path<Uuid>,
    Json(proof): Json<CliHandoffProof>,
) -> Result<StatusCode, WyrdErrorResponse> {
    cli_logins(&state)?
        .cancel(&proof.tenant_route_key, handoff_id, &proof.poll_verifier)
        .await
        .map(|()| StatusCode::NO_CONTENT)
        .map_err(WyrdErrorResponse::from)
}

/// `POST /auth/revoke` — end the login a refresh token belongs to.
///
/// RFC 7009 semantics: possession of the refresh token is the authority, and
/// an unknown, malformed, or already revoked token also answers `204`. Only
/// that login's refresh chain is revoked; the User's other logins continue.
/// The revocation commits with its audit event under the request id.
///
/// # Errors
/// Returns a `503` when the store or the audit path fails and a `500` when
/// auth is not configured.
#[utoipa::path(
    post,
    path = "/auth/revoke",
    request_body = RevokeRefreshToken,
    responses(
        (status = 204, description = "The token's login, if it named one, no longer renews"),
        (status = 503, description = "The auth backend or audit path is unavailable \
          (WYRD_AUTH_503_VERIFY_UNAVAILABLE, WYRD_AUDIT_503_UNAVAILABLE)", body = WyrdProblem)
    ),
    security(()),
    tag = "Auth"
)]
#[tracing::instrument(level = "debug", skip_all)]
pub async fn revoke_refresh_token(
    State(state): State<AppState>,
    request_id: Option<Extension<RequestId>>,
    Json(request): Json<RevokeRefreshToken>,
) -> Result<StatusCode, WyrdErrorResponse> {
    let request_id = request_id.map_or_else(RequestId::now_v7, |Extension(id)| id);
    cli_logins(&state)?
        .end(&request.refresh_token, request_id.as_str())
        .await
        .map(|()| StatusCode::NO_CONTENT)
        .map_err(WyrdErrorResponse::from)
}
