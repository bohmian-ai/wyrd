//! HTTP projection of the Verification control plane.
//!
//! Five operations: evidence upload planning, direct execution, background
//! submission, execution status, and binding status. On the local storage
//! backend a sixth, the evidence body upload, is the target of each evidence
//! plan, since local storage has no presigned URL. Each handler parses its
//! path, header, and body, then delegates to [`VerificationControl`], which
//! owns authorization, audit, tenancy, and error mapping.

use axum::body::Body;
use axum::extract::rejection::PathRejection;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::{Extension, Json};
use wyrd_spec::auth::Sha256Hex;
use wyrd_spec::error::{WyrdError, WyrdProblem};
use wyrd_spec::ids::{BindingId, ExecutionId};
use wyrd_spec::verification::execution::{
    EnqueueVerificationResponse, EvidencePlanRequest, EvidencePlanResponse,
    VerificationExecutionState, VerificationRequest, VerificationResult,
};
use wyrd_spec::verification::{MAX_EXECUTE_BODY_BYTES, VerificationBindingStatus};
use wyrd_storage::BackendConfig;

use super::service::VerificationControl;
use crate::components::auth::Caller;
use crate::components::storage::routes::extract_idempotency_key;
use crate::http::error::{WyrdErrorResponse, path_rejection};
use crate::http::middleware::edge_timeout::EdgeTimer;
use crate::state::AppState;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

/// Build the Verification routes for the `/v1` group.
///
/// The local evidence upload route is mounted only on the local storage
/// backend, the one backend whose evidence plans point at it.
pub fn verification_router(state: &AppState) -> OpenApiRouter<AppState> {
    let router = OpenApiRouter::new()
        .routes(routes!(get_binding))
        .routes(routes!(evidence))
        .routes(routes!(execute))
        .routes(routes!(enqueue))
        .routes(routes!(get_execution));
    if matches!(state.storage.backend_config(), BackendConfig::Local { .. }) {
        router.routes(routes!(local_evidence))
    } else {
        router
    }
}

/// Read a bounded JSON body and decode it as `T`.
///
/// # Errors
/// Returns [`WyrdError::VerificationInputTooLarge`] past
/// [`MAX_EXECUTE_BODY_BYTES`] and [`WyrdError::VerificationInputInvalid`]
/// for a body that is not a valid `T`.
async fn decode_body<T: serde::de::DeserializeOwned>(body: Body) -> Result<T, WyrdError> {
    let body = axum::body::to_bytes(body, MAX_EXECUTE_BODY_BYTES)
        .await
        .map_err(|_| WyrdError::VerificationInputTooLarge {
            message: format!("the request body may be at most {MAX_EXECUTE_BODY_BYTES} bytes"),
            details: serde_json::json!({ "max_bytes": MAX_EXECUTE_BODY_BYTES }),
        })?;
    serde_json::from_slice(&body).map_err(|error| WyrdError::VerificationInputInvalid {
        message: format!("request body is invalid: {error}"),
        details: serde_json::json!({}),
    })
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

/// Plan direct uploads of the evidence files storage does not yet hold.
///
/// # Errors
/// Returns a stable Wyrd error when the body is malformed, a file or the
/// request exceeds a limit, the caller lacks `verifier:run`, or storage fails.
#[utoipa::path(
    post,
    path = "/verification/evidence",
    request_body = EvidencePlanRequest,
    responses(
        (status = 200, description = "Upload plans for missing files only",
         body = EvidencePlanResponse),
        (status = 400, description = "The body is malformed \
          (WYRD_VERIFICATION_400_INPUT_INVALID)", body = WyrdProblem),
        (status = 401, description = "The request carried no usable access token \
          (WYRD_AUTH_401_UNAUTHENTICATED, WYRD_AUTH_401_INVALID_TOKEN, \
          WYRD_AUTH_401_TOKEN_EXPIRED)", body = WyrdProblem),
        (status = 403, description = "The principal lacks verifier:run \
          (WYRD_PERMISSION_403_DENIED_RBAC)", body = WyrdProblem),
        (status = 413, description = "A file or the request exceeds a bound \
          (WYRD_VERIFICATION_413_INPUT_TOO_LARGE)", body = WyrdProblem),
        (status = 503, description = "Storage or Verifier execution is unavailable \
          (WYRD_STORAGE_503_BACKEND_UNAVAILABLE, WYRD_SERVER_503_SERVICE_UNAVAILABLE)",
         body = WyrdProblem)
    ),
    tag = "Verification"
)]
#[tracing::instrument(skip(state, caller, body), fields(operation = "verification.evidence"))]
async fn evidence(
    State(state): State<AppState>,
    caller: Caller,
    body: Body,
) -> Result<Json<EvidencePlanResponse>, WyrdErrorResponse> {
    let request: EvidencePlanRequest = decode_body(body).await?;
    Ok(Json(
        VerificationControl::new(&state)
            .evidence(&caller, &request)
            .await?,
    ))
}

/// Store one evidence body on the local storage backend.
///
/// The target of every local evidence plan. The body must hash to the path
/// digest; a repeated upload rewrites identical bytes.
///
/// # Errors
/// Returns a stable Wyrd error when the digest is malformed, the caller lacks
/// `verifier:run`, the body exceeds the file limit or hashes to another
/// digest, or the write fails.
#[utoipa::path(
    put,
    path = "/verification/evidence/local/{sha256}",
    params(("sha256" = String, Path, description = "Lowercase hex SHA-256 of the body, from \
        the evidence plan")),
    request_body(content = Vec<u8>, content_type = "application/octet-stream"),
    responses(
        (status = 204, description = "Stored"),
        (status = 400, description = "The digest is malformed or the body hashes to another \
          digest (WYRD_SPEC_400_VALIDATION, WYRD_STORAGE_400_SHA256_MISMATCH)", body = WyrdProblem),
        (status = 401, description = "The request carried no usable access token \
          (WYRD_AUTH_401_UNAUTHENTICATED, WYRD_AUTH_401_INVALID_TOKEN, \
          WYRD_AUTH_401_TOKEN_EXPIRED)", body = WyrdProblem),
        (status = 403, description = "The principal lacks verifier:run \
          (WYRD_PERMISSION_403_DENIED_RBAC)", body = WyrdProblem),
        (status = 413, description = "The body exceeds the file limit \
          (WYRD_VERIFICATION_413_INPUT_TOO_LARGE)", body = WyrdProblem),
        (status = 500, description = "The write failed (WYRD_STORAGE_500_BACKEND)",
         body = WyrdProblem),
        (status = 503, description = "This process hosts no Verifier execution \
          (WYRD_SERVER_503_SERVICE_UNAVAILABLE)", body = WyrdProblem)
    ),
    tag = "Verification"
)]
#[tracing::instrument(
    skip(state, caller, body),
    fields(operation = "verification.evidence.upload")
)]
async fn local_evidence(
    State(state): State<AppState>,
    caller: Caller,
    sha256: Result<Path<Sha256Hex>, PathRejection>,
    body: Body,
) -> Result<StatusCode, WyrdErrorResponse> {
    let Path(sha256) = sha256.map_err(|rejection| path_rejection(&rejection))?;
    VerificationControl::new(&state)
        .upload_local_evidence(&caller, &sha256, body)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Stage one background Verifier execution and return its ID.
///
/// # Errors
/// Returns a stable Wyrd error when the body or Idempotency-Key is malformed,
/// the caller lacks `verifier:run` on the Verifier, the inputs exceed a
/// bound, or this process hosts no Verifier queue.
#[utoipa::path(
    post,
    path = "/verification/enqueue",
    request_body = VerificationRequest,
    params(("Idempotency-Key" = Option<String>, Header, description = "Optional retry key; a \
        retry with the same key and body returns the same execution_id")),
    responses(
        (status = 202, description = "Execution staged; poll its status",
         body = EnqueueVerificationResponse),
        (status = 400, description = "The body or Idempotency-Key is invalid \
          (WYRD_VERIFICATION_400_INPUT_INVALID, WYRD_SPEC_400_VALIDATION)", body = WyrdProblem),
        (status = 401, description = "The request carried no usable access token \
          (WYRD_AUTH_401_UNAUTHENTICATED, WYRD_AUTH_401_INVALID_TOKEN, \
          WYRD_AUTH_401_TOKEN_EXPIRED)", body = WyrdProblem),
        (status = 403, description = "The principal lacks verifier:run on the Verifier \
          (WYRD_PERMISSION_403_DENIED_RBAC)", body = WyrdProblem),
        (status = 413, description = "The body or inputs exceed a bound \
          (WYRD_VERIFICATION_413_INPUT_TOO_LARGE)", body = WyrdProblem),
        (status = 503, description = "This process hosts no Verifier queue \
          (WYRD_SERVER_503_SERVICE_UNAVAILABLE)", body = WyrdProblem)
    ),
    tag = "Verification"
)]
#[tracing::instrument(
    skip(state, caller, headers, body),
    fields(operation = "verification.enqueue")
)]
async fn enqueue(
    State(state): State<AppState>,
    caller: Caller,
    headers: HeaderMap,
    body: Body,
) -> Result<(StatusCode, Json<EnqueueVerificationResponse>), WyrdErrorResponse> {
    let key = extract_idempotency_key(&headers)?;
    let request: VerificationRequest = decode_body(body).await?;
    let response = VerificationControl::new(&state).enqueue(&caller, request, key.as_ref())?;
    Ok((StatusCode::ACCEPTED, Json(response)))
}

/// Read one execution's status, dispatches, and result once visible.
///
/// # Errors
/// Returns a stable Wyrd error when the path is malformed, the caller lacks
/// `cards:read`, the execution is not visible in the caller's tenant, or a
/// read fails.
#[utoipa::path(
    get,
    path = "/verification/executions/{execution_id}",
    params(("execution_id" = ExecutionId, Path, description = "Execution ID returned by \
        POST /v1/verification/enqueue")),
    responses(
        (status = 200, description = "Execution status", body = VerificationExecutionState),
        (status = 400, description = "The execution ID is not a valid UUID \
          (WYRD_SPEC_400_VALIDATION)", body = WyrdProblem),
        (status = 401, description = "The request carried no usable access token \
          (WYRD_AUTH_401_UNAUTHENTICATED, WYRD_AUTH_401_INVALID_TOKEN, \
          WYRD_AUTH_401_TOKEN_EXPIRED)", body = WyrdProblem),
        (status = 403, description = "The principal may not read Cards \
          (WYRD_PERMISSION_403_DENIED_RBAC)", body = WyrdProblem),
        (status = 404, description = "No such execution is visible in the caller's tenant \
          yet (WYRD_VERIFICATION_404_EXECUTION_NOT_FOUND)", body = WyrdProblem),
        (status = 503, description = "The registry is unavailable, or no verifier is \
          configured for the access token (WYRD_REGISTRY_503_REGISTRY_UNAVAILABLE, \
          WYRD_AUTH_503_VERIFY_UNAVAILABLE)", body = WyrdProblem)
    ),
    tag = "Verification"
)]
#[tracing::instrument(skip(state, caller), fields(operation = "verification.execution.read"))]
async fn get_execution(
    State(state): State<AppState>,
    caller: Caller,
    execution_id: Result<Path<ExecutionId>, PathRejection>,
) -> Result<Json<VerificationExecutionState>, WyrdErrorResponse> {
    let Path(execution_id) = execution_id.map_err(|rejection| path_rejection(&rejection))?;
    Ok(Json(
        VerificationControl::new(&state)
            .get_execution(&caller, execution_id)
            .await?,
    ))
}

/// Execute one exact Verifier over supplied evidence and return its result.
///
/// The body is read up to [`MAX_EXECUTE_BODY_BYTES`]; a larger body is
/// refused before decoding. Not idempotent: a retry executes again. Body
/// collection, authentication, and resolution are bounded by the server
/// request timeout; the engine by the 60-second execution deadline. Its span
/// is named in the verification catalog, so a tenant's copy of a continued
/// trace keeps the path from the request span to the result unbroken.
///
/// # Errors
/// Returns a stable Wyrd error when the body is too large or malformed, the
/// caller lacks `verifier:run` on the Verifier, the target is unknown, the
/// Verifier judges a scheduled window, or the deadline elapses.
#[utoipa::path(
    post,
    path = "/verification/execute",
    request_body = VerificationRequest,
    responses(
        (status = 200, description = "The result; a failed verdict is a success",
         body = VerificationResult),
        (status = 400, description = "The body or input is malformed \
          (WYRD_VERIFICATION_400_INPUT_INVALID)", body = WyrdProblem),
        (status = 401, description = "The request carried no usable access token \
          (WYRD_AUTH_401_UNAUTHENTICATED, WYRD_AUTH_401_INVALID_TOKEN, \
          WYRD_AUTH_401_TOKEN_EXPIRED)", body = WyrdProblem),
        (status = 403, description = "The principal lacks verifier:run on the Verifier \
          (WYRD_PERMISSION_403_DENIED_RBAC)", body = WyrdProblem),
        (status = 404, description = "No active Verifier and subject with these UIDs in the \
          caller's tenant (WYRD_VERIFICATION_404_TARGET_NOT_FOUND)", body = WyrdProblem),
        (status = 413, description = "The body or input exceeds a bound \
          (WYRD_VERIFICATION_413_INPUT_TOO_LARGE)", body = WyrdProblem),
        (status = 422, description = "The Verifier judges a scheduled window and runs only on \
          its binding schedule (WYRD_VERIFICATION_422_INPUT_UNSUPPORTED)", body = WyrdProblem),
        (status = 503, description = "The registry is unavailable, no verifier is configured \
          for the access token, or this process hosts no Verifier execution \
          (WYRD_REGISTRY_503_REGISTRY_UNAVAILABLE, WYRD_AUTH_503_VERIFY_UNAVAILABLE, \
          WYRD_SERVER_503_SERVICE_UNAVAILABLE)", body = WyrdProblem),
        (status = 504, description = "The execution exceeded its 60-second deadline \
          (WYRD_VERIFICATION_504_EXECUTION_TIMED_OUT)", body = WyrdProblem)
    ),
    tag = "Verification"
)]
#[tracing::instrument(
    name = "verification.handle",
    skip(state, caller, body),
    fields(operation = "verification.execute")
)]
async fn execute(
    State(state): State<AppState>,
    edge_timer: Option<Extension<EdgeTimer>>,
    caller: Caller,
    body: Body,
) -> Result<Json<VerificationResult>, WyrdErrorResponse> {
    let request: VerificationRequest = decode_body(body).await?;
    Ok(Json(
        VerificationControl::new(&state)
            .execute(
                &caller,
                &request,
                edge_timer.map(|Extension(timer)| timer).as_ref(),
            )
            .await?,
    ))
}
