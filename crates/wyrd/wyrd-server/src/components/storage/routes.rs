//! Axum adapters for storage service functions.
//!
//! Authentication is applied once to the whole `/v1` router by HTTP
//! middleware. These handlers add the operation-specific `card_read` or
//! `card_write` permission before calling storage services; keeping that check
//! beside the route makes the capability required by each operation explicit.

use std::str::FromStr;

use axum::body::{Body, Bytes};
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post, put};
use axum::{Json, Router};
use serde::Deserialize;
use tokio_util::io::ReaderStream;
use wyrd_runtime::Permission;
use wyrd_spec::error::{WyrdError, WyrdProblem};
use wyrd_spec::ids::IdempotencyKey;
use wyrd_spec::storage::{
    AbortResponse, DownloadInitRequest, IDEMPOTENCY_KEY_HEADER, LocalBlobUploadResponse,
    PartUrlResponse, UploadCompleteRequest, UploadId, UploadInitRequest,
};
use wyrd_storage::BackendConfig;
use wyrd_storage::service::{self, StorageCaller};

use crate::audit;
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
            .route("/cards/upload/local/{*id}", put(local_blob))
            .route("/cards/download/local/{*path}", get(download_local_blob))
    } else {
        router
    }
}

/// Create or replay a storage upload initialization.
#[utoipa::path(
    post,
    path = "/v1/cards/upload/init",
    request_body = UploadInitRequest,
    params(("Idempotency-Key" = Option<String>, Header, description = "Replays an \
      initialization instead of planning a second upload")),
    responses(
        (status = 200, description = "Upload plan, replayed unchanged for a repeated \
          idempotency key", body = wyrd_spec::storage::UploadInitResponse),
        (status = 400, description = "The idempotency key, the artifact path, or the declared \
          digest or size is invalid (WYRD_SPEC_400_VALIDATION, \
          WYRD_STORAGE_400_TENANT_PATH_MISMATCH, WYRD_STORAGE_400_ARTIFACT_TOO_LARGE, \
          WYRD_STORAGE_400_SHA256_INVALID, WYRD_STORAGE_400_SIZE_INVALID)", body = WyrdProblem),
        (status = 401, description = "The request carried no usable access token \
          (WYRD_AUTH_401_UNAUTHENTICATED, WYRD_AUTH_401_INVALID_TOKEN, \
          WYRD_AUTH_401_TOKEN_EXPIRED, WYRD_AUTH_401_CREDENTIAL_REVOKED)", body = WyrdProblem),
        (status = 403, description = "The principal lacks card write, or the object belongs to \
          another tenant (WYRD_PERMISSION_403_DENIED_RBAC, WYRD_STORAGE_403_UPLOAD_FOREIGN_TENANT)", body = WyrdProblem),
        (status = 409, description = "The upload record conflicts with one already stored \
          (WYRD_SPEC_409_CONFLICT)", body = WyrdProblem),
        (status = 500, description = "The storage backend or the platform store failed, or the \
          decision could not be audited (WYRD_STORAGE_500_BACKEND, WYRD_SPEC_500_INTERNAL, \
          WYRD_VALA_500_AUDIT_UNAVAILABLE)", body = WyrdProblem),
        (status = 503, description = "The revocation store could not vouch for the token, or \
          the storage backend is transiently unavailable \
          (WYRD_AUTH_503_VERIFY_UNAVAILABLE, \
          WYRD_STORAGE_503_BACKEND_UNAVAILABLE)", body = WyrdProblem)
    ),
    tag = "Storage"
)]
async fn init(
    State(state): State<AppState>,
    caller: Caller,
    headers: HeaderMap,
    Json(body): Json<UploadInitRequest>,
) -> Result<Json<wyrd_spec::storage::UploadInitResponse>, WyrdErrorResponse> {
    authorize_card_write(
        &state,
        &caller,
        "storage.upload.init",
        &format!("card:{}", body.card_uid),
    )
    .await?;
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
    /// One-based multipart part number requested by the client.
    part_number: u32,
}

/// Mint a URL or equivalent protocol data for one upload part.
#[utoipa::path(
    post,
    path = "/v1/cards/upload/{id}/part-url",
    params(
        ("id" = String, Path, description = "Upload the part belongs to"),
        ("part_number" = u32, Query, description = "One-based multipart part number")
    ),
    responses(
        (status = 200, description = "Part upload instructions", body = PartUrlResponse),
        (status = 400, description = "The upload identifier is not one this server minted \
          (WYRD_STORAGE_400_INVALID_UPLOAD_ID)", body = WyrdProblem),
        (status = 401, description = "The request carried no usable access token \
          (WYRD_AUTH_401_UNAUTHENTICATED, WYRD_AUTH_401_INVALID_TOKEN, \
          WYRD_AUTH_401_TOKEN_EXPIRED, WYRD_AUTH_401_CREDENTIAL_REVOKED)", body = WyrdProblem),
        (status = 403, description = "The principal lacks card write, or the object belongs to \
          another tenant (WYRD_PERMISSION_403_DENIED_RBAC, WYRD_STORAGE_403_UPLOAD_FOREIGN_TENANT)", body = WyrdProblem),
        (status = 404, description = "No such upload (WYRD_STORAGE_404_UPLOAD_NOT_FOUND)",
         body = WyrdProblem),
        (status = 409, description = "The upload is already terminal \
          (WYRD_STORAGE_409_UPLOAD_NOT_PENDING)", body = WyrdProblem),
        (status = 500, description = "The storage backend or the platform store failed, or the \
          decision could not be audited (WYRD_STORAGE_500_BACKEND, WYRD_SPEC_500_INTERNAL, \
          WYRD_VALA_500_AUDIT_UNAVAILABLE)", body = WyrdProblem),
        (status = 503, description = "The revocation store could not vouch for the token, or \
          the storage backend is transiently unavailable \
          (WYRD_AUTH_503_VERIFY_UNAVAILABLE, \
          WYRD_STORAGE_503_BACKEND_UNAVAILABLE, WYRD_STORAGE_503_PRESIGN_EXPIRED)", body = WyrdProblem)
    ),
    tag = "Storage"
)]
async fn part_url(
    State(state): State<AppState>,
    caller: Caller,
    Path(id): Path<String>,
    Query(query): Query<PartUrlQuery>,
) -> Result<Json<PartUrlResponse>, WyrdErrorResponse> {
    let upload_id = parse_upload_id(&id)?;
    authorize_card_write(
        &state,
        &caller,
        "storage.upload.part_url",
        &format!("upload:{upload_id}"),
    )
    .await?;
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

/// Complete an initialized upload after the client has transferred its bytes.
///
/// The optional `Idempotency-Key` is forwarded to the storage service so a
/// retried client completion keeps the same request context. The service
/// verifies the backend object before persisting its artifact metadata.
#[utoipa::path(
    post,
    path = "/v1/cards/upload/{id}/complete",
    params(
        ("id" = String, Path, description = "Upload to finish"),
        ("Idempotency-Key" = Option<String>, Header, description = "Correlates a retried \
          completion with the original request")
    ),
    request_body = UploadCompleteRequest,
    responses(
        (status = 200, description = "The stored artifact's recorded metadata",
         body = wyrd_spec::storage::UploadCompleteResponse),
        (status = 400, description = "The idempotency key or upload identifier is malformed, or \
          the stored bytes disagree with what was declared (WYRD_SPEC_400_VALIDATION, \
          WYRD_STORAGE_400_INVALID_UPLOAD_ID, WYRD_STORAGE_400_SHA256_MISMATCH, \
          WYRD_STORAGE_400_SIZE_MISMATCH)", body = WyrdProblem),
        (status = 401, description = "The request carried no usable access token \
          (WYRD_AUTH_401_UNAUTHENTICATED, WYRD_AUTH_401_INVALID_TOKEN, \
          WYRD_AUTH_401_TOKEN_EXPIRED, WYRD_AUTH_401_CREDENTIAL_REVOKED)", body = WyrdProblem),
        (status = 403, description = "The principal lacks card write, or the object belongs to \
          another tenant (WYRD_PERMISSION_403_DENIED_RBAC, WYRD_STORAGE_403_UPLOAD_FOREIGN_TENANT)", body = WyrdProblem),
        (status = 404, description = "No such upload, or its object never reached the backend \
          (WYRD_STORAGE_404_UPLOAD_NOT_FOUND, WYRD_STORAGE_404_OBJECT_NOT_FOUND)",
         body = WyrdProblem),
        (status = 409, description = "The upload is already terminal, the backend did not \
          advertise encryption, or the artifact row conflicts \
          (WYRD_STORAGE_409_UPLOAD_NOT_PENDING, WYRD_STORAGE_409_ENCRYPTION_MISSING, \
          WYRD_SPEC_409_CONFLICT)", body = WyrdProblem),
        (status = 500, description = "The storage backend or the platform store failed, or the \
          decision could not be audited (WYRD_STORAGE_500_BACKEND, WYRD_SPEC_500_INTERNAL, \
          WYRD_VALA_500_AUDIT_UNAVAILABLE)", body = WyrdProblem),
        (status = 503, description = "The revocation store could not vouch for the token, or \
          the storage backend is transiently unavailable \
          (WYRD_AUTH_503_VERIFY_UNAVAILABLE, \
          WYRD_STORAGE_503_BACKEND_UNAVAILABLE)", body = WyrdProblem)
    ),
    tag = "Storage"
)]
async fn complete(
    State(state): State<AppState>,
    caller: Caller,
    Path(id): Path<String>,
    headers: HeaderMap,
    Json(body): Json<UploadCompleteRequest>,
) -> Result<Json<wyrd_spec::storage::UploadCompleteResponse>, WyrdErrorResponse> {
    let idempotency_key = extract_idempotency_key(&headers)?;
    let upload_id = parse_upload_id(&id)?;
    authorize_card_write(
        &state,
        &caller,
        "storage.upload.complete",
        &format!("upload:{upload_id}"),
    )
    .await?;
    let storage_caller = storage_caller(&caller);
    service::upload_complete(
        &state.storage,
        state.postgres.wyrd(),
        &storage_caller,
        upload_id,
        idempotency_key,
        body,
    )
    .await
    .map(Json)
    .map_err(WyrdErrorResponse::from)
}

/// Abort an upload and release its pending storage state.
///
/// Aborts are safe to retry: the server treats an already completed or
/// already aborted upload as a no-op, while the idempotency key remains
/// available to the service for request correlation.
#[utoipa::path(
    post,
    path = "/v1/cards/upload/{id}/abort",
    params(
        ("id" = String, Path, description = "Upload to release"),
        ("Idempotency-Key" = Option<String>, Header, description = "Correlates a retried abort \
          with the original request")
    ),
    responses(
        (status = 200, description = "The upload is released; a repeat is a no-op",
         body = AbortResponse),
        (status = 400, description = "The idempotency key or upload identifier is malformed \
          (WYRD_SPEC_400_VALIDATION, WYRD_STORAGE_400_INVALID_UPLOAD_ID)", body = WyrdProblem),
        (status = 401, description = "The request carried no usable access token \
          (WYRD_AUTH_401_UNAUTHENTICATED, WYRD_AUTH_401_INVALID_TOKEN, \
          WYRD_AUTH_401_TOKEN_EXPIRED, WYRD_AUTH_401_CREDENTIAL_REVOKED)", body = WyrdProblem),
        (status = 403, description = "The principal lacks card write, or the object belongs to \
          another tenant (WYRD_PERMISSION_403_DENIED_RBAC, WYRD_STORAGE_403_UPLOAD_FOREIGN_TENANT)", body = WyrdProblem),
        (status = 404, description = "No such upload (WYRD_STORAGE_404_UPLOAD_NOT_FOUND)",
         body = WyrdProblem),
        (status = 409, description = "The upload is already terminal \
          (WYRD_STORAGE_409_UPLOAD_NOT_PENDING)", body = WyrdProblem),
        (status = 500, description = "The storage backend or the platform store failed, or the \
          decision could not be audited (WYRD_STORAGE_500_BACKEND, WYRD_SPEC_500_INTERNAL, \
          WYRD_VALA_500_AUDIT_UNAVAILABLE)", body = WyrdProblem),
        (status = 503, description = "The revocation store could not vouch for the token, or \
          the storage backend is transiently unavailable \
          (WYRD_AUTH_503_VERIFY_UNAVAILABLE, \
          WYRD_STORAGE_503_BACKEND_UNAVAILABLE)", body = WyrdProblem)
    ),
    tag = "Storage"
)]
async fn abort(
    State(state): State<AppState>,
    caller: Caller,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> Result<Json<AbortResponse>, WyrdErrorResponse> {
    let idempotency_key = extract_idempotency_key(&headers)?;
    let upload_id = parse_upload_id(&id)?;
    authorize_card_write(
        &state,
        &caller,
        "storage.upload.abort",
        &format!("upload:{upload_id}"),
    )
    .await?;
    let storage_caller = storage_caller(&caller);
    service::upload_abort(
        &state.storage,
        state.postgres.wyrd(),
        &storage_caller,
        upload_id,
        idempotency_key,
    )
    .await
    .map(Json)
    .map_err(WyrdErrorResponse::from)
}

/// Store bytes for the local development backend.
#[utoipa::path(
    put,
    path = "/v1/cards/upload/local/{id}",
    params(("id" = String, Path, description = "Upload the bytes belong to")),
    request_body(content = Vec<u8>, content_type = "application/octet-stream"),
    responses(
        (status = 200, description = "The bytes were stored", body = LocalBlobUploadResponse),
        (status = 400, description = "The upload identifier is not one this server minted \
          (WYRD_STORAGE_400_INVALID_UPLOAD_ID)", body = WyrdProblem),
        (status = 401, description = "The request carried no usable access token \
          (WYRD_AUTH_401_UNAUTHENTICATED, WYRD_AUTH_401_INVALID_TOKEN, \
          WYRD_AUTH_401_TOKEN_EXPIRED, WYRD_AUTH_401_CREDENTIAL_REVOKED)", body = WyrdProblem),
        (status = 403, description = "The principal lacks card write, or the object belongs to \
          another tenant (WYRD_PERMISSION_403_DENIED_RBAC, WYRD_STORAGE_403_UPLOAD_FOREIGN_TENANT)", body = WyrdProblem),
        (status = 404, description = "No such upload (WYRD_STORAGE_404_UPLOAD_NOT_FOUND)",
         body = WyrdProblem),
        (status = 409, description = "The upload is already terminal \
          (WYRD_STORAGE_409_UPLOAD_NOT_PENDING)", body = WyrdProblem),
        (status = 500, description = "The storage backend or the platform store failed, or the \
          decision could not be audited (WYRD_STORAGE_500_BACKEND, WYRD_SPEC_500_INTERNAL, \
          WYRD_VALA_500_AUDIT_UNAVAILABLE)", body = WyrdProblem),
        (status = 503, description = "The revocation store could not vouch for the token, or \
          the storage backend is transiently unavailable \
          (WYRD_AUTH_503_VERIFY_UNAVAILABLE, \
          WYRD_STORAGE_503_BACKEND_UNAVAILABLE)", body = WyrdProblem)
    ),
    tag = "Storage"
)]
async fn local_blob(
    State(state): State<AppState>,
    caller: Caller,
    Path(id): Path<String>,
    body: Bytes,
) -> Result<Json<LocalBlobUploadResponse>, WyrdErrorResponse> {
    let upload_id = parse_upload_id(&id)?;
    authorize_card_write(
        &state,
        &caller,
        "storage.upload.local_blob",
        &format!("upload:{upload_id}"),
    )
    .await?;
    let storage_caller = storage_caller(&caller);
    service::upload_local_blob(
        &state.storage,
        state.postgres.wyrd(),
        &storage_caller,
        upload_id,
        &body,
    )
    .await
    .map_err(WyrdErrorResponse::from)?;
    Ok(Json(LocalBlobUploadResponse { uploaded: true }))
}

/// Create a signed or local download plan for a stored artifact.
#[utoipa::path(
    post,
    path = "/v1/cards/download/init",
    request_body = DownloadInitRequest,
    responses(
        (status = 200, description = "Download plan",
         body = wyrd_spec::storage::DownloadInitResponse),
        (status = 400, description = "The stored URI or its tenant prefix is unreadable \
          (WYRD_STORAGE_400_INVALID_URI, WYRD_STORAGE_400_TENANT_PREFIX_INVALID)",
         body = WyrdProblem),
        (status = 401, description = "The request carried no usable access token \
          (WYRD_AUTH_401_UNAUTHENTICATED, WYRD_AUTH_401_INVALID_TOKEN, \
          WYRD_AUTH_401_TOKEN_EXPIRED, WYRD_AUTH_401_CREDENTIAL_REVOKED)", body = WyrdProblem),
        (status = 403, description = "The principal lacks card read, or the object belongs to \
          another tenant (WYRD_PERMISSION_403_DENIED_RBAC, WYRD_STORAGE_403_UPLOAD_FOREIGN_TENANT)", body = WyrdProblem),
        (status = 404, description = "No such card artifact, or its object is gone from the \
          backend (WYRD_SPEC_404_NOT_FOUND, WYRD_STORAGE_404_OBJECT_NOT_FOUND)",
         body = WyrdProblem),
        (status = 500, description = "The storage backend or the platform store failed, or the \
          decision could not be audited (WYRD_STORAGE_500_BACKEND, WYRD_SPEC_500_INTERNAL, \
          WYRD_VALA_500_AUDIT_UNAVAILABLE)", body = WyrdProblem),
        (status = 503, description = "The revocation store could not vouch for the token, or \
          the storage backend is transiently unavailable \
          (WYRD_AUTH_503_VERIFY_UNAVAILABLE, \
          WYRD_STORAGE_503_BACKEND_UNAVAILABLE, WYRD_STORAGE_503_PRESIGN_EXPIRED)", body = WyrdProblem)
    ),
    tag = "Storage"
)]
async fn download_init(
    State(state): State<AppState>,
    caller: Caller,
    Json(body): Json<DownloadInitRequest>,
) -> Result<Json<wyrd_spec::storage::DownloadInitResponse>, WyrdErrorResponse> {
    authorize_card_read(
        &state,
        &caller,
        "storage.download.init",
        &format!("card:{}", body.card_uid),
    )
    .await?;
    let storage_caller = storage_caller(&caller);
    service::download_init(&state.storage, state.postgres.wyrd(), &storage_caller, body)
        .await
        .map(Json)
        .map_err(WyrdErrorResponse::from)
}

/// Stream bytes from the local development backend.
#[utoipa::path(
    get,
    path = "/v1/cards/download/local/{path}",
    params(("path" = String, Path, description = "Stored object path to stream")),
    responses(
        (status = 200, description = "The stored bytes",
         content_type = "application/octet-stream"),
        (status = 401, description = "The request carried no usable access token \
          (WYRD_AUTH_401_UNAUTHENTICATED, WYRD_AUTH_401_INVALID_TOKEN, \
          WYRD_AUTH_401_TOKEN_EXPIRED, WYRD_AUTH_401_CREDENTIAL_REVOKED)", body = WyrdProblem),
        (status = 403, description = "The principal lacks card read, or the object belongs to \
          another tenant (WYRD_PERMISSION_403_DENIED_RBAC, WYRD_STORAGE_403_UPLOAD_FOREIGN_TENANT)", body = WyrdProblem),
        (status = 404, description = "No such stored object \
          (WYRD_STORAGE_404_OBJECT_NOT_FOUND)", body = WyrdProblem),
        (status = 500, description = "The storage backend or the platform store failed, or the \
          decision could not be audited (WYRD_STORAGE_500_BACKEND, WYRD_SPEC_500_INTERNAL, \
          WYRD_VALA_500_AUDIT_UNAVAILABLE)", body = WyrdProblem),
        (status = 503, description = "The revocation store could not vouch for the token, or \
          the storage backend is transiently unavailable \
          (WYRD_AUTH_503_VERIFY_UNAVAILABLE, \
          WYRD_STORAGE_503_BACKEND_UNAVAILABLE)", body = WyrdProblem)
    ),
    tag = "Storage"
)]
async fn download_local_blob(
    State(state): State<AppState>,
    caller: Caller,
    Path(path): Path<String>,
) -> Result<Response, WyrdErrorResponse> {
    authorize_card_read(
        &state,
        &caller,
        "storage.download.local_blob",
        &format!("object:{path}"),
    )
    .await?;
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

/// Parse the route upload identifier and map malformed values to the HTTP error.
fn parse_upload_id(value: &str) -> Result<UploadId, WyrdErrorResponse> {
    UploadId::from_str(value)
        .map_err(|error| service::invalid_upload_id(&error))
        .map_err(WyrdErrorResponse::from)
}

/// Parse the optional storage idempotency header.
///
/// Storage initialization permits the header to be omitted, while a supplied
/// value must be valid UTF-8 and satisfy the shared `IdempotencyKey` contract.
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

/// Evaluate and audit the card-write permission for one storage mutation.
///
/// Storage routes own no transaction of their own — the service opens its own —
/// so the verdict is recorded in its own tenant transaction before the mutation
/// is dispatched, and an append failure refuses the request.
async fn authorize_card_write(
    state: &AppState,
    caller: &Caller,
    operation: &str,
    resource: &str,
) -> Result<(), WyrdErrorResponse> {
    audit::authorize(
        state,
        caller,
        &Permission::card_write(),
        operation,
        resource,
    )
    .await
    .map_err(WyrdErrorResponse::from)
}

/// Evaluate and audit the card-read permission for one storage read.
///
/// A permitted download is the moment a principal is allowed at the artifact
/// bytes, so its decision row is written before any plan or stream is produced.
async fn authorize_card_read(
    state: &AppState,
    caller: &Caller,
    operation: &str,
    resource: &str,
) -> Result<(), WyrdErrorResponse> {
    audit::authorize(state, caller, &Permission::card_read(), operation, resource)
        .await
        .map_err(WyrdErrorResponse::from)
}

/// Convert the authenticated caller into the storage service's tenant context.
///
/// Storage orchestration only needs the tenant its rows are isolated by; the
/// route has already evaluated and audited the principal's permission, so no
/// principal identity crosses into the storage crate.
pub(crate) fn storage_caller(caller: &Caller) -> StorageCaller {
    StorageCaller {
        data_tenant_id: caller.data_tenant_id,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn idempotency_key_rejects_non_utf8_header() {
        let mut headers = HeaderMap::new();
        headers.insert(
            axum::http::HeaderName::from_static("idempotency-key"),
            axum::http::HeaderValue::from_bytes(&[0xFF, 0xFE]).expect("raw bytes header"),
        );

        let err = extract_idempotency_key(&headers).expect_err("non-UTF-8 key must fail");

        assert_eq!(err.0.status(), 400);
    }

    #[test]
    fn idempotency_key_rejects_invalid_key_format() {
        let mut headers = HeaderMap::new();
        headers.insert(
            axum::http::HeaderName::from_static("idempotency-key"),
            axum::http::HeaderValue::from_static("bad"),
        );

        let err = extract_idempotency_key(&headers).expect_err("invalid key must fail");

        assert_eq!(err.0.status(), 400);
    }
}
