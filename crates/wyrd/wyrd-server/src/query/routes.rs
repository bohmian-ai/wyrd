//! Axum adapters for the query service functions.
//!
//! The sync route returns a raw Arrow IPC stream (`application/vnd.apache.arrow.stream`)
//! with the `X-Wyrd-Schema-Fingerprint` / `X-Wyrd-Row-Count` headers, so it builds
//! a `Response` directly rather than `Json`. The async submit/status routes are
//! ordinary JSON. Every route delegates to `service`, which authorizes and runs
//! the floor before execution, and maps errors through the single `WyrdErrorResponse`.

use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::{StatusCode, header};
use axum::response::Response;
use axum::routing::{get, post};
use axum::{Json, Router};
use wyrd_spec::error::WyrdError;
use wyrd_spec::vala::api::{
    AsyncQueryRequest, AsyncQueryResponse, AsyncQueryStatus, JobUid, SyncQueryRequest,
};

use crate::auth::Caller;
use crate::error::WyrdErrorResponse;
use crate::query::service;
use crate::state::AppState;

/// Content type of the sync-query Arrow IPC stream response.
const ARROW_STREAM_CONTENT_TYPE: &str = "application/vnd.apache.arrow.stream";

/// Mount the query routes into an existing `/v1` router.
pub fn mount(router: Router<AppState>, _state: &AppState) -> Router<AppState> {
    router
        .route("/query", post(sync_query))
        .route("/query/async", post(async_query))
        .route("/query/async/{job_uid}", get(async_query_status))
}

async fn sync_query(
    State(state): State<AppState>,
    caller: Caller,
    Json(body): Json<SyncQueryRequest>,
) -> Result<Response, WyrdErrorResponse> {
    let result = service::run_sync_query(&state, caller, body)
        .await
        .map_err(WyrdErrorResponse::from)?;

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, ARROW_STREAM_CONTENT_TYPE)
        .header("x-wyrd-schema-fingerprint", result.schema_fingerprint)
        .header("x-wyrd-row-count", result.row_count.to_string())
        .body(Body::from(result.body))
        .map_err(|error| {
            WyrdErrorResponse::from(WyrdError::Internal {
                message: "failed to build query response".to_owned(),
                details: serde_json::json!({ "detail": error.to_string() }),
            })
        })
}

async fn async_query(
    State(state): State<AppState>,
    caller: Caller,
    Json(body): Json<AsyncQueryRequest>,
) -> Result<Json<AsyncQueryResponse>, WyrdErrorResponse> {
    service::submit_async_query(&state, caller, body)
        .await
        .map(Json)
        .map_err(WyrdErrorResponse::from)
}

async fn async_query_status(
    State(state): State<AppState>,
    caller: Caller,
    Path(job_uid): Path<JobUid>,
) -> Result<Json<AsyncQueryStatus>, WyrdErrorResponse> {
    service::get_async_query_status(&state, caller, job_uid)
        .await
        .map(Json)
        .map_err(WyrdErrorResponse::from)
}
