//! Pure storage upload service functions.

use std::path::Path;
use std::time::Duration;

use base64::Engine;
use bytes::Bytes;
use futures_util::stream::BoxStream;
use sha2::{Digest, Sha256};
use sqlx::types::Uuid;
use tracing::instrument;
use url::form_urlencoded::byte_serialize;
use wyrd_spec::DataTenantId;
use wyrd_spec::error::WyrdError;
use wyrd_spec::error::storage::WyrdStorageError;
use wyrd_spec::ids::CardUid;
use wyrd_spec::ids::IdempotencyKey;
use wyrd_spec::registry::StoredArtifactEntry;
use wyrd_spec::storage::{
    AbortResponse, AzureBlockBlobComplete, DownloadInitRequest, DownloadInitResponse, DownloadPlan,
    MultipartComplete, PartUrlResponse, StorageBackendKind, StoredObjectRef, UploadCompleteRequest,
    UploadCompleteResponse, UploadId, UploadIdParseError, UploadInitRequest, UploadInitResponse,
    UploadPlan, WireProtocol,
};
use wyrd_sql::queries::storage::multipart_uploads::{self, MultipartUploadRow, UploadStatus};
use wyrd_sql::{TenantConn, WyrdPostgres};

use crate::StorageHandle;
use crate::error::StorageError;
use crate::plan::{MAX_OBJECT_SIZE_BYTES, PlannedUpload, plan_upload};
use crate::signer::{CompletePayload, HeadInfo, MultipartInit, UploadPlanReplayInput};
use crate::tenant_path::{self, TenantPathError, ValidatedPath};

const INIT_TTL_SECS: u64 = 24 * 60 * 60;
const INIT_TTL_SECS_I64: i64 = 24 * 60 * 60;

/// Request context needed by storage orchestration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorageCaller {
    /// Tenant whose RLS-bound storage operation is executing.
    ///
    /// This is the only caller attribute storage orchestration consumes: every
    /// row it writes is isolated by this tenant. Principal identity belongs to
    /// the route that already evaluated and audited the permission decision.
    pub data_tenant_id: DataTenantId,
}

struct StorageServiceState<'a> {
    storage: &'a StorageHandle,
    postgres: &'a WyrdPostgres,
}

impl StorageServiceState<'_> {
    /// Get a tenant-scoped Postgres connection for storage operations.
    pub async fn storage_tenant_conn(
        &self,
        tenant_id: DataTenantId,
    ) -> Result<wyrd_sql::TenantConn<'_>, WyrdError> {
        self.postgres
            .tenant_conn(tenant_id)
            .await
            .map_err(|error| map_sql_error(&error))
    }
}

struct InitReplay {
    upload_id: UploadId,
    backend: StorageBackendKind,
    storage_path: String,
    validated: ValidatedPath,
    replay_input: UploadPlanReplayInput,
}

struct PriorAbort {
    validated: Result<ValidatedPath, WyrdError>,
    backend_upload_id: Option<String>,
}

struct FailureContext<'a> {
    reason: &'a str,
    status_code: i32,
    error_code: Option<&'a str>,
}

struct PersistUploadInit<'a> {
    upload_uuid: Uuid,
    upload_id: &'a UploadId,
    validated: &'a ValidatedPath,
    backend: StorageBackendKind,
    wire_protocol: WireProtocol,
    backend_upload_id: &'a str,
    idempotency_key: Option<&'a IdempotencyKey>,
    body_sha: Option<&'a [u8; 32]>,
}

/// Who owns one upload session, read from its persisted row.
///
/// Routes select the permission a follow-up operation needs from this
/// ownership rather than from anything the request names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UploadOwner {
    /// A declared file of this Card.
    Card(CardUid),
    /// A verification File upload, owned by no Card.
    File,
}

/// Read the owner of `upload_id` in the caller's tenant.
///
/// The read runs under the caller's tenant, so another tenant's upload is
/// indistinguishable from an absent one.
///
/// # Errors
/// Returns `WYRD_STORAGE_400_INVALID_UPLOAD_ID` for an identifier this
/// server never minted, `WYRD_STORAGE_404_UPLOAD_NOT_FOUND` for an upload
/// absent from the caller's tenant, an internal error for a stored Card UID
/// that does not parse, and the mapped SQL error of a failed read.
#[instrument(skip(postgres, caller), fields(tenant = %caller.data_tenant_id, upload_id = %upload_id))]
pub async fn upload_owner(
    postgres: &WyrdPostgres,
    caller: &StorageCaller,
    upload_id: &UploadId,
) -> Result<UploadOwner, WyrdError> {
    let mut conn = postgres
        .tenant_conn(caller.data_tenant_id)
        .await
        .map_err(|error| map_sql_error(&error))?;
    let row = load_upload(&mut conn, upload_id_uuid(upload_id)?).await?;
    conn.commit().await.map_err(|error| map_sql_error(&error))?;
    row.card_uid.map_or(Ok(UploadOwner::File), |uid| {
        CardUid::new(&uid).map(UploadOwner::Card).map_err(|error| {
            internal_error(
                "stored upload Card UID is invalid",
                serde_json::json!({ "upload_id": upload_id, "error": error.to_string() }),
            )
        })
    })
}

/// What one upload session stores: its owner, object path, and declared
/// digest, size, and content type.
struct UploadSubject<'a> {
    /// Owning Card; `None` for a verification File upload.
    card_uid: Option<&'a str>,
    /// Object path below the owner's namespace.
    relative_path: &'a str,
    /// Declared base64 SHA-256 digest.
    expected_sha256: &'a str,
    /// Declared byte length.
    expected_size_bytes: u64,
    /// Declared content type, when known.
    content_type: Option<&'a str>,
}

/// Initialize an artifact upload.
#[instrument(skip(storage, postgres, caller, body), fields(tenant = %caller.data_tenant_id))]
pub async fn upload_init(
    storage: &StorageHandle,
    postgres: &WyrdPostgres,
    caller: &StorageCaller,
    idempotency_key: Option<IdempotencyKey>,
    body: UploadInitRequest,
) -> Result<UploadInitResponse, WyrdError> {
    let body_sha = sha256_canonical_json(&body);
    let validated = validated_tenant_path(caller, &body)?;
    let subject = UploadSubject {
        card_uid: Some(body.card_uid.as_str()),
        relative_path: &body.relative_path,
        expected_sha256: &body.expected_sha256,
        expected_size_bytes: body.expected_size_bytes,
        content_type: body.content_type.as_deref(),
    };
    init_upload(
        &StorageServiceState { storage, postgres },
        caller,
        idempotency_key.as_ref().map(|key| (key, &body_sha)),
        &subject,
        validated,
    )
    .await
}

/// Initialize the upload of one verification File of `sha256` and
/// `size_bytes`.
///
/// The File is stored at `<tenant>/evidence/sha256/<digest>`, outside every
/// Card, and planned, transferred, and completed exactly like a Card file:
/// the same size limit, planner, and backend protocols apply, and its
/// session is owned by no Card. A re-initialized File aborts its prior
/// pending session.
///
/// # Errors
/// Returns the errors of [`upload_init`].
#[instrument(skip(storage, postgres, caller), fields(tenant = %caller.data_tenant_id, %sha256))]
pub async fn file_upload_init(
    storage: &StorageHandle,
    postgres: &WyrdPostgres,
    caller: &StorageCaller,
    sha256: &wyrd_spec::auth::Sha256Hex,
    size_bytes: u64,
) -> Result<UploadInitResponse, WyrdError> {
    let validated = tenant_path::evidence(caller.data_tenant_id, sha256);
    let expected_sha256 = base64::engine::general_purpose::STANDARD.encode(sha256.as_bytes());
    let relative_path = validated.relative_path.clone();
    let subject = UploadSubject {
        card_uid: None,
        relative_path: &relative_path,
        expected_sha256: &expected_sha256,
        expected_size_bytes: size_bytes,
        content_type: None,
    };
    init_upload(
        &StorageServiceState { storage, postgres },
        caller,
        None,
        &subject,
        validated,
    )
    .await
}

/// Plan, record, and back one upload session for `subject` at `validated`.
///
/// `idempotency` is a supplied key and the canonical body digest it replays
/// against.
///
/// # Errors
/// Returns the size-limit refusal, an idempotency conflict, the backend
/// initialization failure, and the mapped SQL errors.
async fn init_upload(
    state: &StorageServiceState<'_>,
    caller: &StorageCaller,
    idempotency: Option<(&IdempotencyKey, &[u8; 32])>,
    subject: &UploadSubject<'_>,
    validated: ValidatedPath,
) -> Result<UploadInitResponse, WyrdError> {
    let backend = state.storage.backend();
    let planned = plan_upload(
        subject.expected_size_bytes,
        backend,
        state.storage.multipart_threshold_bytes(),
    )
    .map_err(|_| {
        map_storage_error(StorageError::ArtifactTooLarge {
            actual: subject.expected_size_bytes,
            limit: MAX_OBJECT_SIZE_BYTES,
        })
    })?;
    let wire_protocol = derive_wire_protocol(backend, planned);

    let (claim_conn, replay) =
        claim_upload_init(state.postgres, caller, idempotency, subject).await?;
    if let Some(response) = replay {
        claim_conn
            .commit()
            .await
            .map_err(|error| map_sql_error(&error))?;
        return remint_replayed_upload_init(state, response).await;
    }

    let mut conn = state.storage_tenant_conn(caller.data_tenant_id).await?;
    let prior_abort = find_and_mark_prior_pending(&mut conn, caller, subject).await?;

    let upload_id = UploadId::new();
    let upload_uuid = upload_id_uuid(&upload_id)?;
    let counts = upload_row_counts(planned, subject.expected_size_bytes, wire_protocol);
    insert_initiating_upload(
        &mut conn,
        upload_uuid,
        &validated,
        subject,
        backend,
        wire_protocol,
        counts,
    )
    .await?;
    conn.commit().await.map_err(|error| map_sql_error(&error))?;

    if let Some(prior) = prior_abort {
        abort_prior_best_effort(state, prior).await;
    }

    let init = drive_backend_init_or_mark_failed(
        state,
        caller,
        upload_uuid,
        &validated,
        &upload_id,
        planned,
        subject.expected_size_bytes,
    )
    .await?;

    persist_upload_init(
        state.postgres,
        caller,
        PersistUploadInit {
            upload_uuid,
            upload_id: &upload_id,
            validated: &validated,
            backend,
            wire_protocol,
            backend_upload_id: &init.backend_upload_id,
            idempotency_key: idempotency.map(|(key, _)| key),
            body_sha: idempotency.map(|(_, sha)| sha),
        },
    )
    .await?;
    claim_conn
        .commit()
        .await
        .map_err(|error| map_sql_error(&error))?;

    Ok(UploadInitResponse {
        upload_id,
        backend,
        plan: init.plan,
        storage_path: validated.full,
    })
}

async fn persist_upload_init(
    postgres: &WyrdPostgres,
    caller: &StorageCaller,
    input: PersistUploadInit<'_>,
) -> Result<(), WyrdError> {
    let mut conn = postgres
        .tenant_conn(caller.data_tenant_id)
        .await
        .map_err(|error| map_sql_error(&error))?;
    persist_multipart_upload_id(
        &mut conn,
        input.upload_uuid,
        input.wire_protocol,
        input.backend_upload_id,
    )
    .await?;
    if let (Some(key), Some(body_sha)) = (input.idempotency_key, input.body_sha) {
        cache_init_seed(
            &mut conn,
            key,
            body_sha,
            input.upload_id,
            input.validated,
            input.backend,
            input.wire_protocol,
        )
        .await?;
    }
    conn.commit().await.map_err(|error| map_sql_error(&error))?;
    Ok(())
}

async fn claim_upload_init<'a>(
    postgres: &'a WyrdPostgres,
    caller: &StorageCaller,
    idempotency: Option<(&IdempotencyKey, &[u8; 32])>,
    subject: &UploadSubject<'_>,
) -> Result<(TenantConn<'a>, Option<InitReplay>), WyrdError> {
    let lock_identity = match (idempotency, subject.card_uid) {
        (Some((key, _)), _) => format!("idempotency:{key}"),
        (None, Some(card_uid)) => format!(
            "artifact:{card_uid}:{}:{}",
            subject.relative_path, subject.expected_sha256
        ),
        (None, None) => format!("file:{}:{}", subject.relative_path, subject.expected_sha256),
    };
    let mut conn = postgres
        .tenant_conn(caller.data_tenant_id)
        .await
        .map_err(|error| map_sql_error(&error))?;
    wyrd_sql::queries::storage::idempotency::lock_init(&mut conn, &lock_identity)
        .await
        .map_err(|error| map_sql_error(&error))?;
    let replay = match idempotency {
        Some((key, body_sha)) => try_replay_idempotent_init(caller, &mut conn, key, body_sha).await?,
        None => None,
    };
    Ok((conn, replay))
}

/// Return one multipart part URL.
#[instrument(skip(storage, postgres, caller), fields(tenant = %caller.data_tenant_id, upload_id = %upload_id))]
pub async fn upload_part_url(
    storage: &StorageHandle,
    postgres: &WyrdPostgres,
    caller: &StorageCaller,
    upload_id: UploadId,
    part_number: u32,
) -> Result<PartUrlResponse, WyrdError> {
    let state = StorageServiceState { storage, postgres };
    let upload_uuid = upload_id_uuid(&upload_id)?;

    let mut conn = state.storage_tenant_conn(caller.data_tenant_id).await?;
    let row = load_upload(&mut conn, upload_uuid).await?;
    if row.status != UploadStatus::Pending {
        return Err(conflict_error(
            "upload is not pending",
            serde_json::json!({ "upload_id": upload_id.to_string(), "status": row.status }),
        ));
    }
    if row.wire_protocol != WireProtocol::MultipartV1 {
        return Err(validation_error(
            "part-url is only valid for multipart uploads",
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
    let validated = row_path(&row, caller.data_tenant_id)?;
    let backend_upload_id = row.backend_upload_id.ok_or_else(|| {
        internal_error(
            "pending multipart upload is missing backend_upload_id",
            serde_json::json!({ "upload_id": upload_id.to_string() }),
        )
    })?;
    conn.commit().await.map_err(|error| map_sql_error(&error))?;

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
///
/// A pending upload has its backend upload finished, its object verified,
/// and its metadata recorded. An already completed upload is a replay: the
/// Card reconciler can activate a Card, completing its upload, between the
/// client's object write and this call, and a transport retry can repeat an
/// applied completion. The replay skips the backend step and re-verifies the
/// stored object, whose metadata writes are idempotent.
///
/// # Errors
/// Returns `WYRD_STORAGE_409_UPLOAD_NOT_PENDING` for an aborted or failed
/// upload, and the storage, validation, or SQL error of any failed step;
/// a failed backend completion or verification marks a pending upload failed.
#[instrument(
    skip(storage, postgres, caller, body, idempotency_key),
    fields(
        tenant = %caller.data_tenant_id,
        upload_id = %upload_id,
        idempotency_key_present = idempotency_key.is_some()
    )
)]
pub async fn upload_complete(
    storage: &StorageHandle,
    postgres: &WyrdPostgres,
    caller: &StorageCaller,
    upload_id: UploadId,
    idempotency_key: Option<IdempotencyKey>,
    body: UploadCompleteRequest,
) -> Result<UploadCompleteResponse, WyrdError> {
    let state = StorageServiceState { storage, postgres };
    let upload_uuid = upload_id_uuid(&upload_id)?;

    let mut conn = state.storage_tenant_conn(caller.data_tenant_id).await?;
    let row = load_upload(&mut conn, upload_uuid).await?;
    let replay = match row.status {
        UploadStatus::Pending => false,
        UploadStatus::Completed => true,
        UploadStatus::Initiating | UploadStatus::Aborted | UploadStatus::Failed => {
            return Err(WyrdStorageError::UploadNotPending.into());
        }
    };
    let validated = row_path(&row, caller.data_tenant_id)?;
    let payload = build_complete_payload(
        &body,
        row.wire_protocol,
        row.backend,
        row.block_count_planned,
    )?;
    conn.commit().await.map_err(|error| map_sql_error(&error))?;
    finish_upload(&state, caller, upload_uuid, validated, row, replay, payload).await
}

/// Complete an upload on the server's behalf, without a client payload.
///
/// Card reconciliation reaches completion through this entry, so it records
/// exactly what [`upload_complete`] records: an already completed upload is
/// replayed, and a pending upload whose bytes need no backend commit — a
/// single PUT, or a local write — is verified and completed. A pending
/// upload that still needs its client's commit payload is refused, since only
/// the client can supply it.
///
/// # Errors
/// Returns [`WyrdStorageError::UploadNotPending`] for an upload that is
/// initiating, aborted, failed, or still awaiting its client's commit, and
/// the errors of [`upload_complete`].
#[instrument(skip(storage, postgres, caller), fields(tenant = %caller.data_tenant_id, upload_id = %upload_id))]
pub async fn reconcile_upload(
    storage: &StorageHandle,
    postgres: &WyrdPostgres,
    caller: &StorageCaller,
    upload_id: UploadId,
) -> Result<UploadCompleteResponse, WyrdError> {
    let state = StorageServiceState { storage, postgres };
    let upload_uuid = upload_id_uuid(&upload_id)?;
    let mut conn = state.storage_tenant_conn(caller.data_tenant_id).await?;
    let row = load_upload(&mut conn, upload_uuid).await?;
    conn.commit().await.map_err(|error| map_sql_error(&error))?;
    let (replay, payload) = match (row.status, row.wire_protocol, row.backend) {
        (UploadStatus::Completed, _, _) => (true, None),
        (UploadStatus::Pending, WireProtocol::SinglePutV1, _) => (false, None),
        (UploadStatus::Pending, WireProtocol::LocalFsV1, StorageBackendKind::Local) => {
            (false, Some(CompletePayload::Local))
        }
        _ => return Err(WyrdStorageError::UploadNotPending.into()),
    };
    let validated = row_path(&row, caller.data_tenant_id)?;
    finish_upload(&state, caller, upload_uuid, validated, row, replay, payload).await
}

/// Commit, verify, and record one upload's completion.
///
/// `replay` marks an already completed upload, which re-verifies the stored
/// object without a backend commit; otherwise `payload` is the backend
/// commit payload, `None` when the protocol needs none.
///
/// # Errors
/// Returns the backend commit, head, size, encryption, or persistence
/// failure; each verification failure marks the upload failed first.
async fn finish_upload(
    state: &StorageServiceState<'_>,
    caller: &StorageCaller,
    upload_uuid: Uuid,
    validated: ValidatedPath,
    row: MultipartUploadRow,
    replay: bool,
    payload: Option<CompletePayload>,
) -> Result<UploadCompleteResponse, WyrdError> {
    if !replay {
        complete_backend_upload(state, caller, upload_uuid, &validated, &row, payload).await?;
    }
    let head = verified_object_head(state, caller, upload_uuid, &validated, &row).await?;
    verify_object_head(state, caller, upload_uuid, &validated, &row, &head).await?;

    let mut conn = state.storage_tenant_conn(caller.data_tenant_id).await?;
    persist_completed_upload(&mut conn, upload_uuid, &row).await?;
    conn.commit().await.map_err(|error| map_sql_error(&error))?;

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
#[instrument(
    skip(storage, postgres, caller, idempotency_key),
    fields(
        tenant = %caller.data_tenant_id,
        upload_id = %upload_id,
        idempotency_key_present = idempotency_key.is_some()
    )
)]
pub async fn upload_abort(
    storage: &StorageHandle,
    postgres: &WyrdPostgres,
    caller: &StorageCaller,
    upload_id: UploadId,
    idempotency_key: Option<IdempotencyKey>,
) -> Result<AbortResponse, WyrdError> {
    let state = StorageServiceState { storage, postgres };
    let upload_uuid = upload_id_uuid(&upload_id)?;

    let mut conn = state.storage_tenant_conn(caller.data_tenant_id).await?;
    let row = load_upload(&mut conn, upload_uuid).await?;
    if matches!(
        row.status,
        UploadStatus::Aborted | UploadStatus::Completed | UploadStatus::Failed
    ) {
        conn.commit().await.map_err(|error| map_sql_error(&error))?;
        return Ok(AbortResponse { aborted: false });
    }
    let validated = row_path(&row, caller.data_tenant_id)?;
    conn.commit().await.map_err(|error| map_sql_error(&error))?;

    if let Some(backend_upload_id) = row.backend_upload_id.as_deref()
        && let Err(error) = state
            .storage
            .signer()
            .abort_multipart(&validated, backend_upload_id)
            .await
    {
        tracing::warn!(error = %error, upload_id = %upload_id, "best-effort backend abort failed");
    }

    let mut conn = state.storage_tenant_conn(caller.data_tenant_id).await?;
    let aborted =
        match multipart_uploads::mark_aborted(&mut conn, upload_uuid, Some("client-abort")).await {
            Ok(()) => true,
            Err(wyrd_sql::SqlError::Conflict { .. }) => false,
            Err(error) => return Err(map_sql_error(&error)),
        };
    conn.commit().await.map_err(|error| map_sql_error(&error))?;

    Ok(AbortResponse { aborted })
}

/// Write a bounded body through an opaque, tenant-scoped local upload capability.
#[instrument(skip(storage, postgres, caller, body), fields(tenant = %caller.data_tenant_id, upload_id = %upload_id))]
pub async fn upload_local_blob(
    storage: &StorageHandle,
    postgres: &WyrdPostgres,
    caller: &StorageCaller,
    upload_id: UploadId,
    body: &[u8],
) -> Result<(), WyrdError> {
    let upload_uuid = upload_id_uuid(&upload_id)?;
    let mut conn = postgres
        .tenant_conn(caller.data_tenant_id)
        .await
        .map_err(|error| map_sql_error(&error))?;
    let row = load_pending_upload(&mut conn, upload_uuid).await?;
    if row.backend != StorageBackendKind::Local || row.wire_protocol != WireProtocol::LocalFsV1 {
        return Err(map_storage_error(StorageError::BackendCapabilityMismatch {
            signer: row.backend,
            op: "local_upload",
        }));
    }
    let validated = row_path(&row, caller.data_tenant_id)?;
    conn.commit().await.map_err(|error| map_sql_error(&error))?;

    let expected_size = u64::try_from(row.expected_size_bytes).map_err(|_| {
        internal_error(
            "stored expected_size_bytes is negative",
            serde_json::json!({ "upload_id": upload_id }),
        )
    })?;
    let actual_size = u64::try_from(body.len()).map_err(|_| {
        internal_error(
            "request body exceeds the supported size range",
            serde_json::json!({ "size_bytes": body.len() }),
        )
    })?;
    if actual_size != expected_size {
        return Err(map_storage_error(StorageError::SizeMismatch {
            expected: expected_size,
            actual: actual_size,
        }));
    }
    let actual_sha256 = base64::engine::general_purpose::STANDARD.encode(Sha256::digest(body));
    if actual_sha256 != row.expected_sha256 {
        return Err(map_storage_error(StorageError::Sha256Mismatch {
            expected: row.expected_sha256,
            actual: actual_sha256,
        }));
    }
    #[cfg_attr(
        not(feature = "cloud"),
        expect(
            irrefutable_let_patterns,
            reason = "without `cloud` the local signer is the only backend"
        )
    )]
    let crate::BackendSigner::Local(local) = storage.signer() else {
        return Err(internal_error(
            "local blob route mounted for non-local backend",
            serde_json::json!({ "backend": storage.backend() }),
        ));
    };
    local
        .write_atomically(Path::new(&validated.full), body)
        .await
        .map_err(map_storage_error)
}

/// Initialize a download of one completed Card artifact.
///
/// `stored` is the artifact's completed-object inventory entry, which the
/// caller resolved for exactly `body`'s Card and path; its size and declared
/// digest are returned with the plan.
///
/// # Errors
/// Returns the tenant-path refusal for an invalid path, an internal error for
/// a negative recorded size, and the presign failure.
#[instrument(skip(storage, caller, body, stored), fields(tenant = %caller.data_tenant_id))]
pub async fn download_init(
    storage: &StorageHandle,
    caller: &StorageCaller,
    body: DownloadInitRequest,
    stored: StoredArtifactEntry,
) -> Result<DownloadInitResponse, WyrdError> {
    let validated = validated_download_path(caller, &body)?;
    let request_ttl_secs = compute_download_ttl(&body, storage.presign_ttl_secs());
    let get_url = download_url(storage, &validated, request_ttl_secs).await?;
    let ttl_secs = if storage.backend() == StorageBackendKind::Local {
        0
    } else {
        request_ttl_secs
    };

    Ok(DownloadInitResponse {
        plan: DownloadPlan { get_url, ttl_secs },
        size_bytes: u64::try_from(stored.size_bytes).map_err(|_| {
            internal_error(
                "stored artifact size is negative",
                serde_json::json!({
                    "storage_path": validated.full,
                    "size_bytes": stored.size_bytes,
                }),
            )
        })?,
        sha256: stored.sha256,
    })
}

/// Open a local-mode blob for streaming through the authenticated server route.
///
/// The body is read through the handle's one object store, so it classifies a
/// missing object, including a key that names a directory, exactly as every
/// other storage read does.
///
/// # Errors
/// Returns the tenant-path refusal for a path outside the caller's tenant, an
/// internal error when the route is reached on a non-local backend, and the
/// mapped storage error when the object is absent or cannot be opened.
#[instrument(skip(storage), fields(tenant = %data_tenant_id))]
pub async fn download_local_blob(
    storage: &StorageHandle,
    data_tenant_id: DataTenantId,
    path: String,
) -> Result<(BoxStream<'static, Result<Bytes, StorageError>>, u64), WyrdError> {
    let validated = tenant_path::validate(&path, data_tenant_id).map_err(map_tenant_path)?;
    if storage.backend() != StorageBackendKind::Local {
        return Err(internal_error(
            "local download route mounted for non-local backend",
            serde_json::json!({ "backend": storage.backend() }),
        ));
    }
    storage
        .get_object_stream(&validated)
        .await
        .map_err(map_storage_error)
}

async fn try_replay_idempotent_init(
    caller: &StorageCaller,
    conn: &mut TenantConn<'_>,
    key: &IdempotencyKey,
    body_sha: &[u8; 32],
) -> Result<Option<InitReplay>, WyrdError> {
    let Some(cached) = wyrd_sql::queries::storage::idempotency::get(conn, key.as_str(), body_sha)
        .await
        .map_err(|error| map_sql_error(&error))?
    else {
        return Ok(None);
    };

    let upload_id: UploadId = cached
        .seed
        .upload_id
        .parse()
        .map_err(|error| invalid_upload_id(&error))?;
    let row = load_upload(conn, upload_id_uuid(&upload_id)?).await?;
    let validated = row_path(&row, caller.data_tenant_id)?;
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
    caller: &StorageCaller,
    subject: &UploadSubject<'_>,
) -> Result<Option<PriorAbort>, WyrdError> {
    let prior = multipart_uploads::find_pending_for_dedupe(
        conn,
        subject.card_uid,
        subject.relative_path,
        subject.expected_sha256,
    )
    .await
    .map_err(|error| map_sql_error(&error))?;

    if let Some(prior) = prior {
        let abort = PriorAbort {
            validated: row_path(&prior, caller.data_tenant_id),
            backend_upload_id: prior.backend_upload_id,
        };
        multipart_uploads::mark_aborted(conn, prior.id, Some("re-init"))
            .await
            .map_err(|error| map_sql_error(&error))?;
        return Ok(Some(abort));
    }

    Ok(None)
}

async fn abort_prior_best_effort(state: &StorageServiceState<'_>, prior: PriorAbort) {
    let Some(backend_upload_id) = prior.backend_upload_id else {
        return;
    };
    let validated = match prior.validated {
        Ok(validated) => validated,
        Err(error) => {
            tracing::warn!(%error, "re-init prior upload path failed validation");
            return;
        }
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

async fn remint_replayed_upload_init(
    state: &StorageServiceState<'_>,
    response: InitReplay,
) -> Result<UploadInitResponse, WyrdError> {
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
    Ok(UploadInitResponse {
        upload_id: response.upload_id,
        backend: response.backend,
        plan,
        storage_path: response.storage_path,
    })
}

fn validated_tenant_path(
    caller: &StorageCaller,
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
    caller: &StorageCaller,
    body: &DownloadInitRequest,
) -> Result<ValidatedPath, WyrdError> {
    let storage_path = tenant_path::build(
        caller.data_tenant_id,
        body.card_uid.as_str(),
        &body.relative_path,
    );
    tenant_path::validate(&storage_path, caller.data_tenant_id).map_err(map_tenant_path)
}

fn compute_download_ttl(body: &DownloadInitRequest, default_ttl_secs: u32) -> u32 {
    body.ttl_secs.unwrap_or(default_ttl_secs).clamp(60, 3600)
}

/// Mint the download URL for one validated object: the local backend's
/// root-relative route, or a backend-presigned GET valid for `ttl_secs`.
///
/// # Errors
/// Returns the mapped storage error when the backend cannot presign the GET.
async fn download_url(
    storage: &StorageHandle,
    validated: &ValidatedPath,
    ttl_secs: u32,
) -> Result<String, WyrdError> {
    if storage.backend() == StorageBackendKind::Local {
        return Ok(local_download_url(validated));
    }

    storage
        .signer()
        .presign_get(validated, Duration::from_secs(u64::from(ttl_secs)))
        .await
        .map_err(map_storage_error)
}

/// Mint the local backend's download URL for one validated object.
///
/// The stored path carries slashes, so it is carried as the percent-encoded
/// `path` query value of the documented `GET /v1/cards/download/local`
/// operation rather than as a path tail the `OpenAPI` document cannot express.
///
/// The URL is root-relative: the client resolves it against the Wyrd endpoint
/// it is already configured for, so no public base URL is configured here and
/// credentials never leave that origin.
fn local_download_url(validated: &ValidatedPath) -> String {
    let path: String = byte_serialize(validated.full.as_bytes()).collect();
    format!("/v1/cards/download/local?path={path}")
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
        (StorageBackendKind::S3 | StorageBackendKind::Gcs, PlannedUpload::Multipart { .. }) => {
            WireProtocol::MultipartV1
        }
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

async fn insert_initiating_upload(
    conn: &mut TenantConn<'_>,
    upload_uuid: Uuid,
    validated: &ValidatedPath,
    subject: &UploadSubject<'_>,
    backend: StorageBackendKind,
    wire_protocol: WireProtocol,
    counts: UploadRowCounts,
) -> Result<(), WyrdError> {
    multipart_uploads::insert_initiating(
        conn,
        multipart_uploads::NewMultipartUpload {
            id: upload_uuid,
            card_uid: subject.card_uid,
            relative_path: subject.relative_path,
            storage_path: &validated.full,
            backend,
            wire_protocol,
            expected_sha256: subject.expected_sha256,
            expected_size_bytes: i64::try_from(subject.expected_size_bytes).map_err(|_| {
                validation_error(
                    "expected_size_bytes exceeds signed storage metadata range",
                    serde_json::json!({ "expected_size_bytes": subject.expected_size_bytes }),
                )
            })?,
            content_type: subject.content_type,
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
            ttl_secs: INIT_TTL_SECS_I64,
        },
    )
    .await
    .map_err(|error| map_sql_error(&error))
}

async fn drive_backend_init(
    state: &StorageServiceState<'_>,
    validated: &ValidatedPath,
    upload_id: &UploadId,
    planned: PlannedUpload,
    expected_size_bytes: u64,
) -> Result<MultipartInit, WyrdError> {
    match planned {
        PlannedUpload::SinglePut => {
            let plan = if state.storage.backend() == StorageBackendKind::Local {
                local_single_put_plan(state.storage, upload_id)
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

async fn drive_backend_init_or_mark_failed(
    state: &StorageServiceState<'_>,
    caller: &StorageCaller,
    upload_uuid: Uuid,
    validated: &ValidatedPath,
    upload_id: &UploadId,
    planned: PlannedUpload,
    expected_size_bytes: u64,
) -> Result<MultipartInit, WyrdError> {
    match drive_backend_init(state, validated, upload_id, planned, expected_size_bytes).await {
        Ok(init) => Ok(init),
        Err(error) => {
            let error_code = error.code().to_owned();
            let status_code = i32::from(error.status());
            mark_failed(
                state,
                caller,
                upload_uuid,
                validated,
                state.storage.backend(),
                FailureContext {
                    reason: "backend init failed",
                    status_code,
                    error_code: Some(&error_code),
                },
            )
            .await?;
            Err(error)
        }
    }
}

/// Mint the local backend's single-PUT upload plan for one upload id.
///
/// Like [`local_download_url`], the target is the root-relative authenticated
/// upload route; the client resolves it against its configured Wyrd endpoint.
fn local_single_put_plan(storage: &StorageHandle, upload_id: &UploadId) -> UploadPlan {
    UploadPlan::LocalFs {
        put_url: format!("/v1/cards/upload/local/{upload_id}"),
        ttl_secs: storage.presign_ttl_secs(),
    }
}

/// Move one freshly initialized upload into `pending`, keeping the backend id.
///
/// Only the `MultipartV1` protocol (S3 and GCS) hands back an upload id its
/// part signing, completion, and abort need; every other protocol reaches
/// completion from the stored path alone. The transition is upload
/// lifecycle state, not an authorization decision, so the row it writes is the
/// only record it owes.
///
/// # Errors
/// Returns the mapped SQL failure when the transition cannot be written.
async fn persist_multipart_upload_id(
    conn: &mut TenantConn<'_>,
    upload_uuid: Uuid,
    wire_protocol: WireProtocol,
    backend_upload_id: &str,
) -> Result<(), WyrdError> {
    let persisted_backend_id =
        (wire_protocol == WireProtocol::MultipartV1).then_some(backend_upload_id);
    multipart_uploads::mark_pending(conn, upload_uuid, persisted_backend_id)
        .await
        .map_err(|error| map_sql_error(&error))
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
    .map_err(|error| map_sql_error(&error))
}

async fn load_upload(
    conn: &mut TenantConn<'_>,
    upload_uuid: Uuid,
) -> Result<MultipartUploadRow, WyrdError> {
    multipart_uploads::find_by_id(conn, upload_uuid)
        .await
        .map_err(|error| map_sql_error(&error))?
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
) -> Result<Option<CompletePayload>, WyrdError> {
    match (body, wire_protocol, backend) {
        (
            UploadCompleteRequest::SinglePut(_),
            WireProtocol::LocalFsV1,
            StorageBackendKind::Local,
        ) => Ok(Some(CompletePayload::Local)),
        (UploadCompleteRequest::SinglePut(_), WireProtocol::SinglePutV1, _) => Ok(None),
        (
            UploadCompleteRequest::Multipart(MultipartComplete { parts }),
            WireProtocol::MultipartV1,
            _,
        ) => {
            if !parts
                .iter()
                .zip(1_u32..)
                .all(|(part, number)| part.part_number == number)
            {
                return Err(validation_error(
                    "multipart parts must be numbered 1..n in order",
                    serde_json::json!({ "part_count": parts.len() }),
                ));
            }
            Ok(Some(CompletePayload::Multipart {
                parts: parts.clone(),
            }))
        }
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

async fn complete_backend_upload(
    state: &StorageServiceState<'_>,
    caller: &StorageCaller,
    upload_uuid: Uuid,
    validated: &ValidatedPath,
    row: &MultipartUploadRow,
    payload: Option<CompletePayload>,
) -> Result<(), WyrdError> {
    let Some(payload) = payload else {
        return Ok(());
    };
    if let Err(error) = state
        .storage
        .signer()
        .complete_server_side(validated, row.backend_upload_id.as_deref(), payload)
        .await
    {
        let error = map_storage_error(error);
        let error_code = error.code().to_owned();
        let status_code = i32::from(error.status());
        mark_failed(
            state,
            caller,
            upload_uuid,
            validated,
            row.backend,
            FailureContext {
                reason: "backend_complete_failed",
                status_code,
                error_code: Some(&error_code),
            },
        )
        .await?;
        return Err(error);
    }
    Ok(())
}

async fn verified_object_head(
    state: &StorageServiceState<'_>,
    caller: &StorageCaller,
    upload_uuid: Uuid,
    validated: &ValidatedPath,
    row: &MultipartUploadRow,
) -> Result<HeadInfo, WyrdError> {
    match state
        .storage
        .signer()
        .head_for_verification(validated)
        .await
    {
        Ok(head) => Ok(head),
        Err(error) => {
            let error = map_storage_error(error);
            let error_code = error.code().to_owned();
            let status_code = i32::from(error.status());
            mark_failed(
                state,
                caller,
                upload_uuid,
                validated,
                row.backend,
                FailureContext {
                    reason: "head_for_verification_failed",
                    status_code,
                    error_code: Some(&error_code),
                },
            )
            .await?;
            Err(error)
        }
    }
}

/// Mark `upload_uuid` completed and, for a Card upload, its manifest entry
/// verified, inside the caller's transaction.
///
/// Nothing is durable until the caller commits `conn`, so a failure here or
/// before that commit leaves the upload pending for a retried completion.
///
/// # Errors
/// Returns the mapped SQL error when either update fails.
async fn persist_completed_upload(
    conn: &mut TenantConn<'_>,
    upload_uuid: Uuid,
    row: &MultipartUploadRow,
) -> Result<(), WyrdError> {
    multipart_uploads::mark_completed(conn, upload_uuid)
        .await
        .map_err(|error| map_sql_error(&error))?;
    if let Some(Ok(card_uid)) = row.card_uid.as_deref().map(CardUid::new) {
        // Registration manifests are an optional owner of a storage upload.
        // Generic storage callers still succeed when no manifest row exists;
        // registered Card completion gets the durable verification marker.
        wyrd_sql::queries::cards::mark_manifest_verified(conn, &card_uid, upload_uuid).await?;
    }
    Ok(())
}

async fn verify_object_head(
    state: &StorageServiceState<'_>,
    caller: &StorageCaller,
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
        mark_failed(
            state,
            caller,
            upload_uuid,
            validated,
            row.backend,
            FailureContext {
                reason: "size_mismatch",
                status_code,
                error_code: Some(&error_code),
            },
        )
        .await?;
        return Err(error);
    }
    if state.storage.require_encryption() && head.sse_marker.is_none() {
        let error = map_storage_error(StorageError::EncryptionMissing);
        let error_code = error.code().to_owned();
        let status_code = i32::from(error.status());
        mark_failed(
            state,
            caller,
            upload_uuid,
            validated,
            row.backend,
            FailureContext {
                reason: "encryption_missing",
                status_code,
                error_code: Some(&error_code),
            },
        )
        .await?;
        return Err(error);
    }
    Ok(())
}

async fn mark_failed(
    state: &StorageServiceState<'_>,
    caller: &StorageCaller,
    upload_uuid: Uuid,
    validated: &ValidatedPath,
    backend: StorageBackendKind,
    ctx: FailureContext<'_>,
) -> Result<(), WyrdError> {
    let mut conn = state
        .postgres
        .tenant_conn(caller.data_tenant_id)
        .await
        .map_err(|error| map_sql_error(&error))?;
    multipart_uploads::mark_failed(&mut conn, upload_uuid, ctx.reason)
        .await
        .map_err(|error| map_sql_error(&error))?;
    tracing::warn!(
        upload_id = %upload_uuid,
        storage_path = %validated.full,
        %backend,
        status_code = ctx.status_code,
        error_code = ctx.error_code.unwrap_or("unclassified"),
        reason = ctx.reason,
        "upload failed"
    );
    conn.commit().await.map_err(|error| map_sql_error(&error))
}

/// Validate the stored object path of `row` by its ownership: a Card file
/// path for a Card upload, the evidence path for a File upload.
///
/// # Errors
/// Returns the mapped tenant-path refusal when the path does not have its
/// owner's shape in `tenant`.
fn row_path(row: &MultipartUploadRow, tenant: DataTenantId) -> Result<ValidatedPath, WyrdError> {
    match row.card_uid {
        Some(_) => tenant_path::validate(&row.storage_path, tenant),
        None => tenant_path::validate_evidence(&row.storage_path, tenant),
    }
    .map_err(map_tenant_path)
}

fn upload_id_uuid(upload_id: &UploadId) -> Result<Uuid, WyrdError> {
    upload_id
        .as_uuid()
        .map_err(|error| invalid_upload_id(&error))
}

/// Map an upload id parse error into the public storage error catalog.
pub fn invalid_upload_id(error: &UploadIdParseError) -> WyrdError {
    WyrdStorageError::InvalidUploadId {
        reason: error.to_string(),
    }
    .into()
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
pub fn map_sql_error(error: &wyrd_sql::SqlError) -> WyrdError {
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
        | wyrd_sql::SqlError::SchemaNotReady { .. }
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

pub(crate) fn internal_error(message: impl Into<String>, details: serde_json::Value) -> WyrdError {
    WyrdError::Internal {
        message: message.into(),
        details,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BackendConfig, StorageSettings};
    use futures_util::TryStreamExt as _;
    use wyrd_spec::storage::SinglePutComplete;

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
        )
        .expect_err("block count mismatch should fail");

        assert_eq!(err.status(), 400);
    }

    /// Multipart completion refuses parts that are not numbered 1..n in
    /// order, because the backend commits parts by position.
    #[test]
    fn multipart_complete_rejects_out_of_order_parts() {
        let part = |part_number| wyrd_spec::storage::CompletedPart {
            part_number,
            e_tag: format!("etag-{part_number}"),
        };
        let complete = |parts| {
            build_complete_payload(
                &UploadCompleteRequest::Multipart(MultipartComplete { parts }),
                WireProtocol::MultipartV1,
                StorageBackendKind::Gcs,
                None,
            )
        };

        let err = complete(vec![part(2), part(1)]).expect_err("out-of-order parts fail");
        let ordered = complete(vec![part(1), part(2)]).expect("ordered parts build");

        assert_eq!(err.status(), 400);
        assert!(matches!(ordered, Some(CompletePayload::Multipart { parts }) if parts.len() == 2));
    }

    #[test]
    fn cloud_single_put_is_server_verified_without_backend_commit_payload() {
        let body = UploadCompleteRequest::SinglePut(SinglePutComplete {});
        let payload = build_complete_payload(
            &body,
            WireProtocol::SinglePutV1,
            StorageBackendKind::S3,
            None,
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

        let s3 = upload_row_counts(planned, 100, WireProtocol::MultipartV1);
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

    /// A local upload plan targets the root-relative mounted blob route.
    #[tokio::test]
    async fn local_single_put_plan_uses_mounted_http_blob_route() {
        let root = tempfile::tempdir().expect("temp dir");
        let storage = StorageHandle::from_settings(StorageSettings {
            backend: BackendConfig::Local {
                root: root.path().to_path_buf(),
            },
            require_encryption: false,
            presign_ttl: Duration::from_mins(10),
            part_size_bytes: 16 * 1024 * 1024,
            multipart_threshold_bytes: 100 * 1024 * 1024,
        })
        .await
        .expect("local storage handle");
        let upload_id = UploadId::from_uuid(
            Uuid::parse_str("018f0000-0000-7000-8000-000000000001").expect("upload UUID"),
        );

        let plan = local_single_put_plan(&storage, &upload_id);

        let UploadPlan::LocalFs { put_url, ttl_secs } = plan else {
            panic!("expected local_fs plan");
        };
        assert_eq!(
            put_url,
            "/v1/cards/upload/local/wyu_018f0000-0000-7000-8000-000000000001"
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

    /// A local download URL is the root-relative mounted blob route.
    #[test]
    fn local_download_url_uses_mounted_http_blob_route() {
        let tenant = DataTenantId::new_v7();
        let validated = ValidatedPath {
            full: format!("{tenant}/cards/018f0000-0000-7000-8000-000000000000/model.bin"),
            data_tenant_id: tenant,
            card_uid: "018f0000-0000-7000-8000-000000000000".to_owned(),
            relative_path: "model.bin".to_owned(),
        };

        let url = local_download_url(&validated);

        assert_eq!(
            url,
            format!(
                "/v1/cards/download/local?path={}",
                validated.full.replace('/', "%2F")
            )
        );
    }

    #[tokio::test]
    async fn download_local_blob_opens_validated_tenant_file() {
        let root = tempfile::tempdir().expect("temp dir");
        let storage = StorageHandle::from_settings(StorageSettings {
            backend: BackendConfig::Local {
                root: root.path().to_path_buf(),
            },
            require_encryption: false,
            presign_ttl: Duration::from_mins(15),
            part_size_bytes: 16 * 1024 * 1024,
            multipart_threshold_bytes: 100 * 1024 * 1024,
        })
        .await
        .expect("local storage handle");
        let tenant = DataTenantId::new_v7();
        let path = tenant_path::build(
            tenant,
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

        let (stream, len) = download_local_blob(&storage, tenant, path)
            .await
            .expect("download file");
        assert_eq!(
            len,
            u64::try_from(b"download me".len()).expect("test bytes length fits u64")
        );
        let body: Vec<Bytes> = stream.try_collect().await.expect("read opened body");
        assert_eq!(body.concat(), b"download me");
    }

    #[test]
    fn build_complete_payload_catch_all_rejects_protocol_mismatch() {
        let body = UploadCompleteRequest::SinglePut(SinglePutComplete {});
        let err = build_complete_payload(
            &body,
            WireProtocol::MultipartV1,
            StorageBackendKind::S3,
            None,
        )
        .expect_err("protocol mismatch must fail");

        assert_eq!(err.status(), 400);
    }

    #[test]
    fn derive_wire_protocol_gcs_multipart_is_multipart() {
        let planned = PlannedUpload::Multipart {
            part_count: 4,
            part_size_bytes: 16 * 1024 * 1024,
        };
        assert_eq!(
            derive_wire_protocol(StorageBackendKind::Gcs, planned),
            WireProtocol::MultipartV1,
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
}
