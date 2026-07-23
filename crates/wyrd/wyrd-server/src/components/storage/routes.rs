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
use wyrd_runtime::{Permission, PermissionCheck, PrincipalKind};
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
            .route("/cards/upload/local/{*id}", put(local_blob))
            .route("/cards/download/local/{*path}", get(download_local_blob))
    } else {
        router
    }
}

/// Create or replay a storage upload initialization.
async fn init(
    State(state): State<AppState>,
    caller: Caller,
    headers: HeaderMap,
    Json(body): Json<UploadInitRequest>,
) -> Result<Json<wyrd_spec::storage::UploadInitResponse>, WyrdErrorResponse> {
    authorize_card_write(state.authz.permission_check.as_ref(), &caller)?;
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
async fn part_url(
    State(state): State<AppState>,
    caller: Caller,
    Path(id): Path<String>,
    Query(query): Query<PartUrlQuery>,
) -> Result<Json<PartUrlResponse>, WyrdErrorResponse> {
    authorize_card_write(state.authz.permission_check.as_ref(), &caller)?;
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

/// Complete an initialized upload after the client has transferred its bytes.
///
/// The optional `Idempotency-Key` is forwarded to the storage service so a
/// retried client completion keeps the same request context. The service
/// verifies the backend object before persisting its artifact metadata.
async fn complete(
    State(state): State<AppState>,
    caller: Caller,
    Path(id): Path<String>,
    headers: HeaderMap,
    Json(body): Json<UploadCompleteRequest>,
) -> Result<Json<wyrd_spec::storage::UploadCompleteResponse>, WyrdErrorResponse> {
    authorize_card_write(state.authz.permission_check.as_ref(), &caller)?;
    let idempotency_key = extract_idempotency_key(&headers)?;
    let upload_id = parse_upload_id(&id)?;
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
async fn abort(
    State(state): State<AppState>,
    caller: Caller,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> Result<Json<AbortResponse>, WyrdErrorResponse> {
    authorize_card_write(state.authz.permission_check.as_ref(), &caller)?;
    let idempotency_key = extract_idempotency_key(&headers)?;
    let upload_id = parse_upload_id(&id)?;
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
async fn local_blob(
    State(state): State<AppState>,
    caller: Caller,
    Path(id): Path<String>,
    body: Bytes,
) -> Result<Json<LocalBlobUploadResponse>, WyrdErrorResponse> {
    authorize_card_write(state.authz.permission_check.as_ref(), &caller)?;
    let upload_id = parse_upload_id(&id)?;
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
async fn download_init(
    State(state): State<AppState>,
    caller: Caller,
    Json(body): Json<DownloadInitRequest>,
) -> Result<Json<wyrd_spec::storage::DownloadInitResponse>, WyrdErrorResponse> {
    authorize_card_read(state.authz.permission_check.as_ref(), &caller)?;
    let storage_caller = storage_caller(&caller);
    service::download_init(&state.storage, state.postgres.wyrd(), &storage_caller, body)
        .await
        .map(Json)
        .map_err(WyrdErrorResponse::from)
}

/// Stream bytes from the local development backend.
async fn download_local_blob(
    State(state): State<AppState>,
    caller: Caller,
    Path(path): Path<String>,
) -> Result<Response, WyrdErrorResponse> {
    authorize_card_read(state.authz.permission_check.as_ref(), &caller)?;
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

/// Require the card-write permission for storage mutations.
fn authorize_card_write(
    check: &dyn PermissionCheck,
    caller: &Caller,
) -> Result<(), WyrdErrorResponse> {
    authorize(check, caller, Permission::card_write())
}

/// Require the card-read permission for storage reads.
fn authorize_card_read(
    check: &dyn PermissionCheck,
    caller: &Caller,
) -> Result<(), WyrdErrorResponse> {
    authorize(check, caller, Permission::card_read())
}

/// Evaluate one route capability against the already authenticated principal.
fn authorize(
    check: &dyn PermissionCheck,
    caller: &Caller,
    required: Permission,
) -> Result<(), WyrdErrorResponse> {
    check
        .check(&caller.principal, &required)
        .into_result()
        .map_err(WyrdErrorResponse::from)
}

/// Convert the authenticated caller into the storage service's typed subject.
///
/// The principal UUID is passed directly. Storage does not infer identity from
/// a string prefix, and the caller's tenant and request ID are preserved for
/// row-level isolation and audit attribution.
pub(crate) fn storage_caller(caller: &Caller) -> StorageCaller {
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

#[cfg(test)]
mod tests {
    use super::*;
    use wyrd_runtime::{PermissionSet, Principal, PrincipalId};
    use wyrd_semver::VersionBlock;
    use wyrd_spec::DataTenantId;
    use wyrd_spec::envelope::CardKind;
    use wyrd_spec::ids::{CardName, SpaceName};
    use wyrd_spec::reference::{CardRef, CardRefScope};
    use wyrd_spec::request_id::RequestId;

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

    #[test]
    fn local_blob_helpers_require_route_permissions() {
        let caller = caller_with_permissions(PrincipalKind::User, []);

        let check = wyrd_runtime::RbacCheck;
        let read =
            authorize_card_read(&check, &caller).expect_err("missing read permission must fail");
        let write =
            authorize_card_write(&check, &caller).expect_err("missing write permission must fail");

        assert_eq!(read.0.status(), 403);
        assert_eq!(write.0.status(), 403);
    }

    #[test]
    fn storage_caller_maps_runtime_identity_without_string_prefixing() {
        let user = caller_with_permissions(PrincipalKind::User, [Permission::card_read()]);
        let service_ref = card_ref(CardKind::Service, "artifact-writer");
        let service = caller_with_permissions(
            PrincipalKind::Service {
                card_ref: service_ref.clone(),
                card_ref_scope: CardRefScope::own(&service_ref),
            },
            [Permission::card_write()],
        );
        let agent_ref = card_ref(CardKind::Agent, "artifact-agent");
        let agent = caller_with_permissions(
            PrincipalKind::Agent {
                card_ref: agent_ref.clone(),
                card_ref_scope: CardRefScope::own(&agent_ref),
            },
            [Permission::card_write()],
        );

        let user_storage = storage_caller(&user);
        let service_storage = storage_caller(&service);
        let agent_storage = storage_caller(&agent);

        assert_eq!(user_storage.subject.kind, StoragePrincipalKind::User);
        assert_eq!(service_storage.subject.kind, StoragePrincipalKind::Service);
        assert_eq!(agent_storage.subject.kind, StoragePrincipalKind::Agent);
        assert_eq!(
            user_storage.subject.principal_id,
            user.principal.id.as_uuid()
        );
        assert_eq!(user_storage.request_id, user.request_id);
    }

    fn caller_with_permissions(
        kind: PrincipalKind,
        permissions: impl IntoIterator<Item = Permission>,
    ) -> Caller {
        let tenant = DataTenantId::new_v7();
        Caller {
            data_tenant_id: tenant,
            principal: Principal::new(
                PrincipalId::new(uuid::Uuid::now_v7()),
                kind,
                tenant,
                vec![],
                PermissionSet::from_iter(permissions),
            ),
            request_id: RequestId::parse(&uuid::Uuid::now_v7().to_string())
                .expect("request id parses"),
        }
    }

    fn card_ref(kind: CardKind, name: &str) -> CardRef {
        CardRef {
            kind,
            name: CardName::new(name).expect("name"),
            version: VersionBlock::parse("1.0.0").expect("version"),
            space: Some(SpaceName::new("prod").expect("space")),
            uid: None,
        }
    }
}
