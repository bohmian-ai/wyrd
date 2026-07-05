//! Pure storage upload service functions.

use std::path::Path;
use std::time::Duration;

use axum::body::{Body, Bytes};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use base64::Engine;
use sha2::{Digest, Sha256};
use sqlx::types::Uuid;
use tokio_util::io::ReaderStream;
use tracing::instrument;
use wyrd_runtime::Permission;
use wyrd_spec::error::WyrdError;
use wyrd_spec::error::storage::WyrdStorageError;
use wyrd_spec::ids::IdempotencyKey;
use wyrd_spec::storage::{
    AbortResponse, AzureBlockBlobComplete, DownloadInitRequest, DownloadInitResponse, DownloadPlan,
    IDEMPOTENCY_KEY_HEADER, PartUrlResponse, S3MultipartComplete, StorageBackendKind,
    StoredObjectRef, UploadCompleteRequest, UploadCompleteResponse, UploadId, UploadIdParseError,
    UploadInitRequest, UploadInitResponse, UploadPlan, WireProtocol,
};
use wyrd_sql::TenantConn;
use wyrd_sql::queries::storage::artifact_metadata::ArtifactMetadataRow;
use wyrd_sql::queries::storage::multipart_uploads::{self, MultipartUploadRow, UploadStatus};
use wyrd_storage::error::StorageError;
use wyrd_storage::plan::{MAX_OBJECT_SIZE_BYTES, PlannedUpload, plan_upload};
use wyrd_storage::signer::{CompletePayload, HeadInfo, MultipartInit, UploadPlanReplayInput};
use wyrd_storage::tenant_path::{self, TenantPathError, ValidatedPath};

use crate::AppState;
use crate::components::auth::Caller;
use crate::storage::audit::{self, UploadAuditOperation};

const INIT_TTL_SECS: u64 = 24 * 60 * 60;

struct InitReplay {
    upload_id: UploadId,
    backend: StorageBackendKind,
    storage_path: String,
    validated: ValidatedPath,
    replay_input: UploadPlanReplayInput,
}

struct PriorAbort {
    storage_path: String,
    backend_upload_id: Option<String>,
}

struct FailureContext<'a> {
    operation: UploadAuditOperation,
    reason: &'a str,
    status_code: i32,
    error_code: Option<&'a str>,
}

/// Initialize an artifact upload.
#[instrument(skip(state, caller, headers, body), fields(tenant = %caller.data_tenant_id))]
pub async fn upload_init(
    state: &AppState,
    caller: Caller,
    headers: &HeaderMap,
    body: UploadInitRequest,
) -> Result<UploadInitResponse, WyrdError> {
    authorize_card_write(&caller)?;

    let idempotency_key = extract_idempotency_key(headers)?;
    let body_sha = sha256_canonical_json(&body);
    let backend = state.storage.backend();
    let validated = validated_tenant_path(&caller, &body)?;
    let planned = plan_upload(
        body.expected_size_bytes,
        backend,
        state.storage.multipart_threshold_bytes(),
    )
    .map_err(|_| {
        map_storage_error(StorageError::ArtifactTooLarge {
            actual: body.expected_size_bytes,
            limit: MAX_OBJECT_SIZE_BYTES,
        })
    })?;
    let wire_protocol = derive_wire_protocol(backend, planned);

    let mut conn = state
        .postgres
        .tenant_conn(caller.data_tenant_id)
        .await
        .map_err(map_sql_error)?;
    if let Some(key) = idempotency_key.as_ref()
        && let Some(response) =
            try_replay_idempotent_init(&caller, &mut conn, key, &body_sha).await?
    {
        conn.commit().await.map_err(map_sql_error)?;
        let plan = state
            .storage
            .signer()
            .remint_plan(
                &response.validated,
                &response.replay_input,
                state.storage.presign_ttl(),
            )
            .await
            .map_err(map_storage_error)?;
        return Ok(UploadInitResponse {
            upload_id: response.upload_id,
            backend: response.backend,
            plan,
            storage_path: response.storage_path,
        });
    }

    let prior_abort = find_and_mark_prior_pending(&mut conn, &body).await?;

    let upload_id = UploadId::new();
    let upload_uuid = upload_id_uuid(&upload_id)?;
    let counts = upload_row_counts(planned, body.expected_size_bytes, wire_protocol);
    multipart_uploads::insert_initiating(
        &mut conn,
        multipart_uploads::NewMultipartUpload {
            id: upload_uuid,
            card_uid: body.card_uid.as_str(),
            relative_path: &body.relative_path,
            storage_path: &validated.full,
            backend,
            wire_protocol,
            expected_sha256: &body.expected_sha256,
            expected_size_bytes: i64::try_from(body.expected_size_bytes).map_err(|_| {
                validation_error(
                    "expected_size_bytes exceeds signed storage metadata range",
                    serde_json::json!({ "expected_size_bytes": body.expected_size_bytes }),
                )
            })?,
            content_type: body.content_type.as_deref(),
            part_count_planned: i32::try_from(counts.part_count).map_err(|_| {
                internal_error(
                    "planned part count exceeds storage metadata range",
                    serde_json::json!({ "part_count": counts.part_count }),
                )
            })?,
            part_size_bytes: i64::try_from(counts.part_size_bytes).map_err(|_| {
                internal_error(
                    "planned part size exceeds storage metadata range",
                    serde_json::json!({ "part_size_bytes": counts.part_size_bytes }),
                )
            })?,
            block_count_planned: counts.block_count_planned,
            ttl_secs: INIT_TTL_SECS as i64,
        },
    )
    .await
    .map_err(map_sql_error)?;
    conn.commit().await.map_err(map_sql_error)?;

    if let Some(prior) = prior_abort {
        abort_prior_best_effort(state, &caller, prior).await;
    }

    let init = match drive_backend_init(state, &validated, planned, body.expected_size_bytes).await
    {
        Ok(init) => init,
        Err(error) => {
            let error_code = error.code().to_owned();
            let status_code = i32::from(error.status());
            mark_failed_best_effort(
                state,
                &caller,
                upload_uuid,
                &validated,
                backend,
                FailureContext {
                    operation: UploadAuditOperation::UploadInit,
                    reason: "backend init failed",
                    status_code,
                    error_code: Some(&error_code),
                },
            )
            .await;
            return Err(error);
        }
    };

    let mut conn = state
        .postgres
        .tenant_conn(caller.data_tenant_id)
        .await
        .map_err(map_sql_error)?;
    persist_s3_upload_id_and_audit(
        &mut conn,
        &caller,
        upload_uuid,
        &validated,
        backend,
        &init.backend_upload_id,
    )
    .await?;
    if let Some(key) = idempotency_key.as_ref() {
        cache_init_seed(
            &mut conn,
            key,
            &body_sha,
            &upload_id,
            &validated,
            backend,
            wire_protocol,
        )
        .await?;
    }
    conn.commit().await.map_err(map_sql_error)?;

    Ok(UploadInitResponse {
        upload_id,
        backend,
        plan: init.plan,
        storage_path: validated.full,
    })
}

/// Return one S3 multipart part URL.
#[instrument(skip(state, caller), fields(tenant = %caller.data_tenant_id, upload_id = %upload_id))]
pub async fn upload_part_url(
    state: &AppState,
    caller: Caller,
    upload_id: UploadId,
    part_number: u32,
) -> Result<PartUrlResponse, WyrdError> {
    authorize_card_write(&caller)?;
    let upload_uuid = upload_id_uuid(&upload_id)?;

    let mut conn = state
        .postgres
        .tenant_conn(caller.data_tenant_id)
        .await
        .map_err(map_sql_error)?;
    let row = load_upload(&mut conn, upload_uuid).await?;
    if row.status != UploadStatus::Pending {
        return Err(conflict_error(
            "upload is not pending",
            serde_json::json!({ "upload_id": upload_id.to_string(), "status": row.status }),
        ));
    }
    if row.wire_protocol != WireProtocol::S3MultipartV1 {
        return Err(validation_error(
            "part-url is only valid for S3 multipart uploads",
            serde_json::json!({
                "upload_id": upload_id.to_string(),
                "wire_protocol": row.wire_protocol,
            }),
        ));
    }
    let max_part = u32::try_from(row.part_count_planned).unwrap_or(u32::MAX);
    if part_number == 0 || part_number > max_part {
        return Err(validation_error(
            "part_number is outside the planned part range",
            serde_json::json!({ "part_number": part_number, "max_part": max_part }),
        ));
    }
    let validated =
        tenant_path::validate(&row.storage_path, caller.data_tenant_id).map_err(map_tenant_path)?;
    let backend_upload_id = row.backend_upload_id.ok_or_else(|| {
        internal_error(
            "pending S3 upload is missing backend_upload_id",
            serde_json::json!({ "upload_id": upload_id.to_string() }),
        )
    })?;
    conn.commit().await.map_err(map_sql_error)?;

    let url = state
        .storage
        .signer()
        .presign_part(
            &validated,
            &backend_upload_id,
            part_number,
            state.storage.presign_ttl(),
        )
        .await
        .map_err(map_storage_error)?;

    Ok(PartUrlResponse {
        url,
        ttl_secs: state.storage.presign_ttl_secs(),
    })
}

/// Complete an upload and return the verified stored object descriptor.
#[instrument(skip(state, caller, body), fields(tenant = %caller.data_tenant_id, upload_id = %upload_id))]
pub async fn upload_complete(
    state: &AppState,
    caller: Caller,
    upload_id: UploadId,
    body: UploadCompleteRequest,
) -> Result<UploadCompleteResponse, WyrdError> {
    authorize_card_write(&caller)?;
    let upload_uuid = upload_id_uuid(&upload_id)?;

    let mut conn = state
        .postgres
        .tenant_conn(caller.data_tenant_id)
        .await
        .map_err(map_sql_error)?;
    let row = load_pending_upload(&mut conn, upload_uuid).await?;
    let validated =
        tenant_path::validate(&row.storage_path, caller.data_tenant_id).map_err(map_tenant_path)?;
    let payload = build_complete_payload(
        &body,
        row.wire_protocol,
        row.backend,
        row.block_count_planned,
        &row.expected_sha256,
    )?;
    conn.commit().await.map_err(map_sql_error)?;

    if let Some(payload) = payload
        && let Err(error) = state
            .storage
            .signer()
            .complete_server_side(&validated, row.backend_upload_id.as_deref(), payload)
            .await
    {
        let error = map_storage_error(error);
        let error_code = error.code().to_owned();
        let status_code = i32::from(error.status());
        mark_failed_best_effort(
            state,
            &caller,
            upload_uuid,
            &validated,
            row.backend,
            FailureContext {
                operation: UploadAuditOperation::UploadComplete,
                reason: "backend_complete_failed",
                status_code,
                error_code: Some(&error_code),
            },
        )
        .await;
        return Err(error);
    }

    let head = match state
        .storage
        .signer()
        .head_for_verification(&validated)
        .await
    {
        Ok(head) => head,
        Err(error) => {
            let error = map_storage_error(error);
            let error_code = error.code().to_owned();
            let status_code = i32::from(error.status());
            mark_failed_best_effort(
                state,
                &caller,
                upload_uuid,
                &validated,
                row.backend,
                FailureContext {
                    operation: UploadAuditOperation::UploadComplete,
                    reason: "head_for_verification_failed",
                    status_code,
                    error_code: Some(&error_code),
                },
            )
            .await;
            return Err(error);
        }
    };
    verify_object_head(state, &caller, upload_uuid, &validated, &row, &head).await?;

    let mut conn = state
        .postgres
        .tenant_conn(caller.data_tenant_id)
        .await
        .map_err(map_sql_error)?;
    wyrd_sql::queries::storage::artifact_metadata::insert(
        &mut conn,
        wyrd_sql::queries::storage::artifact_metadata::NewArtifactMetadata {
            storage_path: &validated.full,
            card_uid: &validated.card_uid,
            size_bytes: i64::try_from(head.size_bytes).map_err(|_| {
                internal_error(
                    "verified object size exceeds storage metadata range",
                    serde_json::json!({ "size_bytes": head.size_bytes }),
                )
            })?,
            sha256: &row.expected_sha256,
            content_type: head.content_type.as_deref().or(row.content_type.as_deref()),
            sse_marker: head.sse_marker.as_deref(),
            backend: row.backend,
        },
    )
    .await
    .map_err(map_sql_error)?;
    multipart_uploads::mark_completed(&mut conn, upload_uuid)
        .await
        .map_err(map_sql_error)?;
    audit::write(
        &mut conn,
        &caller,
        UploadAuditOperation::UploadComplete,
        Some(upload_uuid),
        &validated.full,
        row.backend,
        200,
        None,
    )
    .await?;
    conn.commit().await.map_err(map_sql_error)?;

    Ok(UploadCompleteResponse {
        stored: StoredObjectRef {
            storage_path: validated.full,
            size_bytes: head.size_bytes,
            sha256: row.expected_sha256,
            content_type: head.content_type.or(row.content_type),
            sse_marker: head.sse_marker,
            created_at: chrono::Utc::now(),
        },
    })
}

/// Abort an in-flight upload.
#[instrument(skip(state, caller), fields(tenant = %caller.data_tenant_id, upload_id = %upload_id))]
pub async fn upload_abort(
    state: &AppState,
    caller: Caller,
    upload_id: UploadId,
) -> Result<AbortResponse, WyrdError> {
    authorize_card_write(&caller)?;
    let upload_uuid = upload_id_uuid(&upload_id)?;

    let mut conn = state
        .postgres
        .tenant_conn(caller.data_tenant_id)
        .await
        .map_err(map_sql_error)?;
    let row = load_upload(&mut conn, upload_uuid).await?;
    if matches!(
        row.status,
        UploadStatus::Aborted | UploadStatus::Completed | UploadStatus::Failed
    ) {
        conn.commit().await.map_err(map_sql_error)?;
        return Ok(AbortResponse { aborted: false });
    }
    let validated =
        tenant_path::validate(&row.storage_path, caller.data_tenant_id).map_err(map_tenant_path)?;
    conn.commit().await.map_err(map_sql_error)?;

    if let Some(backend_upload_id) = row.backend_upload_id.as_deref()
        && let Err(error) = state
            .storage
            .signer()
            .abort_multipart(&validated, backend_upload_id)
            .await
    {
        tracing::warn!(error = %error, upload_id = %upload_id, "best-effort backend abort failed");
    }

    let mut conn = state
        .postgres
        .tenant_conn(caller.data_tenant_id)
        .await
        .map_err(map_sql_error)?;
    let aborted =
        match multipart_uploads::mark_aborted(&mut conn, upload_uuid, Some("client-abort")).await {
            Ok(()) => true,
            Err(wyrd_sql::SqlError::Conflict { .. }) => false,
            Err(error) => return Err(map_sql_error(error)),
        };
    audit::write(
        &mut conn,
        &caller,
        UploadAuditOperation::UploadAbort,
        Some(upload_uuid),
        &validated.full,
        row.backend,
        200,
        None,
    )
    .await?;
    conn.commit().await.map_err(map_sql_error)?;

    Ok(AbortResponse { aborted })
}

/// Write a local-mode raw blob body.
#[instrument(skip(state, caller, body), fields(tenant = %caller.data_tenant_id))]
pub async fn upload_local_blob(
    state: &AppState,
    caller: Caller,
    path: String,
    body: Bytes,
) -> Result<(), WyrdError> {
    authorize_card_write(&caller)?;
    let validated = tenant_path::validate(&path, caller.data_tenant_id).map_err(map_tenant_path)?;
    let wyrd_storage::BackendSigner::Local(local) = state.storage.signer() else {
        return Err(internal_error(
            "local blob route mounted for non-local backend",
            serde_json::json!({ "backend": state.storage.backend() }),
        ));
    };
    local
        .write_atomically(Path::new(&validated.full), &body)
        .await
        .map_err(map_storage_error)
}

/// Initialize an artifact download.
#[instrument(skip(state, caller, body), fields(tenant = %caller.data_tenant_id))]
pub async fn download_init(
    state: &AppState,
    caller: Caller,
    body: DownloadInitRequest,
) -> Result<DownloadInitResponse, WyrdError> {
    authorize_card_read(&caller)?;
    let validated = validated_download_path(&caller, &body)?;

    let mut conn = state
        .postgres
        .tenant_conn(caller.data_tenant_id)
        .await
        .map_err(map_sql_error)?;
    let metadata = load_artifact_metadata(&mut conn, &validated).await?;
    conn.commit().await.map_err(map_sql_error)?;

    let request_ttl_secs = compute_download_ttl(&body, state.storage.presign_ttl_secs());
    let get_url = download_url(state, &validated, request_ttl_secs).await?;
    let ttl_secs = if state.storage.backend() == StorageBackendKind::Local {
        0
    } else {
        request_ttl_secs
    };

    let mut conn = state
        .postgres
        .tenant_conn(caller.data_tenant_id)
        .await
        .map_err(map_sql_error)?;
    audit::write(
        &mut conn,
        &caller,
        UploadAuditOperation::DownloadInit,
        None,
        &validated.full,
        metadata.backend,
        200,
        None,
    )
    .await?;
    conn.commit().await.map_err(map_sql_error)?;

    Ok(DownloadInitResponse {
        plan: DownloadPlan { get_url, ttl_secs },
        size_bytes: u64::try_from(metadata.size_bytes).map_err(|_| {
            internal_error(
                "stored artifact size is negative",
                serde_json::json!({
                    "storage_path": validated.full,
                    "size_bytes": metadata.size_bytes,
                }),
            )
        })?,
        sha256: metadata.sha256,
    })
}

/// Stream a local-mode blob through the authenticated server route.
#[instrument(skip(state, caller), fields(tenant = %caller.data_tenant_id))]
pub async fn download_local_blob(
    state: &AppState,
    caller: Caller,
    path: String,
) -> Result<Response, WyrdError> {
    authorize_card_read(&caller)?;
    let validated = tenant_path::validate(&path, caller.data_tenant_id).map_err(map_tenant_path)?;
    let wyrd_storage::BackendSigner::Local(local) = state.storage.signer() else {
        return Err(internal_error(
            "local download route mounted for non-local backend",
            serde_json::json!({ "backend": state.storage.backend() }),
        ));
    };
    let (file, len) = open_local_blob(local, &validated).await?;
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

async fn try_replay_idempotent_init(
    caller: &Caller,
    conn: &mut TenantConn<'_>,
    key: &IdempotencyKey,
    body_sha: &[u8; 32],
) -> Result<Option<InitReplay>, WyrdError> {
    let Some(cached) = wyrd_sql::queries::storage::idempotency::get(conn, key.as_str(), body_sha)
        .await
        .map_err(map_sql_error)?
    else {
        return Ok(None);
    };

    let upload_id: UploadId = cached.seed.upload_id.parse().map_err(invalid_upload_id)?;
    let row = load_upload(conn, upload_id_uuid(&upload_id)?).await?;
    let validated = tenant_path::validate(&cached.seed.storage_path, caller.data_tenant_id)
        .map_err(map_tenant_path)?;
    let replay = UploadPlanReplayInput {
        wire_protocol: row.wire_protocol,
        backend_upload_id: row.backend_upload_id,
        part_count_planned: u32::try_from(row.part_count_planned).unwrap_or(u32::MAX),
        part_size_bytes: u64::try_from(row.part_size_bytes).unwrap_or(0),
        block_count_planned: row
            .block_count_planned
            .map(|value| u32::try_from(value).unwrap_or(u32::MAX)),
    };

    Ok(Some(InitReplay {
        upload_id,
        backend: cached.seed.backend,
        storage_path: cached.seed.storage_path,
        validated,
        replay_input: replay,
    }))
}

async fn find_and_mark_prior_pending(
    conn: &mut TenantConn<'_>,
    body: &UploadInitRequest,
) -> Result<Option<PriorAbort>, WyrdError> {
    let prior = multipart_uploads::find_pending_for_dedupe(
        conn,
        body.card_uid.as_str(),
        &body.expected_sha256,
    )
    .await
    .map_err(map_sql_error)?;

    if let Some(prior) = prior {
        let abort = PriorAbort {
            storage_path: prior.storage_path,
            backend_upload_id: prior.backend_upload_id,
        };
        multipart_uploads::mark_aborted(conn, prior.id, Some("re-init"))
            .await
            .map_err(map_sql_error)?;
        return Ok(Some(abort));
    }

    Ok(None)
}

async fn abort_prior_best_effort(state: &AppState, caller: &Caller, prior: PriorAbort) {
    let Some(backend_upload_id) = prior.backend_upload_id else {
        return;
    };
    let Ok(validated) = tenant_path::validate(&prior.storage_path, caller.data_tenant_id) else {
        tracing::warn!(storage_path = %prior.storage_path, "re-init prior upload path failed validation");
        return;
    };
    if let Err(error) = state
        .storage
        .signer()
        .abort_multipart(&validated, &backend_upload_id)
        .await
    {
        tracing::warn!(error = %error, "best-effort re-init backend abort failed");
    }
}

fn extract_idempotency_key(headers: &HeaderMap) -> Result<Option<IdempotencyKey>, WyrdError> {
    headers
        .get(IDEMPOTENCY_KEY_HEADER)
        .map(|value| {
            value
                .to_str()
                .map_err(|_| {
                    validation_error(
                        "idempotency key header is not valid UTF-8",
                        serde_json::json!({ "header": IDEMPOTENCY_KEY_HEADER }),
                    )
                })
                .and_then(|value| {
                    IdempotencyKey::new(value).map_err(|error| {
                        validation_error(
                            "idempotency key is invalid",
                            serde_json::json!({
                                "header": IDEMPOTENCY_KEY_HEADER,
                                "source": error.to_string(),
                            }),
                        )
                    })
                })
        })
        .transpose()
}

fn authorize_card_write(caller: &Caller) -> Result<(), WyrdError> {
    let required = Permission::card_write();
    if caller.principal.effective_permissions.contains(&required) {
        return Ok(());
    }
    Err(WyrdError::PermissionDeniedRbac {
        message: "caller lacks required permission card:write".to_owned(),
        details: serde_json::json!({ "required": required }),
    })
}

fn authorize_card_read(caller: &Caller) -> Result<(), WyrdError> {
    let required = Permission::card_read();
    if caller.principal.effective_permissions.contains(&required) {
        return Ok(());
    }
    Err(WyrdError::PermissionDeniedRbac {
        message: "caller lacks required permission card:read".to_owned(),
        details: serde_json::json!({ "required": required }),
    })
}

fn validated_tenant_path(
    caller: &Caller,
    body: &UploadInitRequest,
) -> Result<ValidatedPath, WyrdError> {
    validate_sha256_b64(&body.expected_sha256)?;
    let storage_path = tenant_path::build(
        caller.data_tenant_id,
        body.card_uid.as_str(),
        &body.relative_path,
    );
    tenant_path::validate(&storage_path, caller.data_tenant_id).map_err(map_tenant_path)
}

fn validated_download_path(
    caller: &Caller,
    body: &DownloadInitRequest,
) -> Result<ValidatedPath, WyrdError> {
    let storage_path = tenant_path::build(
        caller.data_tenant_id,
        body.card_uid.as_str(),
        &body.relative_path,
    );
    tenant_path::validate(&storage_path, caller.data_tenant_id).map_err(map_tenant_path)
}

async fn load_artifact_metadata(
    conn: &mut TenantConn<'_>,
    validated: &ValidatedPath,
) -> Result<ArtifactMetadataRow, WyrdError> {
    wyrd_sql::queries::storage::artifact_metadata::get(conn, &validated.full)
        .await
        .map_err(map_sql_error)?
        .ok_or_else(|| {
            map_storage_error(StorageError::ObjectNotFound {
                storage_path: validated.full.clone(),
            })
        })
}

fn compute_download_ttl(body: &DownloadInitRequest, default_ttl_secs: u32) -> u32 {
    body.ttl_secs.unwrap_or(default_ttl_secs).clamp(60, 3600)
}

async fn download_url(
    state: &AppState,
    validated: &ValidatedPath,
    ttl_secs: u32,
) -> Result<String, WyrdError> {
    if state.storage.backend() == StorageBackendKind::Local {
        return local_download_url(state, validated);
    }

    state
        .storage
        .signer()
        .presign_get(validated, Duration::from_secs(u64::from(ttl_secs)))
        .await
        .map_err(map_storage_error)
}

fn local_download_url(state: &AppState, validated: &ValidatedPath) -> Result<String, WyrdError> {
    let base = state.storage.public_base_url().ok_or_else(|| {
        internal_error(
            "local storage requires WYRD_PUBLIC_BASE_URL to mint download URLs",
            serde_json::json!({ "backend": StorageBackendKind::Local }),
        )
    })?;
    let base = normalize_base_url(base);
    Ok(format!("{base}/v1/cards/download/local/{}", validated.full))
}

async fn open_local_blob(
    local: &wyrd_storage::LocalSigner,
    validated: &ValidatedPath,
) -> Result<(tokio::fs::File, u64), WyrdError> {
    let fs_path = local.root().join(&validated.full);
    let file = tokio::fs::File::open(&fs_path)
        .await
        .map_err(|error| map_local_read_error(error, validated))?;
    let len = file
        .metadata()
        .await
        .map_err(|error| map_storage_error(StorageError::Io(error)))?
        .len();
    Ok((file, len))
}

fn map_local_read_error(error: std::io::Error, validated: &ValidatedPath) -> WyrdError {
    if error.kind() == std::io::ErrorKind::NotFound {
        return map_storage_error(StorageError::ObjectNotFound {
            storage_path: validated.full.clone(),
        });
    }
    map_storage_error(StorageError::Io(error))
}

fn validate_sha256_b64(value: &str) -> Result<(), WyrdError> {
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(value)
        .map_err(|error| WyrdStorageError::Sha256Invalid {
            detail: error.to_string(),
        })?;
    if decoded.len() != 32 {
        return Err(WyrdStorageError::Sha256Invalid {
            detail: format!("decoded digest was {} bytes", decoded.len()),
        }
        .into());
    }
    Ok(())
}

fn sha256_canonical_json(body: &UploadInitRequest) -> [u8; 32] {
    let bytes = match serde_json::to_vec(body) {
        Ok(bytes) => bytes,
        Err(error) => panic!("invariant: UploadInitRequest serializes to JSON: {error}"),
    };
    Sha256::digest(bytes).into()
}

fn derive_wire_protocol(backend: StorageBackendKind, planned: PlannedUpload) -> WireProtocol {
    match (backend, planned) {
        (StorageBackendKind::Local, _) => WireProtocol::LocalFsV1,
        (_, PlannedUpload::SinglePut) => WireProtocol::SinglePutV1,
        (StorageBackendKind::S3, PlannedUpload::Multipart { .. }) => WireProtocol::S3MultipartV1,
        (StorageBackendKind::Gcs, PlannedUpload::Multipart { .. }) => WireProtocol::GcsResumableV1,
        (StorageBackendKind::Azure, PlannedUpload::Multipart { .. }) => {
            WireProtocol::AzureBlockBlobV1
        }
    }
}

struct UploadRowCounts {
    part_count: u32,
    part_size_bytes: u64,
    block_count_planned: Option<i32>,
}

fn upload_row_counts(
    planned: PlannedUpload,
    expected_size_bytes: u64,
    wire_protocol: WireProtocol,
) -> UploadRowCounts {
    let (part_count, part_size_bytes) = match planned {
        PlannedUpload::SinglePut => (1, expected_size_bytes),
        PlannedUpload::Multipart {
            part_count,
            part_size_bytes,
        } => (part_count, part_size_bytes),
    };
    let block_count_planned = if wire_protocol == WireProtocol::AzureBlockBlobV1 {
        Some(i32::try_from(part_count).unwrap_or(i32::MAX))
    } else {
        None
    };
    UploadRowCounts {
        part_count,
        part_size_bytes,
        block_count_planned,
    }
}

async fn drive_backend_init(
    state: &AppState,
    validated: &ValidatedPath,
    planned: PlannedUpload,
    expected_size_bytes: u64,
) -> Result<MultipartInit, WyrdError> {
    match planned {
        PlannedUpload::SinglePut => {
            let plan = if state.storage.backend() == StorageBackendKind::Local {
                local_single_put_plan(state, validated)?
            } else {
                state
                    .storage
                    .signer()
                    .presign_single_put(validated, expected_size_bytes, state.storage.presign_ttl())
                    .await
                    .map_err(map_storage_error)?
            };
            Ok(MultipartInit {
                plan,
                backend_upload_id: String::new(),
            })
        }
        PlannedUpload::Multipart {
            part_count,
            part_size_bytes,
        } => state
            .storage
            .signer()
            .init_multipart(
                validated,
                part_count,
                part_size_bytes,
                state.storage.presign_ttl(),
            )
            .await
            .map_err(map_storage_error),
    }
}

fn local_single_put_plan(
    state: &AppState,
    validated: &ValidatedPath,
) -> Result<UploadPlan, WyrdError> {
    let base = state.storage.public_base_url().ok_or_else(|| {
        internal_error(
            "local storage requires WYRD_PUBLIC_BASE_URL to mint upload URLs",
            serde_json::json!({ "backend": StorageBackendKind::Local }),
        )
    })?;
    let base = normalize_base_url(base);
    Ok(UploadPlan::LocalFs {
        put_url: format!("{base}/v1/cards/upload/local/{}", validated.full),
        ttl_secs: state.storage.presign_ttl_secs(),
    })
}

async fn persist_s3_upload_id_and_audit(
    conn: &mut TenantConn<'_>,
    caller: &Caller,
    upload_uuid: Uuid,
    validated: &ValidatedPath,
    backend: StorageBackendKind,
    backend_upload_id: &str,
) -> Result<(), WyrdError> {
    let persisted_backend_id = (backend == StorageBackendKind::S3).then_some(backend_upload_id);
    multipart_uploads::mark_pending(conn, upload_uuid, persisted_backend_id)
        .await
        .map_err(map_sql_error)?;
    audit::write(
        conn,
        caller,
        UploadAuditOperation::UploadInit,
        Some(upload_uuid),
        &validated.full,
        backend,
        200,
        None,
    )
    .await
}

async fn cache_init_seed(
    conn: &mut TenantConn<'_>,
    key: &IdempotencyKey,
    body_sha: &[u8; 32],
    upload_id: &UploadId,
    validated: &ValidatedPath,
    backend: StorageBackendKind,
    wire_protocol: WireProtocol,
) -> Result<(), WyrdError> {
    let seed = wyrd_sql::queries::storage::idempotency::UploadInitReplaySeed {
        upload_id: upload_id.to_string(),
        storage_path: validated.full.clone(),
        backend,
        wire_protocol,
    };
    wyrd_sql::queries::storage::idempotency::store(
        conn,
        key.as_str(),
        body_sha,
        200,
        &seed,
        Duration::from_secs(INIT_TTL_SECS),
    )
    .await
    .map_err(map_sql_error)
}

async fn load_upload(
    conn: &mut TenantConn<'_>,
    upload_uuid: Uuid,
) -> Result<MultipartUploadRow, WyrdError> {
    multipart_uploads::find_by_id(conn, upload_uuid)
        .await
        .map_err(map_sql_error)?
        .ok_or_else(|| WyrdStorageError::UploadNotFound.into())
}

async fn load_pending_upload(
    conn: &mut TenantConn<'_>,
    upload_uuid: Uuid,
) -> Result<MultipartUploadRow, WyrdError> {
    let row = load_upload(conn, upload_uuid).await?;
    if row.status != UploadStatus::Pending {
        return Err(WyrdStorageError::UploadNotPending.into());
    }
    Ok(row)
}

fn build_complete_payload(
    body: &UploadCompleteRequest,
    wire_protocol: WireProtocol,
    backend: StorageBackendKind,
    block_count_planned: Option<i32>,
    expected_sha256: &str,
) -> Result<Option<CompletePayload>, WyrdError> {
    match (body, wire_protocol, backend) {
        (
            UploadCompleteRequest::SinglePut(_),
            WireProtocol::LocalFsV1,
            StorageBackendKind::Local,
        ) => Ok(Some(CompletePayload::Local)),
        (UploadCompleteRequest::SinglePut(_), WireProtocol::SinglePutV1, _) => Ok(None),
        (
            UploadCompleteRequest::S3Multipart(S3MultipartComplete { parts }),
            WireProtocol::S3MultipartV1,
            _,
        ) => Ok(Some(CompletePayload::S3 {
            parts: parts.clone(),
            expected_sha256: expected_sha256.to_owned(),
        })),
        (UploadCompleteRequest::GcsResumable(_), WireProtocol::GcsResumableV1, _) => Ok(None),
        (
            UploadCompleteRequest::AzureBlockBlob(AzureBlockBlobComplete { block_count }),
            WireProtocol::AzureBlockBlobV1,
            _,
        ) => {
            let planned = block_count_planned.ok_or_else(|| {
                internal_error(
                    "azure upload row is missing block_count_planned",
                    serde_json::json!({ "wire_protocol": wire_protocol }),
                )
            })?;
            let planned = u32::try_from(planned).map_err(|_| {
                internal_error(
                    "azure upload row has invalid block_count_planned",
                    serde_json::json!({ "block_count_planned": planned }),
                )
            })?;
            if *block_count != planned {
                return Err(validation_error(
                    "azure block count does not match planned upload",
                    serde_json::json!({ "actual": block_count, "planned": planned }),
                ));
            }
            Ok(Some(CompletePayload::Azure {
                block_count: planned,
            }))
        }
        (_, stored, _) => Err(validation_error(
            "complete payload does not match upload protocol",
            serde_json::json!({ "wire_protocol": stored }),
        )),
    }
}

async fn verify_object_head(
    state: &AppState,
    caller: &Caller,
    upload_uuid: Uuid,
    validated: &ValidatedPath,
    row: &MultipartUploadRow,
    head: &HeadInfo,
) -> Result<(), WyrdError> {
    let expected = u64::try_from(row.expected_size_bytes).map_err(|_| {
        internal_error(
            "stored expected_size_bytes is negative",
            serde_json::json!({ "upload_id": upload_uuid, "expected_size_bytes": row.expected_size_bytes }),
        )
    })?;
    if head.size_bytes != expected {
        let error = map_storage_error(StorageError::SizeMismatch {
            expected,
            actual: head.size_bytes,
        });
        let error_code = error.code().to_owned();
        let status_code = i32::from(error.status());
        mark_failed_best_effort(
            state,
            caller,
            upload_uuid,
            validated,
            row.backend,
            FailureContext {
                operation: UploadAuditOperation::UploadComplete,
                reason: "size_mismatch",
                status_code,
                error_code: Some(&error_code),
            },
        )
        .await;
        return Err(error);
    }
    if state.storage.require_encryption() && head.sse_marker.is_none() {
        let error = map_storage_error(StorageError::EncryptionMissing);
        let error_code = error.code().to_owned();
        let status_code = i32::from(error.status());
        mark_failed_best_effort(
            state,
            caller,
            upload_uuid,
            validated,
            row.backend,
            FailureContext {
                operation: UploadAuditOperation::UploadComplete,
                reason: "encryption_missing",
                status_code,
                error_code: Some(&error_code),
            },
        )
        .await;
        return Err(error);
    }
    Ok(())
}

async fn mark_failed_best_effort(
    state: &AppState,
    caller: &Caller,
    upload_uuid: Uuid,
    validated: &ValidatedPath,
    backend: StorageBackendKind,
    ctx: FailureContext<'_>,
) {
    let Ok(mut conn) = state.postgres.tenant_conn(caller.data_tenant_id).await else {
        tracing::warn!(upload_id = %upload_uuid, "failed to acquire tenant connection for failure mark");
        return;
    };
    if let Err(error) = multipart_uploads::mark_failed(&mut conn, upload_uuid, ctx.reason).await {
        tracing::warn!(error = %error, upload_id = %upload_uuid, "failed to mark upload failed");
    }
    if let Err(error) = audit::write(
        &mut conn,
        caller,
        ctx.operation,
        Some(upload_uuid),
        &validated.full,
        backend,
        ctx.status_code,
        ctx.error_code,
    )
    .await
    {
        tracing::warn!(error = %error, upload_id = %upload_uuid, "failed to append storage failure audit");
    }
    if let Err(error) = conn.commit().await {
        tracing::warn!(error = %error, upload_id = %upload_uuid, "failed to commit failure mark");
    }
}

fn upload_id_uuid(upload_id: &UploadId) -> Result<Uuid, WyrdError> {
    upload_id.as_uuid().map_err(invalid_upload_id)
}

pub(crate) fn invalid_upload_id(error: UploadIdParseError) -> WyrdError {
    WyrdStorageError::InvalidUploadId {
        reason: error.to_string(),
    }
    .into()
}

fn normalize_base_url(base: &str) -> &str {
    base.strip_suffix('/').unwrap_or(base)
}

fn map_tenant_path(error: TenantPathError) -> WyrdError {
    match error {
        TenantPathError::TenantMismatch { prefix, caller } => {
            tracing::warn!(
                %prefix,
                %caller,
                error_class = "tenant_path_foreign",
                "storage path belongs to a different tenant"
            );
            WyrdStorageError::TenantPathForeign.into()
        }
        other => WyrdStorageError::TenantPathMismatch {
            detail: other.to_string(),
        }
        .into(),
    }
}

/// Map a SQL-layer error into the current public error catalog.
pub fn map_sql_error(error: wyrd_sql::SqlError) -> WyrdError {
    let code = error.code();
    let message = error.to_string();
    let details = serde_json::json!({ "source_code": code });
    match error {
        wyrd_sql::SqlError::NoRows => not_found_error(&message, details),
        wyrd_sql::SqlError::UniqueViolation { .. }
        | wyrd_sql::SqlError::FkViolation { .. }
        | wyrd_sql::SqlError::CheckViolation { .. }
        | wyrd_sql::SqlError::Conflict { .. } => conflict_error(&message, details),
        wyrd_sql::SqlError::RlsDenied { .. } => {
            WyrdError::PermissionDeniedRbac { message, details }
        }
        wyrd_sql::SqlError::Connect(_)
        | wyrd_sql::SqlError::Migrate(_)
        | wyrd_sql::SqlError::MigrateChecksum { .. }
        | wyrd_sql::SqlError::Query(_)
        | wyrd_sql::SqlError::InvariantViolation { .. }
        | wyrd_sql::SqlError::TxFailed(_)
        | wyrd_sql::SqlError::InsufficientPrivilege { .. }
        | wyrd_sql::SqlError::InvalidDataTenantId(_)
        | wyrd_sql::SqlError::TriggerException { .. } => {
            tracing::error!(
                error = %message,
                source_code = code,
                "storage sql internal error"
            );
            WyrdError::Internal {
                message: "storage operation failed".to_owned(),
                details: serde_json::json!({}),
            }
        }
    }
}

fn map_storage_error(error: StorageError) -> WyrdError {
    WyrdStorageError::from(error).into()
}

fn validation_error(message: impl Into<String>, details: serde_json::Value) -> WyrdError {
    WyrdError::Validation {
        message: message.into(),
        details,
    }
}

fn not_found_error(message: impl Into<String>, details: serde_json::Value) -> WyrdError {
    WyrdError::NotFound {
        message: message.into(),
        details,
    }
}

fn conflict_error(message: impl Into<String>, details: serde_json::Value) -> WyrdError {
    WyrdError::Conflict {
        message: message.into(),
        details,
    }
}

fn internal_error(message: impl Into<String>, details: serde_json::Value) -> WyrdError {
    WyrdError::Internal {
        message: message.into(),
        details,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::postgres::ServerPostgres;
    use axum::body::to_bytes;
    use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
    use std::sync::Arc;
    use wyrd_runtime::{PermissionSet, Principal, PrincipalId, PrincipalKind};
    use wyrd_spec::DataTenantId;
    use wyrd_spec::request_id::RequestId;
    use wyrd_spec::storage::SinglePutComplete;
    use wyrd_storage::{BackendConfig, StorageHandle, StorageSettings};

    #[test]
    fn local_backend_uses_local_fs_protocol_even_for_large_uploads() {
        let planned = PlannedUpload::Multipart {
            part_count: 2,
            part_size_bytes: 16,
        };

        assert_eq!(
            derive_wire_protocol(StorageBackendKind::Local, planned),
            WireProtocol::LocalFsV1
        );
    }

    #[test]
    fn azure_complete_rejects_block_count_drift() {
        let body = UploadCompleteRequest::AzureBlockBlob(AzureBlockBlobComplete { block_count: 2 });
        let err = build_complete_payload(
            &body,
            WireProtocol::AzureBlockBlobV1,
            StorageBackendKind::Azure,
            Some(3),
            "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=",
        )
        .expect_err("block count mismatch should fail");

        assert_eq!(err.status(), 400);
    }

    #[test]
    fn cloud_single_put_is_server_verified_without_backend_commit_payload() {
        let body = UploadCompleteRequest::SinglePut(SinglePutComplete {});
        let payload = build_complete_payload(
            &body,
            WireProtocol::SinglePutV1,
            StorageBackendKind::S3,
            None,
            "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=",
        )
        .expect("single put payload builds");

        assert!(payload.is_none());
    }

    #[test]
    fn upload_row_counts_persists_azure_block_count_only() {
        let planned = PlannedUpload::Multipart {
            part_count: 7,
            part_size_bytes: 16,
        };

        let azure = upload_row_counts(planned, 100, WireProtocol::AzureBlockBlobV1);
        assert_eq!(azure.part_count, 7);
        assert_eq!(azure.part_size_bytes, 16);
        assert_eq!(azure.block_count_planned, Some(7));

        let s3 = upload_row_counts(planned, 100, WireProtocol::S3MultipartV1);
        assert_eq!(s3.part_count, 7);
        assert_eq!(s3.part_size_bytes, 16);
        assert_eq!(s3.block_count_planned, None);
    }

    #[test]
    fn upload_plan_wire_protocol_is_closed() {
        let plan = UploadPlan::SinglePut {
            put_url: "https://example.test/upload".to_owned(),
            ttl_secs: 60,
            required_headers: Vec::new(),
        };
        let json = serde_json::to_value(plan).expect("plan serializes");

        assert_eq!(json["protocol"], "single_put");
    }

    #[tokio::test]
    async fn local_single_put_plan_uses_mounted_http_blob_route() {
        let root = tempfile::tempdir().expect("temp dir");
        let storage = StorageHandle::from_settings(StorageSettings {
            backend: BackendConfig::Local {
                root: root.path().to_path_buf(),
            },
            require_encryption: false,
            presign_ttl: Duration::from_secs(600),
            part_size_bytes: 16 * 1024 * 1024,
            multipart_threshold_bytes: 100 * 1024 * 1024,
            public_base_url: Some("https://wyrd.test/".to_owned()),
        })
        .await
        .expect("local storage handle");
        let postgres = Arc::new(ServerPostgres::lazy_for_tests(
            PgPoolOptions::new().connect_lazy_with(PgConnectOptions::new()),
        ));
        let state = AppState::new(postgres, Arc::clone(&storage));
        let tenant = DataTenantId::new_v7();
        let validated = ValidatedPath {
            full: format!("{tenant}/cards/018f0000-0000-7000-8000-000000000000/model.bin"),
            data_tenant_id: tenant,
            card_uid: "018f0000-0000-7000-8000-000000000000".to_owned(),
            relative_path: "model.bin".to_owned(),
        };

        let plan = local_single_put_plan(&state, &validated).expect("local plan");

        let UploadPlan::LocalFs { put_url, ttl_secs } = plan else {
            panic!("expected local_fs plan");
        };
        assert_eq!(
            put_url,
            format!("https://wyrd.test/v1/cards/upload/local/{}", validated.full)
        );
        assert_eq!(ttl_secs, 600);
    }

    #[test]
    fn download_ttl_is_clamped_to_public_bounds() {
        let card_uid =
            wyrd_spec::ids::CardUid::new("018f0000-0000-7000-8000-000000000000").expect("card uid");

        let mut body = DownloadInitRequest {
            card_uid,
            relative_path: "model.bin".to_owned(),
            ttl_secs: None,
        };
        assert_eq!(compute_download_ttl(&body, 600), 600);

        body.ttl_secs = Some(1);
        assert_eq!(compute_download_ttl(&body, 600), 60);

        body.ttl_secs = Some(300);
        assert_eq!(compute_download_ttl(&body, 600), 300);

        body.ttl_secs = Some(10_000);
        assert_eq!(compute_download_ttl(&body, 600), 3600);
    }

    #[tokio::test]
    async fn local_download_url_uses_mounted_http_blob_route() {
        let root = tempfile::tempdir().expect("temp dir");
        let storage = StorageHandle::from_settings(StorageSettings {
            backend: BackendConfig::Local {
                root: root.path().to_path_buf(),
            },
            require_encryption: false,
            presign_ttl: Duration::from_secs(900),
            part_size_bytes: 16 * 1024 * 1024,
            multipart_threshold_bytes: 100 * 1024 * 1024,
            public_base_url: Some("https://wyrd.test/".to_owned()),
        })
        .await
        .expect("local storage handle");
        let postgres = Arc::new(ServerPostgres::lazy_for_tests(
            PgPoolOptions::new().connect_lazy_with(PgConnectOptions::new()),
        ));
        let state = AppState::new(postgres, Arc::clone(&storage));
        let tenant = DataTenantId::new_v7();
        let validated = ValidatedPath {
            full: format!("{tenant}/cards/018f0000-0000-7000-8000-000000000000/model.bin"),
            data_tenant_id: tenant,
            card_uid: "018f0000-0000-7000-8000-000000000000".to_owned(),
            relative_path: "model.bin".to_owned(),
        };

        let url = local_download_url(&state, &validated).expect("download URL");

        assert_eq!(
            url,
            format!(
                "https://wyrd.test/v1/cards/download/local/{}",
                validated.full
            )
        );
    }

    #[tokio::test]
    async fn download_local_blob_streams_validated_tenant_file() {
        let root = tempfile::tempdir().expect("temp dir");
        let storage = StorageHandle::from_settings(StorageSettings {
            backend: BackendConfig::Local {
                root: root.path().to_path_buf(),
            },
            require_encryption: false,
            presign_ttl: Duration::from_secs(900),
            part_size_bytes: 16 * 1024 * 1024,
            multipart_threshold_bytes: 100 * 1024 * 1024,
            public_base_url: Some("https://wyrd.test".to_owned()),
        })
        .await
        .expect("local storage handle");
        let postgres = Arc::new(ServerPostgres::lazy_for_tests(
            PgPoolOptions::new().connect_lazy_with(PgConnectOptions::new()),
        ));
        let state = AppState::new(postgres, Arc::clone(&storage));
        let caller = read_caller();
        let path = tenant_path::build(
            caller.data_tenant_id,
            "018f0000-0000-7000-8000-000000000000",
            "nested/model.bin",
        );
        let target = root.path().join(&path);
        tokio::fs::create_dir_all(target.parent().expect("target parent"))
            .await
            .expect("create parent");
        tokio::fs::write(&target, b"download me")
            .await
            .expect("write blob");

        let response = download_local_blob(&state, caller, path)
            .await
            .expect("download response");

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response
                .headers()
                .get(header::CONTENT_TYPE)
                .and_then(|value| value.to_str().ok()),
            Some("application/octet-stream")
        );
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("collect body");
        assert_eq!(&body[..], b"download me");
    }

    #[tokio::test]
    async fn download_local_blob_requires_card_read_permission() {
        let root = tempfile::tempdir().expect("temp dir");
        let storage = StorageHandle::from_settings(StorageSettings {
            backend: BackendConfig::Local {
                root: root.path().to_path_buf(),
            },
            require_encryption: false,
            presign_ttl: Duration::from_secs(900),
            part_size_bytes: 16 * 1024 * 1024,
            multipart_threshold_bytes: 100 * 1024 * 1024,
            public_base_url: Some("https://wyrd.test".to_owned()),
        })
        .await
        .expect("local storage handle");
        let postgres = Arc::new(ServerPostgres::lazy_for_tests(
            PgPoolOptions::new().connect_lazy_with(PgConnectOptions::new()),
        ));
        let state = AppState::new(postgres, Arc::clone(&storage));
        let caller = caller_with_permissions([]);
        let path = tenant_path::build(
            caller.data_tenant_id,
            "018f0000-0000-7000-8000-000000000000",
            "model.bin",
        );

        let error = download_local_blob(&state, caller, path)
            .await
            .expect_err("missing permission should fail");

        assert_eq!(error.status(), 403);
    }

    #[tokio::test]
    async fn upload_local_blob_requires_card_write_permission() {
        let root = tempfile::tempdir().expect("temp dir");
        let storage = StorageHandle::from_settings(StorageSettings {
            backend: BackendConfig::Local {
                root: root.path().to_path_buf(),
            },
            require_encryption: false,
            presign_ttl: Duration::from_secs(900),
            part_size_bytes: 16 * 1024 * 1024,
            multipart_threshold_bytes: 100 * 1024 * 1024,
            public_base_url: Some("https://wyrd.test".to_owned()),
        })
        .await
        .expect("local storage handle");
        let postgres = Arc::new(ServerPostgres::lazy_for_tests(
            PgPoolOptions::new().connect_lazy_with(PgConnectOptions::new()),
        ));
        let state = AppState::new(postgres, Arc::clone(&storage));
        let caller = caller_with_permissions([]);
        let path = tenant_path::build(
            caller.data_tenant_id,
            "018f0000-0000-7000-8000-000000000000",
            "model.bin",
        );

        let error = upload_local_blob(&state, caller, path, Bytes::from_static(b"data"))
            .await
            .expect_err("missing permission should fail");

        assert_eq!(error.status(), 403);
    }

    #[test]
    fn build_complete_payload_catch_all_rejects_protocol_mismatch() {
        let body = UploadCompleteRequest::SinglePut(SinglePutComplete {});
        let err = build_complete_payload(
            &body,
            WireProtocol::S3MultipartV1,
            StorageBackendKind::S3,
            None,
            "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=",
        )
        .expect_err("protocol mismatch must fail");

        assert_eq!(err.status(), 400);
    }

    #[test]
    fn derive_wire_protocol_gcs_multipart_is_gcs_resumable() {
        let planned = PlannedUpload::Multipart {
            part_count: 4,
            part_size_bytes: 16 * 1024 * 1024,
        };
        assert_eq!(
            derive_wire_protocol(StorageBackendKind::Gcs, planned),
            WireProtocol::GcsResumableV1,
        );
    }

    #[test]
    fn derive_wire_protocol_azure_multipart_is_block_blob() {
        let planned = PlannedUpload::Multipart {
            part_count: 4,
            part_size_bytes: 16 * 1024 * 1024,
        };
        assert_eq!(
            derive_wire_protocol(StorageBackendKind::Azure, planned),
            WireProtocol::AzureBlockBlobV1,
        );
    }

    #[test]
    fn extract_idempotency_key_rejects_non_utf8_header() {
        let mut headers = HeaderMap::new();
        headers.insert(
            axum::http::HeaderName::from_static("idempotency-key"),
            axum::http::HeaderValue::from_bytes(&[0xFF, 0xFE]).expect("raw bytes header"),
        );
        let err = extract_idempotency_key(&headers).expect_err("non-UTF-8 key must fail");
        assert_eq!(err.status(), 400);
    }

    #[test]
    fn extract_idempotency_key_rejects_invalid_key_format() {
        let mut headers = HeaderMap::new();
        headers.insert(
            axum::http::HeaderName::from_static("idempotency-key"),
            axum::http::HeaderValue::from_static("bad"),
        );
        let err = extract_idempotency_key(&headers).expect_err("invalid key must fail");
        assert_eq!(err.status(), 400);
    }

    fn read_caller() -> Caller {
        caller_with_permissions([Permission::card_read()])
    }

    fn caller_with_permissions(permissions: impl IntoIterator<Item = Permission>) -> Caller {
        let tenant = DataTenantId::new_v7();
        Caller {
            data_tenant_id: tenant,
            principal: Principal::new(
                PrincipalId::new(uuid::Uuid::now_v7()),
                PrincipalKind::User,
                tenant,
                vec![],
                PermissionSet::from_iter(permissions),
            ),
            request_id: RequestId::parse(&uuid::Uuid::now_v7().to_string())
                .expect("request id parses"),
        }
    }
}
