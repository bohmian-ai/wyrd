//! Axum adapters for storage service functions.

use std::str::FromStr;

use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use axum::response::Response;
use axum::routing::{get, post, put};
use axum::{Json, Router};
use serde::Deserialize;
use wyrd_spec::storage::{
    AbortResponse, DownloadInitRequest, LocalBlobUploadResponse, PartUrlResponse,
    UploadCompleteRequest, UploadId, UploadInitRequest,
};
use wyrd_storage::BackendConfig;

use crate::auth::Caller;
use crate::http::error::WyrdErrorResponse;
use crate::state::AppState;
use crate::storage::service;

/// Build storage routes for the `/v1` group.
pub fn storage_router(state: &AppState) -> Router<AppState> {
    let router = Router::new()
        .route("/cards/upload/init", post(init))
        .route("/cards/upload/{id}/part-url", post(part_url))
        .route("/cards/upload/{id}/complete", post(complete))
        .route("/cards/upload/{id}/abort", post(abort))
        .route("/cards/download/init", post(download_init));

    if matches!(state.storage.backend_config(), BackendConfig::Local { .. }) {
        router
            .route("/cards/upload/local/{*path}", put(local_blob))
            .route("/cards/download/local/{*path}", get(download_local_blob))
    } else {
        router
    }
}

async fn init(
    State(state): State<AppState>,
    caller: Caller,
    headers: HeaderMap,
    Json(body): Json<UploadInitRequest>,
) -> Result<Json<wyrd_spec::storage::UploadInitResponse>, WyrdErrorResponse> {
    service::upload_init(&state, caller, &headers, body)
        .await
        .map(Json)
        .map_err(WyrdErrorResponse::from)
}

#[derive(Debug, Deserialize)]
struct PartUrlQuery {
    part_number: u32,
}

async fn part_url(
    State(state): State<AppState>,
    caller: Caller,
    Path(id): Path<String>,
    Query(query): Query<PartUrlQuery>,
) -> Result<Json<PartUrlResponse>, WyrdErrorResponse> {
    let upload_id = parse_upload_id(&id)?;
    service::upload_part_url(&state, caller, upload_id, query.part_number)
        .await
        .map(Json)
        .map_err(WyrdErrorResponse::from)
}

async fn complete(
    State(state): State<AppState>,
    caller: Caller,
    Path(id): Path<String>,
    Json(body): Json<UploadCompleteRequest>,
) -> Result<Json<wyrd_spec::storage::UploadCompleteResponse>, WyrdErrorResponse> {
    let upload_id = parse_upload_id(&id)?;
    service::upload_complete(&state, caller, upload_id, body)
        .await
        .map(Json)
        .map_err(WyrdErrorResponse::from)
}

async fn abort(
    State(state): State<AppState>,
    caller: Caller,
    Path(id): Path<String>,
) -> Result<Json<AbortResponse>, WyrdErrorResponse> {
    let upload_id = parse_upload_id(&id)?;
    service::upload_abort(&state, caller, upload_id)
        .await
        .map(Json)
        .map_err(WyrdErrorResponse::from)
}

async fn local_blob(
    State(state): State<AppState>,
    caller: Caller,
    Path(path): Path<String>,
    body: Bytes,
) -> Result<Json<LocalBlobUploadResponse>, WyrdErrorResponse> {
    service::upload_local_blob(&state, caller, path, body)
        .await
        .map_err(WyrdErrorResponse::from)?;
    Ok(Json(LocalBlobUploadResponse { uploaded: true }))
}

async fn download_init(
    State(state): State<AppState>,
    caller: Caller,
    Json(body): Json<DownloadInitRequest>,
) -> Result<Json<wyrd_spec::storage::DownloadInitResponse>, WyrdErrorResponse> {
    service::download_init(&state, caller, body)
        .await
        .map(Json)
        .map_err(WyrdErrorResponse::from)
}

async fn download_local_blob(
    State(state): State<AppState>,
    caller: Caller,
    Path(path): Path<String>,
) -> Result<Response, WyrdErrorResponse> {
    service::download_local_blob(&state, caller, path)
        .await
        .map_err(WyrdErrorResponse::from)
}

fn parse_upload_id(value: &str) -> Result<UploadId, WyrdErrorResponse> {
    UploadId::from_str(value)
        .map_err(service::invalid_upload_id)
        .map_err(WyrdErrorResponse::from)
}
