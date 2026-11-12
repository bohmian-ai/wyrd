//! HTTP projection of the Verification control plane.
//!
//! Evidence upload planning, direct execution, background submission,
//! execution status and details, invocation and publication inspection,
//! publication download planning, cancellation, and binding status. Evidence bytes travel
//! through the shared storage upload lifecycle. Each handler parses its
//! path, header, and body, then delegates to [`VerificationControl`], which
//! owns authorization, audit, tenancy, and error mapping.

use axum::body::Body;
use axum::extract::rejection::{PathRejection, QueryRejection};
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::{Extension, Json};
use wyrd_spec::error::{WyrdError, WyrdProblem};
use wyrd_spec::ids::{BindingId, ExecutionId};
use wyrd_spec::storage::DownloadInitResponse;
use wyrd_spec::verification::execution::{
    CancelExecutionResponse, EnqueueVerificationResponse, EvidencePlanRequest,
    EvidencePlanResponse, ExecutionPageQuery, InvocationPage, Publication, PublicationPage,
    PublicationSelector, VerificationDetails, VerificationExecutionState, VerificationRequest,
    VerificationResult,
};
use wyrd_spec::verification::{
    MAX_EXECUTE_BODY_BYTES, PublicationName, TaskId, VerificationBindingStatus,
};

use super::service::VerificationControl;
use crate::components::auth::Caller;
use crate::components::storage::routes::extract_idempotency_key;
use crate::http::error::{WyrdErrorResponse, path_rejection, query_rejection};
use crate::http::middleware::edge_timeout::EdgeTimer;
use crate::http::middleware::metrics::RequestArrival;
use crate::state::AppState;
use crate::verification::telemetry::record_submission;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

/// Build the Verification routes for the `/v1` group.
pub fn verification_router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(get_binding))
        .routes(routes!(evidence))
        .routes(routes!(execute))
        .routes(routes!(enqueue))
        .routes(routes!(get_execution))
        .routes(routes!(get_execution_details))
        .routes(routes!(list_invocations))
        .routes(routes!(list_publications))
        .routes(routes!(get_publication))
        .routes(routes!(download_publication))
        .routes(routes!(publication_content))
        .routes(routes!(cancel_execution))
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

/// Open shared upload sessions for the evidence files the tenant holds no
/// completed File upload of.
///
/// Each returned upload is transferred by its plan and finished through
/// `POST /v1/cards/upload/{upload_id}/complete`.
///
/// # Errors
/// Returns a stable Wyrd error when the body is malformed, a file or the
/// request exceeds a limit, the caller lacks `verifier:run`, the inventory
/// cannot be read now, or storage fails.
#[utoipa::path(
    post,
    path = "/verification/evidence",
    request_body = EvidencePlanRequest,
    responses(
        (status = 200, description = "Upload sessions for files without a completed File \
          upload only", body = EvidencePlanResponse),
        (status = 400, description = "The body is malformed \
          (WYRD_VERIFICATION_400_INPUT_INVALID)", body = WyrdProblem),
        (status = 401, description = "The request carried no usable access token \
          (WYRD_AUTH_401_UNAUTHENTICATED, WYRD_AUTH_401_INVALID_TOKEN, \
          WYRD_AUTH_401_TOKEN_EXPIRED)", body = WyrdProblem),
        (status = 403, description = "The principal lacks verifier:run \
          (WYRD_PERMISSION_403_DENIED_RBAC)", body = WyrdProblem),
        (status = 413, description = "A file or the request exceeds a bound \
          (WYRD_VERIFICATION_413_INPUT_TOO_LARGE)", body = WyrdProblem),
        (status = 503, description = "Storage, the object inventory, or Verifier execution is \
          unavailable; retry (WYRD_STORAGE_503_BACKEND_UNAVAILABLE, \
          WYRD_VALA_503_QUERY_VISIBILITY_UNAVAILABLE, WYRD_SERVER_503_SERVICE_UNAVAILABLE)",
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

/// Durably admit one background Verifier execution and return its ID.
///
/// # Errors
/// Returns a stable Wyrd error when the body or Idempotency-Key is malformed,
/// the key names a different request, the caller lacks `verifier:run` on the
/// Verifier, the inputs exceed a bound, this process hosts no Verifier queue,
/// or the submission was lost before admission.
#[utoipa::path(
    post,
    path = "/verification/enqueue",
    request_body = VerificationRequest,
    params(("Idempotency-Key" = Option<String>, Header, description = "Optional retry key; a \
        retry with the same key and body returns the same execution_id")),
    responses(
        (status = 202, description = "Execution durably admitted; poll its status",
         body = EnqueueVerificationResponse),
        (status = 400, description = "The body or Idempotency-Key is invalid \
          (WYRD_VERIFICATION_400_INPUT_INVALID, WYRD_SPEC_400_VALIDATION)", body = WyrdProblem),
        (status = 401, description = "The request carried no usable access token \
          (WYRD_AUTH_401_UNAUTHENTICATED, WYRD_AUTH_401_INVALID_TOKEN, \
          WYRD_AUTH_401_TOKEN_EXPIRED)", body = WyrdProblem),
        (status = 403, description = "The principal lacks verifier:run on the Verifier \
          (WYRD_PERMISSION_403_DENIED_RBAC)", body = WyrdProblem),
        (status = 409, description = "The Idempotency-Key already names a different request \
          (WYRD_REGISTRY_409_IDEMPOTENCY_CONFLICT)", body = WyrdProblem),
        (status = 413, description = "The body or inputs exceed a bound \
          (WYRD_VERIFICATION_413_INPUT_TOO_LARGE)", body = WyrdProblem),
        (status = 503, description = "This process hosts no Verifier queue, or the submission \
          was not admitted and may be retried under its key \
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
    arrival: Option<Extension<RequestArrival>>,
    caller: Caller,
    headers: HeaderMap,
    body: Body,
) -> Result<(StatusCode, Json<EnqueueVerificationResponse>), WyrdErrorResponse> {
    let key = extract_idempotency_key(&headers)?;
    let request: VerificationRequest = decode_body(body).await?;
    let response = VerificationControl::new(&state)
        .enqueue(&caller, request, key.as_ref())
        .await;
    let outcome = if response.is_ok() {
        "staged"
    } else {
        "refused"
    };
    record_submission(arrival.map(|Extension(arrival)| arrival), outcome);
    Ok((StatusCode::ACCEPTED, Json(response?)))
}

/// Read one execution's status, dispatches, and compact result once visible.
///
/// # Errors
/// Returns a stable Wyrd error when the path is malformed, the caller lacks
/// `verifier:read` on the execution's Verifier, the execution is not visible
/// in the caller's tenant, or a read fails.
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
        (status = 403, description = "The principal lacks verifier:read on the execution's \
          Verifier (WYRD_PERMISSION_403_DENIED_RBAC)", body = WyrdProblem),
        (status = 404, description = "No such execution is visible in the caller's tenant \
          yet (WYRD_VERIFICATION_404_EXECUTION_NOT_FOUND)", body = WyrdProblem),
        (status = 503, description = "The registry or the execution records are unavailable, \
          or no verifier is configured for the access token \
          (WYRD_REGISTRY_503_REGISTRY_UNAVAILABLE, WYRD_VALA_503_QUERY_VISIBILITY_UNAVAILABLE, \
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

/// List one page of a queued execution's invocation attempts.
///
/// Progress is read from the execution records, current while history
/// projection lags. A cursor continues the listing at the cut its first
/// page fixed.
///
/// # Errors
/// Returns a stable Wyrd error when the path, limit, or cursor is invalid,
/// the caller lacks `verifier:read` on the execution's Verifier, the queued
/// execution is not in the caller's tenant, or a read fails.
#[utoipa::path(
    get,
    path = "/verification/executions/{execution_id}/invocations",
    params(
        ("execution_id" = ExecutionId, Path, description = "Execution ID returned by \
          POST /v1/verification/enqueue"),
        ExecutionPageQuery
    ),
    responses(
        (status = 200, description = "Invocation attempts in insertion order",
         body = InvocationPage),
        (status = 400, description = "The execution ID, limit, or cursor is invalid \
          (WYRD_SPEC_400_VALIDATION)", body = WyrdProblem),
        (status = 401, description = "The request carried no usable access token \
          (WYRD_AUTH_401_UNAUTHENTICATED, WYRD_AUTH_401_INVALID_TOKEN, \
          WYRD_AUTH_401_TOKEN_EXPIRED)", body = WyrdProblem),
        (status = 403, description = "The principal lacks verifier:read on the execution's \
          Verifier (WYRD_PERMISSION_403_DENIED_RBAC)", body = WyrdProblem),
        (status = 404, description = "No such queued execution in the caller's tenant \
          (WYRD_VERIFICATION_404_EXECUTION_NOT_FOUND)", body = WyrdProblem),
        (status = 503, description = "The registry is unavailable, or no verifier is \
          configured for the access token (WYRD_REGISTRY_503_REGISTRY_UNAVAILABLE, \
          WYRD_AUTH_503_VERIFY_UNAVAILABLE)", body = WyrdProblem)
    ),
    tag = "Verification"
)]
#[tracing::instrument(
    skip(state, caller, query),
    fields(operation = "verification.execution.invocations")
)]
async fn list_invocations(
    State(state): State<AppState>,
    caller: Caller,
    execution_id: Result<Path<ExecutionId>, PathRejection>,
    query: Result<Query<ExecutionPageQuery>, QueryRejection>,
) -> Result<Json<InvocationPage>, WyrdErrorResponse> {
    let Path(execution_id) = execution_id.map_err(|rejection| path_rejection(&rejection))?;
    let Query(query) = query.map_err(|rejection| query_rejection(&rejection))?;
    Ok(Json(
        VerificationControl::new(&state)
            .invocations(&caller, execution_id, &query)
            .await?,
    ))
}

/// List one page of a queued execution's publication names, without content.
///
/// # Errors
/// Returns a stable Wyrd error when the path, limit, or cursor is invalid,
/// the caller lacks `verifier:read` on the execution's Verifier, the queued
/// execution is not in the caller's tenant, or a read fails.
#[utoipa::path(
    get,
    path = "/verification/executions/{execution_id}/publications",
    params(
        ("execution_id" = ExecutionId, Path, description = "Execution ID returned by \
          POST /v1/verification/enqueue"),
        ExecutionPageQuery
    ),
    responses(
        (status = 200, description = "Publications in acceptance order", body = PublicationPage),
        (status = 400, description = "The execution ID, limit, or cursor is invalid \
          (WYRD_SPEC_400_VALIDATION)", body = WyrdProblem),
        (status = 401, description = "The request carried no usable access token \
          (WYRD_AUTH_401_UNAUTHENTICATED, WYRD_AUTH_401_INVALID_TOKEN, \
          WYRD_AUTH_401_TOKEN_EXPIRED)", body = WyrdProblem),
        (status = 403, description = "The principal lacks verifier:read on the execution's \
          Verifier (WYRD_PERMISSION_403_DENIED_RBAC)", body = WyrdProblem),
        (status = 404, description = "No such queued execution in the caller's tenant \
          (WYRD_VERIFICATION_404_EXECUTION_NOT_FOUND)", body = WyrdProblem),
        (status = 503, description = "The registry is unavailable, or no verifier is \
          configured for the access token (WYRD_REGISTRY_503_REGISTRY_UNAVAILABLE, \
          WYRD_AUTH_503_VERIFY_UNAVAILABLE)", body = WyrdProblem)
    ),
    tag = "Verification"
)]
#[tracing::instrument(
    skip(state, caller, query),
    fields(operation = "verification.execution.publications")
)]
async fn list_publications(
    State(state): State<AppState>,
    caller: Caller,
    execution_id: Result<Path<ExecutionId>, PathRejection>,
    query: Result<Query<ExecutionPageQuery>, QueryRejection>,
) -> Result<Json<PublicationPage>, WyrdErrorResponse> {
    let Path(execution_id) = execution_id.map_err(|rejection| path_rejection(&rejection))?;
    let Query(query) = query.map_err(|rejection| query_rejection(&rejection))?;
    Ok(Json(
        VerificationControl::new(&state)
            .publications(&caller, execution_id, &query)
            .await?,
    ))
}

/// Read one publication of an exact producer attempt with its JSON value or
/// immutable manifest.
///
/// # Errors
/// Returns a stable Wyrd error when the path or producer selector is
/// invalid, the caller lacks `verifier:read` on the execution's Verifier,
/// the queued execution or publication is absent, or a read fails.
#[utoipa::path(
    get,
    path = "/verification/executions/{execution_id}/publications/{step_id}/{name}",
    params(
        ("execution_id" = ExecutionId, Path, description = "Execution ID returned by \
          POST /v1/verification/enqueue"),
        ("step_id" = TaskId, Path, description = "The producer step"),
        ("name" = PublicationName, Path, description = "The logical publication name"),
        PublicationSelector
    ),
    responses(
        (status = 200, description = "The publication", body = Publication),
        (status = 400, description = "A path identifier or the producer selector is invalid \
          (WYRD_SPEC_400_VALIDATION)", body = WyrdProblem),
        (status = 401, description = "The request carried no usable access token \
          (WYRD_AUTH_401_UNAUTHENTICATED, WYRD_AUTH_401_INVALID_TOKEN, \
          WYRD_AUTH_401_TOKEN_EXPIRED)", body = WyrdProblem),
        (status = 403, description = "The principal lacks verifier:read on the execution's \
          Verifier (WYRD_PERMISSION_403_DENIED_RBAC)", body = WyrdProblem),
        (status = 404, description = "No such queued execution in the caller's tenant, or no \
          such publication of the producer attempt \
          (WYRD_VERIFICATION_404_EXECUTION_NOT_FOUND, \
          WYRD_VERIFICATION_404_PUBLICATION_NOT_FOUND)", body = WyrdProblem),
        (status = 503, description = "The registry is unavailable, or no verifier is \
          configured for the access token (WYRD_REGISTRY_503_REGISTRY_UNAVAILABLE, \
          WYRD_AUTH_503_VERIFY_UNAVAILABLE)", body = WyrdProblem)
    ),
    tag = "Verification"
)]
#[tracing::instrument(
    skip(state, caller, path, query),
    fields(operation = "verification.execution.publication")
)]
async fn get_publication(
    State(state): State<AppState>,
    caller: Caller,
    path: Result<Path<(ExecutionId, TaskId, PublicationName)>, PathRejection>,
    query: Result<Query<PublicationSelector>, QueryRejection>,
) -> Result<Json<Publication>, WyrdErrorResponse> {
    let Path((execution_id, step, name)) = path.map_err(|rejection| path_rejection(&rejection))?;
    let Query(selector) = query.map_err(|rejection| query_rejection(&rejection))?;
    Ok(Json(
        VerificationControl::new(&state)
            .publication(&caller, execution_id, &step, &name, &selector)
            .await?,
    ))
}

/// Plan the download of one file of a file or directory publication.
///
/// A file publication takes no `path`; a directory publication names one
/// regular file its manifest lists, relative to its root.
///
/// # Errors
/// Returns a stable Wyrd error when the path, producer selector, or file
/// path is invalid, the publication is JSON, the caller lacks
/// `verifier:read` on the execution's Verifier, the queued execution or
/// publication is absent, its stored file descriptor is malformed, or storage
/// fails.
#[utoipa::path(
    post,
    path = "/verification/executions/{execution_id}/publications/{step_id}/{name}/download",
    params(
        ("execution_id" = ExecutionId, Path, description = "Execution ID returned by \
          POST /v1/verification/enqueue"),
        ("step_id" = TaskId, Path, description = "The producer step"),
        ("name" = PublicationName, Path, description = "The logical publication name"),
        PublicationSelector
    ),
    responses(
        (status = 200, description = "Download plan", body = DownloadInitResponse),
        (status = 400, description = "A path identifier, the producer selector, or the file \
          path is invalid, or the publication is JSON (WYRD_SPEC_400_VALIDATION)",
         body = WyrdProblem),
        (status = 401, description = "The request carried no usable access token \
          (WYRD_AUTH_401_UNAUTHENTICATED, WYRD_AUTH_401_INVALID_TOKEN, \
          WYRD_AUTH_401_TOKEN_EXPIRED)", body = WyrdProblem),
        (status = 403, description = "The principal lacks verifier:read on the execution's \
          Verifier (WYRD_PERMISSION_403_DENIED_RBAC)", body = WyrdProblem),
        (status = 404, description = "No such queued execution in the caller's tenant, or no \
          such publication of the producer attempt \
          (WYRD_VERIFICATION_404_EXECUTION_NOT_FOUND, \
          WYRD_VERIFICATION_404_PUBLICATION_NOT_FOUND)", body = WyrdProblem),
        (status = 503, description = "The file's completed upload is not visible yet, storage \
          or the registry is unavailable, or no verifier is configured for the access token; \
          retry (WYRD_VALA_503_QUERY_VISIBILITY_UNAVAILABLE, \
          WYRD_STORAGE_503_BACKEND_UNAVAILABLE, WYRD_REGISTRY_503_REGISTRY_UNAVAILABLE, \
          WYRD_AUTH_503_VERIFY_UNAVAILABLE)", body = WyrdProblem)
    ),
    tag = "Verification"
)]
#[tracing::instrument(
    skip(state, caller, path, query),
    fields(operation = "verification.execution.publication.download")
)]
async fn download_publication(
    State(state): State<AppState>,
    caller: Caller,
    path: Result<Path<(ExecutionId, TaskId, PublicationName)>, PathRejection>,
    query: Result<Query<PublicationSelector>, QueryRejection>,
) -> Result<Json<DownloadInitResponse>, WyrdErrorResponse> {
    let Path((execution_id, step, name)) = path.map_err(|rejection| path_rejection(&rejection))?;
    let Query(selector) = query.map_err(|rejection| query_rejection(&rejection))?;
    Ok(Json(
        VerificationControl::new(&state)
            .download(&caller, execution_id, &step, &name, &selector)
            .await?,
    ))
}

/// Stream the bytes of one publication file on the local storage backend.
///
/// This is the `get_url` a local publication download plan hands out; cloud
/// backends hand out presigned URLs instead. It selects and authorizes
/// exactly as the download plan does, `verifier:read` on the execution's
/// Verifier, so a caller who may plan the download may fetch it.
///
/// # Errors
/// Returns the stable Wyrd errors of the download plan, and a storage error
/// when the object is missing, the backend fails, or the backend is not local.
#[utoipa::path(
    get,
    path = "/verification/executions/{execution_id}/publications/{step_id}/{name}/content",
    params(
        ("execution_id" = ExecutionId, Path, description = "Execution ID returned by \
          POST /v1/verification/enqueue"),
        ("step_id" = TaskId, Path, description = "The producer step"),
        ("name" = PublicationName, Path, description = "The logical publication name"),
        PublicationSelector
    ),
    responses(
        (status = 200, description = "The stored bytes", body = [u8],
         content_type = "application/octet-stream"),
        (status = 400, description = "A path identifier, the producer selector, or the file \
          path is invalid, or the publication is JSON (WYRD_SPEC_400_VALIDATION)",
         body = WyrdProblem),
        (status = 401, description = "The request carried no usable access token \
          (WYRD_AUTH_401_UNAUTHENTICATED, WYRD_AUTH_401_INVALID_TOKEN, \
          WYRD_AUTH_401_TOKEN_EXPIRED)", body = WyrdProblem),
        (status = 403, description = "The principal lacks verifier:read on the execution's \
          Verifier (WYRD_PERMISSION_403_DENIED_RBAC)", body = WyrdProblem),
        (status = 404, description = "No such queued execution in the caller's tenant, no \
          such publication of the producer attempt, or no such stored object \
          (WYRD_VERIFICATION_404_EXECUTION_NOT_FOUND, \
          WYRD_VERIFICATION_404_PUBLICATION_NOT_FOUND, WYRD_STORAGE_404_OBJECT_NOT_FOUND)",
         body = WyrdProblem),
        (status = 500, description = "The storage backend failed or is not local \
          (WYRD_STORAGE_500_BACKEND, WYRD_SPEC_500_INTERNAL)", body = WyrdProblem),
        (status = 503, description = "The file's completed upload is not visible yet, storage \
          or the registry is unavailable, or no verifier is configured for the access token; \
          retry (WYRD_VALA_503_QUERY_VISIBILITY_UNAVAILABLE, \
          WYRD_STORAGE_503_BACKEND_UNAVAILABLE, WYRD_REGISTRY_503_REGISTRY_UNAVAILABLE, \
          WYRD_AUTH_503_VERIFY_UNAVAILABLE)", body = WyrdProblem)
    ),
    tag = "Verification"
)]
#[tracing::instrument(
    skip(state, caller, path, query),
    fields(operation = "verification.execution.publication.content")
)]
async fn publication_content(
    State(state): State<AppState>,
    caller: Caller,
    path: Result<Path<(ExecutionId, TaskId, PublicationName)>, PathRejection>,
    query: Result<Query<PublicationSelector>, QueryRejection>,
) -> Result<Response, WyrdErrorResponse> {
    let Path((execution_id, step, name)) = path.map_err(|rejection| path_rejection(&rejection))?;
    let Query(selector) = query.map_err(|rejection| query_rejection(&rejection))?;
    let (stream, len) = VerificationControl::new(&state)
        .content(&caller, execution_id, &step, &name, &selector)
        .await?;
    Ok((
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "application/octet-stream"),
            (header::CONTENT_LENGTH, &len.to_string()),
        ],
        Body::from_stream(stream),
    )
        .into_response())
}

/// Cancel a queued execution and answer its status after the fence commits.
///
/// Unfinished work settles `cancelled`; settled work keeps its status,
/// result, and publications, so a repeat answers the current status.
///
/// # Errors
/// Returns a stable Wyrd error when the path is malformed, the caller lacks
/// `verifier:run` on the execution's Verifier, the queued execution is not in
/// the caller's tenant, or the write fails.
#[utoipa::path(
    post,
    path = "/verification/executions/{execution_id}/cancel",
    params(("execution_id" = ExecutionId, Path, description = "Execution ID returned by \
        POST /v1/verification/enqueue")),
    responses(
        (status = 200, description = "The execution's status after cancellation",
         body = CancelExecutionResponse),
        (status = 400, description = "The execution ID is not a valid UUID \
          (WYRD_SPEC_400_VALIDATION)", body = WyrdProblem),
        (status = 401, description = "The request carried no usable access token \
          (WYRD_AUTH_401_UNAUTHENTICATED, WYRD_AUTH_401_INVALID_TOKEN, \
          WYRD_AUTH_401_TOKEN_EXPIRED)", body = WyrdProblem),
        (status = 403, description = "The principal lacks verifier:run on the execution's \
          Verifier (WYRD_PERMISSION_403_DENIED_RBAC)", body = WyrdProblem),
        (status = 404, description = "No such queued execution in the caller's tenant \
          (WYRD_VERIFICATION_404_EXECUTION_NOT_FOUND)", body = WyrdProblem),
        (status = 503, description = "The registry is unavailable, or no verifier is \
          configured for the access token (WYRD_REGISTRY_503_REGISTRY_UNAVAILABLE, \
          WYRD_AUTH_503_VERIFY_UNAVAILABLE)", body = WyrdProblem)
    ),
    tag = "Verification"
)]
#[tracing::instrument(
    skip(state, caller),
    fields(operation = "verification.execution.cancel")
)]
async fn cancel_execution(
    State(state): State<AppState>,
    caller: Caller,
    execution_id: Result<Path<ExecutionId>, PathRejection>,
) -> Result<Json<CancelExecutionResponse>, WyrdErrorResponse> {
    let Path(execution_id) = execution_id.map_err(|rejection| path_rejection(&rejection))?;
    Ok(Json(
        VerificationControl::new(&state)
            .cancel(&caller, execution_id)
            .await?,
    ))
}

/// Read what one completed execution ran: its exact authored checks,
/// provenance, steps, and pinned evidence beside the same judgments.
///
/// # Errors
/// Returns a stable Wyrd error when the path is malformed, the caller lacks
/// `verifier:read` on the execution's Verifier, no complete details are
/// visible in the caller's tenant, or a read fails.
#[utoipa::path(
    get,
    path = "/verification/executions/{execution_id}/details",
    params(("execution_id" = ExecutionId, Path, description = "Execution ID returned by \
        POST /v1/verification/execute or POST /v1/verification/enqueue")),
    responses(
        (status = 200, description = "Execution details", body = VerificationDetails),
        (status = 400, description = "The execution ID is not a valid UUID \
          (WYRD_SPEC_400_VALIDATION)", body = WyrdProblem),
        (status = 401, description = "The request carried no usable access token \
          (WYRD_AUTH_401_UNAUTHENTICATED, WYRD_AUTH_401_INVALID_TOKEN, \
          WYRD_AUTH_401_TOKEN_EXPIRED)", body = WyrdProblem),
        (status = 403, description = "The principal lacks verifier:read on the execution's \
          Verifier (WYRD_PERMISSION_403_DENIED_RBAC)", body = WyrdProblem),
        (status = 404, description = "No complete details of such an execution are visible \
          in the caller's tenant: it is unknown, not yet recorded, or no longer readable \
          (WYRD_VERIFICATION_404_EXECUTION_NOT_FOUND)", body = WyrdProblem),
        (status = 503, description = "The registry is unavailable, or no verifier is \
          configured for the access token (WYRD_REGISTRY_503_REGISTRY_UNAVAILABLE, \
          WYRD_AUTH_503_VERIFY_UNAVAILABLE)", body = WyrdProblem)
    ),
    tag = "Verification"
)]
#[tracing::instrument(
    skip(state, caller),
    fields(operation = "verification.execution.details.read")
)]
async fn get_execution_details(
    State(state): State<AppState>,
    caller: Caller,
    execution_id: Result<Path<ExecutionId>, PathRejection>,
) -> Result<Json<VerificationDetails>, WyrdErrorResponse> {
    let Path(execution_id) = execution_id.map_err(|rejection| path_rejection(&rejection))?;
    Ok(Json(
        VerificationControl::new(&state)
            .get_execution_details(&caller, execution_id)
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
    level = "debug",
    skip(state, caller, body),
    fields(operation = "verification.execute")
)]
async fn execute(
    State(state): State<AppState>,
    arrival: Option<Extension<RequestArrival>>,
    edge_timer: Option<Extension<EdgeTimer>>,
    caller: Caller,
    body: Body,
) -> Result<Json<VerificationResult>, WyrdErrorResponse> {
    let request: VerificationRequest = decode_body(body).await?;
    Ok(Json(
        VerificationControl::new(&state)
            .execute(
                &caller,
                request,
                edge_timer.map(|Extension(timer)| timer).as_ref(),
                arrival.map(|Extension(arrival)| arrival),
            )
            .await?,
    ))
}
