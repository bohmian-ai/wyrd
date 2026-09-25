//! Tenant human OIDC connection administration: `/v1/identity/oidc/*`.
//!
//! Thin HTTP adapters over [`HumanConnections`], the one owner of a tenant's
//! human login trust. The tenant is always the bearer's tenant; no route takes
//! a tenant identifier. Every route requires `identity_connections:write`, and
//! its verdict is audited before any provider IO or store read: a denial is
//! recorded standalone and refused, and the allowed decision is appended in the
//! same tenant transaction as the state change it authorizes. A refused caller
//! therefore sees neither connection metadata nor secrets, and no secret ever
//! appears in a response, error, log, or audit row.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use serde_json::Value;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;
use wyrd_auth::connections::HumanConnections;
use wyrd_runtime::Permission;
use wyrd_spec::auth::{
    ConnectionActivate, ConnectionInput, ConnectionTestRequest, ConnectionTestResponse,
    HumanConnectionView, HumanConnectionsResponse,
};
use wyrd_spec::error::{WyrdError, WyrdProblem};
use wyrd_spec::vala::api::AuditEvent;

use crate::audit;
use crate::auth::auth_not_configured;
use crate::components::auth::Caller;
use crate::http::error::WyrdErrorResponse;
use crate::state::AppState;

/// Audit resource naming the caller tenant's connection slot.
const RESOURCE: &str = "identity:oidc_connection";

/// Build the tenant connection administration routes for the `/v1` group.
pub fn identity_router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_connections))
        .routes(routes!(put_candidate))
        .routes(routes!(test_candidate))
        .routes(routes!(activate_candidate))
        .routes(routes!(deactivate_active))
        .routes(routes!(remove_connection))
}

/// The deployment's connection owner, built once at boot.
///
/// # Errors
/// Returns `AUTH_NOT_CONFIGURED` when the server was assembled without auth.
fn connections(state: &AppState) -> Result<&HumanConnections, WyrdErrorResponse> {
    state
        .auth
        .human_connections
        .as_ref()
        .ok_or_else(auth_not_configured)
}

/// Evaluate `identity_connections:write` for `operation`, recording a denial.
///
/// # Errors
/// Returns the RBAC denial (already audited) and `AuditUnavailable` when the
/// denial cannot be recorded.
async fn decide(
    state: &AppState,
    caller: &Caller,
    operation: &str,
) -> Result<AuditEvent, WyrdErrorResponse> {
    audit::authorize_recording_denial(
        state,
        caller,
        &Permission::identity_connections_write(),
        operation,
        RESOURCE,
    )
    .await
    .map_err(WyrdErrorResponse::from)
}

/// Commit an allowed decision standalone and return `refusal`.
///
/// Used when a request is refused after authorization but before any tenant
/// transaction exists (an invalid body), so the allowance is still durable.
///
/// # Errors
/// Always returns an error: `refusal`, or `AuditUnavailable` when the decision
/// cannot be recorded.
async fn refuse_after_decision<T>(
    state: &AppState,
    caller: &Caller,
    decision: &AuditEvent,
    refusal: WyrdError,
) -> Result<T, WyrdErrorResponse> {
    audit::record_audit(state.postgres.vala_pool(), caller.data_tenant_id, decision)
        .await
        .map_err(WyrdErrorResponse::from)?;
    Err(WyrdErrorResponse::from(refusal))
}

/// `GET /v1/identity/oidc/connections` — the caller tenant's Active and
/// Candidate connections, redacted, plus the callback URL to register.
///
/// # Errors
/// Returns `403` without `identity_connections:write`, and `503` when the store is
/// unavailable or the decision cannot be audited.
#[utoipa::path(
    get,
    path = "/identity/oidc/connections",
    responses(
        (status = 200, description = "Redacted Active and Candidate connections; secrets are \
          never returned", body = HumanConnectionsResponse),
        (status = 401, description = "The request carried no usable access token \
          (WYRD_AUTH_401_UNAUTHENTICATED, WYRD_AUTH_401_INVALID_TOKEN, \
          WYRD_AUTH_401_TOKEN_EXPIRED)", body = WyrdProblem),
        (status = 403, description = "Caller lacks identity_connections:write \
          (WYRD_PERMISSION_403_DENIED_RBAC)", body = WyrdProblem),
        (status = 500, description = "A stored row did not decode (WYRD_SPEC_500_INTERNAL)",
         body = WyrdProblem),
        (status = 503, description = "The store is unavailable or the decision could not be \
          audited (WYRD_AUTH_503_VERIFY_UNAVAILABLE, WYRD_AUDIT_503_UNAVAILABLE)",
         body = WyrdProblem)
    ),
    tag = "Identity"
)]
#[tracing::instrument(skip_all, fields(operation = "identity.oidc.connections.list"))]
async fn list_connections(
    State(state): State<AppState>,
    caller: Caller,
) -> Result<Json<HumanConnectionsResponse>, WyrdErrorResponse> {
    let decision = decide(&state, &caller, "identity.oidc.connections.list").await?;
    connections(&state)?
        .list(caller.data_tenant_id, &decision)
        .await
        .map(Json)
        .map_err(WyrdErrorResponse::from)
}

/// `PUT /v1/identity/oidc/candidate` — create the tenant's candidate, or
/// replace it at the next revision.
///
/// This is also the rotation path for a secret or group-role map, including
/// when the issuer is unchanged. Replacing a candidate clears its test stamp.
/// The body is authorized before it is interpreted.
///
/// # Errors
/// Returns `400` for invalid input, a missing public origin, or an unsupported
/// client authentication method, `403` without `identity_connections:write`,
/// `409` for a stale `expected_revision`, and `503` when the store is
/// unavailable or the decision cannot be audited.
#[utoipa::path(
    put,
    path = "/identity/oidc/candidate",
    request_body = ConnectionInput,
    responses(
        (status = 200, description = "Candidate staged; the secret is never returned",
         body = HumanConnectionView),
        (status = 400, description = "Invalid input or no public origin configured \
          (WYRD_SPEC_400_VALIDATION), or an unsupported client authentication method \
          (WYRD_AUTH_400_UNSUPPORTED_CLIENT_AUTH)", body = WyrdProblem),
        (status = 401, description = "The request carried no usable access token \
          (WYRD_AUTH_401_UNAUTHENTICATED, WYRD_AUTH_401_INVALID_TOKEN, \
          WYRD_AUTH_401_TOKEN_EXPIRED)", body = WyrdProblem),
        (status = 403, description = "Caller lacks identity_connections:write \
          (WYRD_PERMISSION_403_DENIED_RBAC)", body = WyrdProblem),
        (status = 409, description = "expected_revision does not match the current candidate \
          (WYRD_AUTH_409_CONNECTION_CONFLICT)", body = WyrdProblem),
        (status = 500, description = "An unexpected server failure (WYRD_SPEC_500_INTERNAL)",
         body = WyrdProblem),
        (status = 503, description = "The store is unavailable or the decision could not be \
          audited (WYRD_AUTH_503_VERIFY_UNAVAILABLE, WYRD_AUDIT_503_UNAVAILABLE)",
         body = WyrdProblem)
    ),
    tag = "Identity"
)]
#[tracing::instrument(skip_all, fields(operation = "identity.oidc.candidate.put"))]
async fn put_candidate(
    State(state): State<AppState>,
    caller: Caller,
    Json(body): Json<Value>,
) -> Result<Json<HumanConnectionView>, WyrdErrorResponse> {
    let decision = decide(&state, &caller, "identity.oidc.candidate.put").await?;
    let owner = connections(&state)?;
    let staged = match ConnectionInput::from_json(body).and_then(|input| owner.stage(input)) {
        Ok(staged) => staged,
        Err(refusal) => return refuse_after_decision(&state, &caller, &decision, refusal).await,
    };
    owner
        .put_candidate(caller.data_tenant_id, staged, &decision)
        .await
        .map(Json)
        .map_err(WyrdErrorResponse::from)
}

/// `POST /v1/identity/oidc/candidate/test` — prove the candidate at
/// `expected_revision` against its provider and stamp it activatable for
/// fifteen minutes.
///
/// The allowed decision is committed before the screened provider IO starts.
/// After every check passes, `identity_connections:write` is evaluated again
/// as `identity.oidc.candidate.tested`; that decision and the stamp commit
/// together, and a denial there stamps nothing.
///
/// # Errors
/// Returns `400` when no public origin is configured, `403` without
/// `identity_connections:write` at either evaluation, `409` for a stale revision
/// (`CONNECTION_CONFLICT`) or a failed check (`CONNECTION_NOT_TESTED`), and
/// `503` when the provider is refused by address screening or unavailable, the
/// store is unavailable, or a decision cannot be audited.
#[utoipa::path(
    post,
    path = "/identity/oidc/candidate/test",
    request_body = ConnectionTestRequest,
    responses(
        (status = 200, description = "The candidate passed every check and is activatable \
          until tested_until", body = ConnectionTestResponse),
        (status = 400, description = "No public origin is configured \
          (WYRD_SPEC_400_VALIDATION)", body = WyrdProblem),
        (status = 401, description = "The request carried no usable access token \
          (WYRD_AUTH_401_UNAUTHENTICATED, WYRD_AUTH_401_INVALID_TOKEN, \
          WYRD_AUTH_401_TOKEN_EXPIRED)", body = WyrdProblem),
        (status = 403, description = "Caller lacks identity_connections:write \
          (WYRD_PERMISSION_403_DENIED_RBAC)", body = WyrdProblem),
        (status = 409, description = "No candidate at expected_revision \
          (WYRD_AUTH_409_CONNECTION_CONFLICT), or a check failed; details.reason names it \
          (WYRD_AUTH_409_CONNECTION_NOT_TESTED)", body = WyrdProblem),
        (status = 500, description = "An unexpected server failure (WYRD_SPEC_500_INTERNAL)",
         body = WyrdProblem),
        (status = 503, description = "The provider is refused by address screening or \
          unavailable, the store is unavailable, or a decision could not be audited \
          (WYRD_AUTH_503_DISCOVERY_UNAVAILABLE, \
          WYRD_AUTH_503_VERIFY_UNAVAILABLE, WYRD_AUDIT_503_UNAVAILABLE)",
         body = WyrdProblem)
    ),
    tag = "Identity"
)]
#[tracing::instrument(skip_all, fields(operation = "identity.oidc.candidate.test"))]
async fn test_candidate(
    State(state): State<AppState>,
    caller: Caller,
    Json(request): Json<ConnectionTestRequest>,
) -> Result<Json<ConnectionTestResponse>, WyrdErrorResponse> {
    let decision = decide(&state, &caller, "identity.oidc.candidate.test").await?;
    audit::record_audit(state.postgres.vala_pool(), caller.data_tenant_id, &decision)
        .await
        .map_err(WyrdErrorResponse::from)?;
    let owner = connections(&state)?;
    let tested = owner
        .probe_candidate(caller.data_tenant_id, request.expected_revision)
        .await
        .map_err(WyrdErrorResponse::from)?;
    // The probe ran for seconds without a lock; the caller's authority is
    // evaluated again at the stamp so a grant withdrawn meanwhile stamps
    // nothing.
    let stamp = decide(&state, &caller, "identity.oidc.candidate.tested").await?;
    owner
        .stamp_candidate(caller.data_tenant_id, tested, &stamp)
        .await
        .map(|candidate| Json(ConnectionTestResponse { candidate }))
        .map_err(WyrdErrorResponse::from)
}

/// `POST /v1/identity/oidc/candidate/activate` — make the freshly tested
/// candidate the tenant's Active connection, retiring the previous one in the
/// same transaction.
///
/// Requires the candidate's current test stamp and a recovery API key of a
/// headless principal of this tenant holding `identity_connections:write`.
/// The key is verified and discarded; it is never stored, logged, or audited.
///
/// # Errors
/// Returns `403` without `identity_connections:write`, `409` for a stale
/// revision or invalid recovery key (`CONNECTION_CONFLICT`) or a missing or
/// expired test (`CONNECTION_NOT_TESTED`), and `503` when the store is
/// unavailable or the decision cannot be audited.
#[utoipa::path(
    post,
    path = "/identity/oidc/candidate/activate",
    request_body = ConnectionActivate,
    responses(
        (status = 200, description = "The candidate is now the Active connection",
         body = HumanConnectionView),
        (status = 401, description = "The request carried no usable access token \
          (WYRD_AUTH_401_UNAUTHENTICATED, WYRD_AUTH_401_INVALID_TOKEN, \
          WYRD_AUTH_401_TOKEN_EXPIRED)", body = WyrdProblem),
        (status = 403, description = "Caller lacks identity_connections:write \
          (WYRD_PERMISSION_403_DENIED_RBAC)", body = WyrdProblem),
        (status = 409, description = "Stale revision or invalid recovery key \
          (WYRD_AUTH_409_CONNECTION_CONFLICT), or no unexpired test for this revision \
          (WYRD_AUTH_409_CONNECTION_NOT_TESTED)", body = WyrdProblem),
        (status = 500, description = "An unexpected server failure (WYRD_SPEC_500_INTERNAL)",
         body = WyrdProblem),
        (status = 503, description = "The store is unavailable or the decision could not be \
          audited (WYRD_AUTH_503_VERIFY_UNAVAILABLE, WYRD_AUDIT_503_UNAVAILABLE)",
         body = WyrdProblem)
    ),
    tag = "Identity"
)]
#[tracing::instrument(skip_all, fields(operation = "identity.oidc.candidate.activate"))]
async fn activate_candidate(
    State(state): State<AppState>,
    caller: Caller,
    Json(request): Json<ConnectionActivate>,
) -> Result<Json<HumanConnectionView>, WyrdErrorResponse> {
    let decision = decide(&state, &caller, "identity.oidc.candidate.activate").await?;
    connections(&state)?
        .activate(caller.data_tenant_id, request, &decision)
        .await
        .map(Json)
        .map_err(WyrdErrorResponse::from)
}

/// `POST /v1/identity/oidc/active/deactivate` — retire the Active connection;
/// human login through it stops immediately on every replica.
///
/// # Errors
/// Returns `403` without `identity_connections:write`, `404` when no Active
/// connection exists, and `503` when the store is unavailable
/// or the decision cannot be audited.
#[utoipa::path(
    post,
    path = "/identity/oidc/active/deactivate",
    responses(
        (status = 204, description = "The Active connection was retired"),
        (status = 401, description = "The request carried no usable access token \
          (WYRD_AUTH_401_UNAUTHENTICATED, WYRD_AUTH_401_INVALID_TOKEN, \
          WYRD_AUTH_401_TOKEN_EXPIRED)", body = WyrdProblem),
        (status = 403, description = "Caller lacks identity_connections:write \
          (WYRD_PERMISSION_403_DENIED_RBAC)", body = WyrdProblem),
        (status = 404, description = "No Active connection exists \
          (WYRD_SPEC_404_NOT_FOUND)", body = WyrdProblem),
        (status = 500, description = "An unexpected server failure (WYRD_SPEC_500_INTERNAL)",
         body = WyrdProblem),
        (status = 503, description = "The store is unavailable or the decision could not be \
          audited (WYRD_AUTH_503_VERIFY_UNAVAILABLE, WYRD_AUDIT_503_UNAVAILABLE)",
         body = WyrdProblem)
    ),
    tag = "Identity"
)]
#[tracing::instrument(skip_all, fields(operation = "identity.oidc.active.deactivate"))]
async fn deactivate_active(
    State(state): State<AppState>,
    caller: Caller,
) -> Result<StatusCode, WyrdErrorResponse> {
    let decision = decide(&state, &caller, "identity.oidc.active.deactivate").await?;
    connections(&state)?
        .deactivate(caller.data_tenant_id, &decision)
        .await
        .map(|_| StatusCode::NO_CONTENT)
        .map_err(WyrdErrorResponse::from)
}

/// `DELETE /v1/identity/oidc/connections/{id}` — tombstone one connection: it
/// stops trusting logins, its secret is wiped, and its id is kept for history.
///
/// # Errors
/// Returns `403` without `identity_connections:write`, `404` when this tenant
/// has no live connection with `id`, and `503` when the store is
/// unavailable or the decision cannot be audited.
#[utoipa::path(
    delete,
    path = "/identity/oidc/connections/{id}",
    params(("id" = Uuid, Path, description = "Connection id")),
    responses(
        (status = 204, description = "The connection was tombstoned"),
        (status = 401, description = "The request carried no usable access token \
          (WYRD_AUTH_401_UNAUTHENTICATED, WYRD_AUTH_401_INVALID_TOKEN, \
          WYRD_AUTH_401_TOKEN_EXPIRED)", body = WyrdProblem),
        (status = 403, description = "Caller lacks identity_connections:write \
          (WYRD_PERMISSION_403_DENIED_RBAC)", body = WyrdProblem),
        (status = 404, description = "No live connection with this id in the caller tenant \
          (WYRD_SPEC_404_NOT_FOUND)", body = WyrdProblem),
        (status = 500, description = "An unexpected server failure (WYRD_SPEC_500_INTERNAL)",
         body = WyrdProblem),
        (status = 503, description = "The store is unavailable or the decision could not be \
          audited (WYRD_AUTH_503_VERIFY_UNAVAILABLE, WYRD_AUDIT_503_UNAVAILABLE)",
         body = WyrdProblem)
    ),
    tag = "Identity"
)]
#[tracing::instrument(skip_all, fields(operation = "identity.oidc.connection.remove"))]
async fn remove_connection(
    State(state): State<AppState>,
    caller: Caller,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, WyrdErrorResponse> {
    let decision = decide(&state, &caller, "identity.oidc.connection.remove").await?;
    connections(&state)?
        .remove(caller.data_tenant_id, id, &decision)
        .await
        .map(|()| StatusCode::NO_CONTENT)
        .map_err(WyrdErrorResponse::from)
}
