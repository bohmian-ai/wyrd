//! HTTP projection of the Verification control plane.
//!
//! Exactly three operations: binding status, manual run request, and run
//! status. Each handler parses its path, header, and body, then delegates to
//! [`VerificationControl`], which owns authorization, audit, tenancy, and
//! error mapping. Verdicts and Drift/Eval details are read from Bifrost by
//! `result_id`, so there is no result endpoint here.

use axum::Json;
use axum::body::Bytes;
use axum::extract::rejection::PathRejection;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use wyrd_spec::error::{WyrdError, WyrdProblem};
use wyrd_spec::ids::{BindingId, VerificationRunId};
use wyrd_spec::verification::{
    StartVerificationRunRequest, StartVerificationRunResponse, VerificationBindingStatus,
    VerificationRunStatus,
};

use super::service::{VerificationControl, decode_start_request};
use crate::components::auth::Caller;
use crate::components::storage::routes::extract_idempotency_key;
use crate::http::error::{WyrdErrorResponse, path_rejection};
use crate::state::AppState;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

/// Build the Verification routes for the `/v1` group.
pub fn verification_router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(get_binding))
        .routes(routes!(start_run))
        .routes(routes!(get_run))
}

/// Read one verification binding's identities, activity, readiness, and cursor.
///
/// # Errors
/// Returns a stable Wyrd error when the path is malformed, the caller lacks
/// `cards:read`, the binding is not in the caller's tenant, or a read fails.
#[utoipa::path(
    get,
    path = "/verification/bindings/{binding_id}",
    params(("binding_id" = BindingId, Path, description = "Binding ID from the owner Card's \
        card.status.verification.binding_ids")),
    responses(
        (status = 200, description = "Binding status", body = VerificationBindingStatus),
        (status = 400, description = "The binding ID is not a valid UUID \
          (WYRD_SPEC_400_VALIDATION)", body = WyrdProblem),
        (status = 401, description = "The request carried no usable access token \
          (WYRD_AUTH_401_UNAUTHENTICATED, WYRD_AUTH_401_INVALID_TOKEN, \
          WYRD_AUTH_401_TOKEN_EXPIRED)", body = WyrdProblem),
        (status = 403, description = "The principal may not read Cards \
          (WYRD_PERMISSION_403_DENIED_RBAC)", body = WyrdProblem),
        (status = 404, description = "No such binding in the caller's tenant \
          (WYRD_VERIFICATION_404_BINDING_NOT_FOUND)", body = WyrdProblem),
        (status = 500, description = "The authorization decision could not be audited \
          (WYRD_VALA_500_AUDIT_UNAVAILABLE)", body = WyrdProblem),
        (status = 503, description = "The registry is unavailable, or no verifier is \
          configured for the access token (WYRD_REGISTRY_503_REGISTRY_UNAVAILABLE, \
          WYRD_AUTH_503_VERIFY_UNAVAILABLE)", body = WyrdProblem)
    ),
    tag = "Verification"
)]
#[tracing::instrument(skip(state, caller), fields(operation = "verification.binding.read"))]
async fn get_binding(
    State(state): State<AppState>,
    caller: Caller,
    binding_id: Result<Path<BindingId>, PathRejection>,
) -> Result<Json<VerificationBindingStatus>, WyrdErrorResponse> {
    let Path(binding_id) = binding_id.map_err(|rejection| path_rejection(&rejection))?;
    Ok(Json(
        VerificationControl::new(&state)
            .get_binding(&caller, binding_id)
            .await?,
    ))
}

/// Durably enqueue one manual Drift run and return its ID.
///
/// # Errors
/// Returns a stable Wyrd error when the body, window, or Idempotency-Key is
/// malformed, the caller lacks `evals:run` or subject scope, the target is
/// unknown, unrunnable, or not ready, the key was used for a different
/// request, or the enqueue fails.
#[utoipa::path(
    post,
    path = "/verification/runs",
    request_body = StartVerificationRunRequest,
    params(("Idempotency-Key" = Option<String>, Header, description = "Optional retry key; a \
        retry with the same key and body returns the same run_id")),
    responses(
        (status = 202, description = "Run durably enqueued", body = StartVerificationRunResponse),
        (status = 400, description = "The body, target, window, or Idempotency-Key is invalid \
          (WYRD_SPEC_400_VALIDATION, WYRD_VERIFICATION_400_INVALID_TARGET, \
          WYRD_VERIFICATION_400_INVALID_WINDOW)", body = WyrdProblem),
        (status = 401, description = "The request carried no usable access token \
          (WYRD_AUTH_401_UNAUTHENTICATED, WYRD_AUTH_401_INVALID_TOKEN, \
          WYRD_AUTH_401_TOKEN_EXPIRED)", body = WyrdProblem),
        (status = 403, description = "The principal lacks evals:run or Card scope over the \
          subject (WYRD_PERMISSION_403_DENIED_RBAC)", body = WyrdProblem),
        (status = 404, description = "No such binding in the caller's tenant \
          (WYRD_VERIFICATION_404_BINDING_NOT_FOUND)", body = WyrdProblem),
        (status = 409, description = "The Verifier's baseline is not ready, or the \
          Idempotency-Key was used for a different request \
          (WYRD_VERIFICATION_409_VERIFIER_NOT_READY, WYRD_REGISTRY_409_IDEMPOTENCY_CONFLICT)",
         body = WyrdProblem),
        (status = 500, description = "The authorization decision could not be audited \
          (WYRD_VALA_500_AUDIT_UNAVAILABLE)", body = WyrdProblem),
        (status = 503, description = "The registry is unavailable, or no verifier is \
          configured for the access token (WYRD_REGISTRY_503_REGISTRY_UNAVAILABLE, \
          WYRD_AUTH_503_VERIFY_UNAVAILABLE)", body = WyrdProblem)
    ),
    tag = "Verification"
)]
#[tracing::instrument(
    skip(state, caller, headers, body),
    fields(operation = "verification.run.start")
)]
async fn start_run(
    State(state): State<AppState>,
    caller: Caller,
    headers: HeaderMap,
    body: Bytes,
) -> Result<(StatusCode, Json<StartVerificationRunResponse>), WyrdErrorResponse> {
    let key = extract_idempotency_key(&headers)?;
    let body = serde_json::from_slice(&body).map_err(|error| WyrdError::Validation {
        message: format!("request body is not JSON: {error}"),
        details: serde_json::json!({}),
    })?;
    let request = decode_start_request(body)?;
    let run_id = VerificationControl::new(&state)
        .start_run(&caller, &request, key.as_ref())
        .await?;
    Ok((
        StatusCode::ACCEPTED,
        Json(StartVerificationRunResponse { run_id }),
    ))
}

/// Read one run's execution status, requester, result pointer, and dispatches.
///
/// # Errors
/// Returns a stable Wyrd error when the path is malformed, the caller lacks
/// `cards:read`, the run is not in the caller's tenant, or a read fails.
#[utoipa::path(
    get,
    path = "/verification/runs/{run_id}",
    params(("run_id" = VerificationRunId, Path, description = "Run ID returned by \
        POST /v1/verification/runs")),
    responses(
        (status = 200, description = "Run status", body = VerificationRunStatus),
        (status = 400, description = "The run ID is not a valid UUID \
          (WYRD_SPEC_400_VALIDATION)", body = WyrdProblem),
        (status = 401, description = "The request carried no usable access token \
          (WYRD_AUTH_401_UNAUTHENTICATED, WYRD_AUTH_401_INVALID_TOKEN, \
          WYRD_AUTH_401_TOKEN_EXPIRED)", body = WyrdProblem),
        (status = 403, description = "The principal may not read Cards \
          (WYRD_PERMISSION_403_DENIED_RBAC)", body = WyrdProblem),
        (status = 404, description = "No such run in the caller's tenant \
          (WYRD_VERIFICATION_404_RUN_NOT_FOUND)", body = WyrdProblem),
        (status = 500, description = "The authorization decision could not be audited \
          (WYRD_VALA_500_AUDIT_UNAVAILABLE)", body = WyrdProblem),
        (status = 503, description = "The registry is unavailable, or no verifier is \
          configured for the access token (WYRD_REGISTRY_503_REGISTRY_UNAVAILABLE, \
          WYRD_AUTH_503_VERIFY_UNAVAILABLE)", body = WyrdProblem)
    ),
    tag = "Verification"
)]
#[tracing::instrument(skip(state, caller), fields(operation = "verification.run.read"))]
async fn get_run(
    State(state): State<AppState>,
    caller: Caller,
    run_id: Result<Path<VerificationRunId>, PathRejection>,
) -> Result<Json<VerificationRunStatus>, WyrdErrorResponse> {
    let Path(run_id) = run_id.map_err(|rejection| path_rejection(&rejection))?;
    Ok(Json(
        VerificationControl::new(&state)
            .get_run(&caller, run_id)
            .await?,
    ))
}
