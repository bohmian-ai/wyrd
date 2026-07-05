//! Axum adapters for storage service functions.

use std::str::FromStr;

use axum::body::{Body, Bytes};
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post, put};
use axum::{Json, Router};
use serde::Deserialize;
use tokio_util::io::ReaderStream;
use wyrd_runtime::{Permission, PrincipalKind};
use wyrd_spec::error::WyrdError;
use wyrd_spec::ids::IdempotencyKey;
use wyrd_spec::storage::{
    AbortResponse, DownloadInitRequest, IDEMPOTENCY_KEY_HEADER, LocalBlobUploadResponse,
    PartUrlResponse, UploadCompleteRequest, UploadId, UploadInitRequest,
};
use wyrd_storage::BackendConfig;
use wyrd_storage::service::{self, StorageCaller, StoragePrincipalKind, StorageSubject};

use crate::components::auth::Caller;
use crate::http::error::WyrdErrorResponse;
use crate::state::AppState;

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
    authorize_card_write(&caller)?;
    let idempotency_key = extract_idempotency_key(&headers)?;
    let storage_caller = storage_caller(&caller);
    service::upload_init(
        &state.storage,
        state.postgres.wyrd(),
        &storage_caller,
        idempotency_key,
        body,
    )
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
    authorize_card_write(&caller)?;
    let upload_id = parse_upload_id(&id)?;
    let storage_caller = storage_caller(&caller);
    service::upload_part_url(
        &state.storage,
        state.postgres.wyrd(),
        &storage_caller,
        upload_id,
        query.part_number,
    )
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
    authorize_card_write(&caller)?;
    let upload_id = parse_upload_id(&id)?;
    let storage_caller = storage_caller(&caller);
    service::upload_complete(
        &state.storage,
        state.postgres.wyrd(),
        &storage_caller,
        upload_id,
        body,
    )
    .await
    .map(Json)
    .map_err(WyrdErrorResponse::from)
}

async fn abort(
    State(state): State<AppState>,
    caller: Caller,
    Path(id): Path<String>,
) -> Result<Json<AbortResponse>, WyrdErrorResponse> {
    authorize_card_write(&caller)?;
    let upload_id = parse_upload_id(&id)?;
    let storage_caller = storage_caller(&caller);
    service::upload_abort(
        &state.storage,
        state.postgres.wyrd(),
        &storage_caller,
        upload_id,
    )
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
    authorize_card_write(&caller)?;
    service::upload_local_blob(&state.storage, caller.data_tenant_id, path, &body)
        .await
        .map_err(WyrdErrorResponse::from)?;
    Ok(Json(LocalBlobUploadResponse { uploaded: true }))
}

async fn download_init(
    State(state): State<AppState>,
    caller: Caller,
    Json(body): Json<DownloadInitRequest>,
) -> Result<Json<wyrd_spec::storage::DownloadInitResponse>, WyrdErrorResponse> {
    authorize_card_read(&caller)?;
    let storage_caller = storage_caller(&caller);
    service::download_init(&state.storage, state.postgres.wyrd(), &storage_caller, body)
        .await
        .map(Json)
        .map_err(WyrdErrorResponse::from)
}

async fn download_local_blob(
    State(state): State<AppState>,
    caller: Caller,
    Path(path): Path<String>,
) -> Result<Response, WyrdErrorResponse> {
    authorize_card_read(&caller)?;
    let (file, len) = service::download_local_blob(&state.storage, caller.data_tenant_id, path)
        .await
        .map_err(WyrdErrorResponse::from)?;
    let stream = ReaderStream::new(file);

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

fn parse_upload_id(value: &str) -> Result<UploadId, WyrdErrorResponse> {
    UploadId::from_str(value)
        .map_err(service::invalid_upload_id)
        .map_err(WyrdErrorResponse::from)
}

fn extract_idempotency_key(
    headers: &HeaderMap,
) -> Result<Option<IdempotencyKey>, WyrdErrorResponse> {
    headers
        .get(IDEMPOTENCY_KEY_HEADER)
        .map(|value| {
            value
                .to_str()
                .map_err(|_| WyrdError::Validation {
                    message: "idempotency key header is not valid UTF-8".to_owned(),
                    details: serde_json::json!({ "header": IDEMPOTENCY_KEY_HEADER }),
                })
                .and_then(|value| {
                    IdempotencyKey::new(value).map_err(|error| WyrdError::Validation {
                        message: "idempotency key is invalid".to_owned(),
                        details: serde_json::json!({
                            "header": IDEMPOTENCY_KEY_HEADER,
                            "source": error.to_string(),
                        }),
                    })
                })
        })
        .transpose()
        .map_err(WyrdErrorResponse::from)
}

fn authorize_card_write(caller: &Caller) -> Result<(), WyrdErrorResponse> {
    authorize(caller, Permission::card_write(), "card:write")
}

fn authorize_card_read(caller: &Caller) -> Result<(), WyrdErrorResponse> {
    authorize(caller, Permission::card_read(), "card:read")
}

fn authorize(
    caller: &Caller,
    required: Permission,
    label: &'static str,
) -> Result<(), WyrdErrorResponse> {
    if caller.principal.effective_permissions.contains(&required) {
        return Ok(());
    }
    Err(WyrdErrorResponse::from(WyrdError::PermissionDeniedRbac {
        message: format!("caller lacks required permission {label}"),
        details: serde_json::json!({ "required": required }),
    }))
}

fn storage_caller(caller: &Caller) -> StorageCaller {
    StorageCaller {
        data_tenant_id: caller.data_tenant_id,
        subject: StorageSubject {
            principal_id: caller.principal.id.as_uuid(),
            kind: match &caller.principal.kind {
                PrincipalKind::User => StoragePrincipalKind::User,
                PrincipalKind::Service { .. } => StoragePrincipalKind::Service,
                PrincipalKind::Agent { .. } => StoragePrincipalKind::Agent,
            },
        },
        request_id: caller.request_id.clone(),
    }
}
