//! Axum adapter for accepted server Workflow runs.
//!
//! Authentication is enforced by the `/v1` middleware layer and the
//! [`Caller`] extractor runs before any other extractor, so every request is
//! authenticated before its key or body is read. Each handler only parses
//! the request and delegates to [`WorkflowRunHost`].

use axum::Json;
use axum::extract::rejection::JsonRejection;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use wyrd_spec::card::workflow::{CreateWorkflowRunRequest, WorkflowRun};
use wyrd_spec::error::{WyrdError, WyrdProblem};
use wyrd_spec::ids::IdempotencyKey;
use wyrd_spec::storage::IDEMPOTENCY_KEY_HEADER;

use super::host::{Created, WorkflowRunHost};
use crate::components::auth::Caller;
use crate::http::error::WyrdErrorResponse;
use crate::state::AppState;

/// Build the Workflow run routes for the `/v1` group.
pub fn workflow_runs_router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(create_workflow_run_http))
        .routes(routes!(get_workflow_run_http))
        .routes(routes!(cancel_workflow_run_http))
}

/// Accept a registered Workflow for asynchronous execution.
///
/// The first acceptance of a key answers `202` with the queued run; the same
/// request under the same key, before or after acceptance, answers `200`
/// with the run's current snapshot.
///
/// # Errors
/// Returns `WYRD_WORKFLOW_422_RUN_REQUEST` for a missing or malformed
/// `Idempotency-Key` or body, and every error of [`WorkflowRunHost::create`].
#[utoipa::path(
    post,
    path = "/workflow-runs",
    request_body = CreateWorkflowRunRequest,
    params(
        ("Idempotency-Key" = String, Header, description = "Stable key reused for retries", example = "workflow-run-001")
    ),
    responses(
        (status = 202, description = "Workflow run accepted and queued", body = WorkflowRun),
        (status = 200, description = "Replay of the same request under the same key", body = WorkflowRun),
        (status = 401, description = "The request carried no usable access token \
          (WYRD_AUTH_401_UNAUTHENTICATED, WYRD_AUTH_401_INVALID_TOKEN, \
          WYRD_AUTH_401_TOKEN_EXPIRED)", body = WyrdProblem),
        (status = 403, description = "The principal may not run Workflows \
          (WYRD_PERMISSION_403_DENIED_RBAC, WYRD_AUTH_403_PRINCIPAL_ORPHANED)", body = WyrdProblem),
        (status = 404, description = "No such Workflow Card is visible to this tenant \
          (WYRD_REGISTRY_404_CARD_NOT_FOUND)", body = WyrdProblem),
        (status = 409, description = "The idempotency key was used with a different request \
          (WYRD_WORKFLOW_409_IDEMPOTENCY_CONFLICT)", body = WyrdProblem),
        (status = 413, description = "The input or graph exceeds a server bound \
          (WYRD_WORKFLOW_413_INPUT_TOO_LARGE, WYRD_WORKFLOW_413_GRAPH_TOO_LARGE)", body = WyrdProblem),
        (status = 422, description = "The request, graph, route, or tools cannot run on this server \
          (WYRD_WORKFLOW_422_RUN_REQUEST, WYRD_WORKFLOW_422_ROUTE_UNSUPPORTED, \
          WYRD_WORKFLOW_422_SERVER_NATIVE_UNSUPPORTED, WYRD_WORKFLOW_422_TOOL_UNAVAILABLE, \
          WYRD_REGISTRY_422_UNRESOLVED_DEPENDENCY)", body = WyrdProblem),
        (status = 429, description = "Workflow run capacity is exhausted \
          (WYRD_WORKFLOW_429_RUN_CAPACITY)", body = WyrdProblem),
        (status = 500, description = "The authorization decision could not be audited \
          (WYRD_VALA_500_AUDIT_UNAVAILABLE)", body = WyrdProblem),
        (status = 503, description = "The server is shutting down, a route binding is \
          unavailable, or the registry is unavailable (WYRD_WORKFLOW_503_RUN_UNAVAILABLE, \
          WYRD_WORKFLOW_503_BINDING_UNAVAILABLE, WYRD_REGISTRY_503_REGISTRY_UNAVAILABLE)", body = WyrdProblem)
    ),
    tag = "Workflows"
)]
#[tracing::instrument(skip_all, fields(operation = "workflow.run.create"))]
async fn create_workflow_run_http(
    State(state): State<AppState>,
    caller: Caller,
    headers: HeaderMap,
    request: Result<Json<CreateWorkflowRunRequest>, JsonRejection>,
) -> Result<(StatusCode, Json<WorkflowRun>), WyrdErrorResponse> {
    let key = idempotency_key(&headers)?;
    let Json(request) = request.map_err(|_| WyrdError::WorkflowRunRequest {
        message: "the body is not a valid Workflow run request".to_owned(),
        details: serde_json::json!({ "field": "body" }),
    })?;
    let (created, run) = WorkflowRunHost::new(state)
        .create(&caller, key, request)
        .await?;
    let status = match created {
        Created::Accepted => StatusCode::ACCEPTED,
        Created::Replayed => StatusCode::OK,
    };
    Ok((status, Json(run)))
}

/// Read the current snapshot of one of the caller's Workflow runs.
///
/// # Errors
/// Returns every error of [`WorkflowRunHost::get`].
#[utoipa::path(
    get,
    path = "/workflow-runs/{run_id}",
    params(("run_id" = String, Path, description = "Workflow run id")),
    responses(
        (status = 200, description = "Current Workflow run snapshot", body = WorkflowRun),
        (status = 401, description = "The request carried no usable access token \
          (WYRD_AUTH_401_UNAUTHENTICATED, WYRD_AUTH_401_INVALID_TOKEN, \
          WYRD_AUTH_401_TOKEN_EXPIRED)", body = WyrdProblem),
        (status = 403, description = "The principal may not run Workflows \
          (WYRD_PERMISSION_403_DENIED_RBAC, WYRD_AUTH_403_PRINCIPAL_ORPHANED)", body = WyrdProblem),
        (status = 404, description = "No such run is visible to the caller on this replica \
          (WYRD_WORKFLOW_404_RUN_NOT_FOUND)", body = WyrdProblem),
        (status = 500, description = "The authorization decision could not be audited \
          (WYRD_VALA_500_AUDIT_UNAVAILABLE)", body = WyrdProblem)
    ),
    tag = "Workflows"
)]
#[tracing::instrument(skip_all, fields(operation = "workflow.run.read"))]
async fn get_workflow_run_http(
    State(state): State<AppState>,
    caller: Caller,
    Path(run_id): Path<String>,
) -> Result<Json<WorkflowRun>, WyrdErrorResponse> {
    Ok(Json(
        WorkflowRunHost::new(state).get(&caller, &run_id).await?,
    ))
}

/// Cancel one of the caller's Workflow runs and return its terminal snapshot.
///
/// # Errors
/// Returns every error of [`WorkflowRunHost::cancel`].
#[utoipa::path(
    post,
    path = "/workflow-runs/{run_id}/cancel",
    params(("run_id" = String, Path, description = "Workflow run id")),
    responses(
        (status = 200, description = "The terminal Workflow run snapshot that won", body = WorkflowRun),
        (status = 401, description = "The request carried no usable access token \
          (WYRD_AUTH_401_UNAUTHENTICATED, WYRD_AUTH_401_INVALID_TOKEN, \
          WYRD_AUTH_401_TOKEN_EXPIRED)", body = WyrdProblem),
        (status = 403, description = "The principal may not run Workflows \
          (WYRD_PERMISSION_403_DENIED_RBAC, WYRD_AUTH_403_PRINCIPAL_ORPHANED)", body = WyrdProblem),
        (status = 404, description = "No such run is visible to the caller on this replica \
          (WYRD_WORKFLOW_404_RUN_NOT_FOUND)", body = WyrdProblem),
        (status = 500, description = "The authorization decision could not be audited \
          (WYRD_VALA_500_AUDIT_UNAVAILABLE)", body = WyrdProblem)
    ),
    tag = "Workflows"
)]
#[tracing::instrument(skip_all, fields(operation = "workflow.run.cancel"))]
async fn cancel_workflow_run_http(
    State(state): State<AppState>,
    caller: Caller,
    Path(run_id): Path<String>,
) -> Result<Json<WorkflowRun>, WyrdErrorResponse> {
    Ok(Json(
        WorkflowRunHost::new(state).cancel(&caller, &run_id).await?,
    ))
}

/// The required `Idempotency-Key` of a create.
///
/// # Errors
/// Returns `WYRD_WORKFLOW_422_RUN_REQUEST` naming the header when it is
/// missing, not UTF-8, or not a valid key.
fn idempotency_key(headers: &HeaderMap) -> Result<IdempotencyKey, WyrdError> {
    let invalid = |reason: &str| WyrdError::WorkflowRunRequest {
        message: format!("Idempotency-Key {reason}"),
        details: serde_json::json!({ "field": IDEMPOTENCY_KEY_HEADER }),
    };
    let value = headers
        .get(IDEMPOTENCY_KEY_HEADER)
        .ok_or_else(|| invalid("header is required"))?
        .to_str()
        .map_err(|_| invalid("header is not valid UTF-8"))?;
    IdempotencyKey::new(value).map_err(|_| invalid("header is not a valid key"))
}
