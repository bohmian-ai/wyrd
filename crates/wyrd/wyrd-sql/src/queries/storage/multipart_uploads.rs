//! Tenant-scoped CRUD for storage upload rows.

use crate::error::SqlError;
use crate::tenant_conn::TenantConn;
use serde::{Deserialize, Serialize};
use sqlx::types::{Uuid, chrono};
use std::fmt;
use std::str::FromStr;
use wyrd_spec::storage::{StorageBackendKind, WireProtocol};

/// Canonical lifecycle state for `wyrd.storage_multipart_uploads`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UploadStatus {
    /// Row was inserted before the backend initialization call.
    Initiating,
    /// Backend initialization succeeded and the client may upload bytes.
    Pending,
    /// Upload was completed and artifact metadata was written.
    Completed,
    /// Upload was aborted by caller, dedupe handling, or sweeper.
    Aborted,
    /// Upload failed terminal verification or backend completion.
    Failed,
}

impl fmt::Display for UploadStatus {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Initiating => "initiating",
            Self::Pending => "pending",
            Self::Completed => "completed",
            Self::Aborted => "aborted",
            Self::Failed => "failed",
        })
    }
}

impl FromStr for UploadStatus {
    type Err = SqlError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "initiating" => Ok(Self::Initiating),
            "pending" => Ok(Self::Pending),
            "completed" => Ok(Self::Completed),
            "aborted" => Ok(Self::Aborted),
            "failed" => Ok(Self::Failed),
            other => Err(SqlError::InvariantViolation {
                detail: format!("storage_multipart_uploads.status has invalid value {other:?}"),
            }),
        }
    }
}

/// Values required to create a two-phase upload row.
#[derive(Debug, Clone)]
pub struct NewMultipartUpload<'a> {
    /// Upload row identifier.
    pub id: Uuid,
    /// Artifact card UID.
    pub card_uid: &'a str,
    /// Object path below the card namespace.
    pub relative_path: &'a str,
    /// Full tenant-scoped storage path.
    pub storage_path: &'a str,
    /// Configured backend.
    pub backend: StorageBackendKind,
    /// Upload wire protocol.
    pub wire_protocol: WireProtocol,
    /// Expected base64 SHA-256 digest.
    pub expected_sha256: &'a str,
    /// Expected byte length.
    pub expected_size_bytes: i64,
    /// Optional content type.
    pub content_type: Option<&'a str>,
    /// Planned number of parts.
    pub part_count_planned: i32,
    /// Planned part size.
    pub part_size_bytes: i64,
    /// Planned Azure block count, when applicable.
    pub block_count_planned: Option<i32>,
    /// Row time-to-live in seconds.
    pub ttl_secs: i64,
}

/// Decoded upload row with string columns parsed to closed Wyrd enums.
#[derive(Debug, Clone, Serialize)]
pub struct MultipartUploadRow {
    /// Upload row identifier.
    pub id: Uuid,
    /// Tenant isolation key.
    pub data_tenant_id: Uuid,
    /// Artifact card UID.
    pub card_uid: String,
    /// Object path below the card namespace.
    pub relative_path: String,
    /// Full tenant-scoped storage path.
    pub storage_path: String,
    /// Configured backend.
    pub backend: StorageBackendKind,
    /// Upload wire protocol.
    pub wire_protocol: WireProtocol,
    /// Expected base64 SHA-256 digest.
    pub expected_sha256: String,
    /// Expected byte length.
    pub expected_size_bytes: i64,
    /// Optional content type.
    pub content_type: Option<String>,
    /// Planned number of parts.
    pub part_count_planned: i32,
    /// Planned part size.
    pub part_size_bytes: i64,
    /// Persisted non-bearer backend upload identifier.
    pub backend_upload_id: Option<String>,
    /// Planned Azure block count, when applicable.
    pub block_count_planned: Option<i32>,
    /// Upload lifecycle status.
    pub status: UploadStatus,
    /// Terminal failure or abort reason.
    pub failure_reason: Option<String>,
    /// Row creation time.
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// Completion time.
    pub completed_at: Option<chrono::DateTime<chrono::Utc>>,
    /// Abort or failure time.
    pub aborted_at: Option<chrono::DateTime<chrono::Utc>>,
    /// Sweeper expiration time.
    pub expires_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct MultipartUploadRowDb {
    id: Uuid,
    data_tenant_id: Uuid,
    card_uid: String,
    relative_path: String,
    storage_path: String,
    backend: String,
    wire_protocol: String,
    expected_sha256: String,
    expected_size_bytes: i64,
    content_type: Option<String>,
    part_count_planned: i32,
    part_size_bytes: i64,
    backend_upload_id: Option<String>,
    block_count_planned: Option<i32>,
    status: String,
    failure_reason: Option<String>,
    created_at: chrono::DateTime<chrono::Utc>,
    completed_at: Option<chrono::DateTime<chrono::Utc>>,
    aborted_at: Option<chrono::DateTime<chrono::Utc>>,
    expires_at: chrono::DateTime<chrono::Utc>,
}

impl TryFrom<MultipartUploadRowDb> for MultipartUploadRow {
    type Error = SqlError;

    fn try_from(row: MultipartUploadRowDb) -> Result<Self, Self::Error> {
        let backend = StorageBackendKind::from_str(&row.backend).map_err(|_| {
            SqlError::InvariantViolation {
                detail: format!(
                    "storage_multipart_uploads.backend has invalid value {:?}",
                    row.backend
                ),
            }
        })?;
        let wire_protocol = WireProtocol::from_str(&row.wire_protocol).map_err(|_| {
            SqlError::InvariantViolation {
                detail: format!(
                    "storage_multipart_uploads.wire_protocol has invalid value {:?}",
                    row.wire_protocol
                ),
            }
        })?;
        let status = UploadStatus::from_str(&row.status)?;

        Ok(Self {
            id: row.id,
            data_tenant_id: row.data_tenant_id,
            card_uid: row.card_uid,
            relative_path: row.relative_path,
            storage_path: row.storage_path,
            backend,
            wire_protocol,
            expected_sha256: row.expected_sha256,
            expected_size_bytes: row.expected_size_bytes,
            content_type: row.content_type,
            part_count_planned: row.part_count_planned,
            part_size_bytes: row.part_size_bytes,
            backend_upload_id: row.backend_upload_id,
            block_count_planned: row.block_count_planned,
            status,
            failure_reason: row.failure_reason,
            created_at: row.created_at,
            completed_at: row.completed_at,
            aborted_at: row.aborted_at,
            expires_at: row.expires_at,
        })
    }
}

/// Insert the first row in the two-phase upload initialization flow.
///
/// # Errors
/// Returns [`SqlError`] when Postgres rejects the insert.
pub async fn insert_initiating(
    conn: &mut TenantConn<'_>,
    row: NewMultipartUpload<'_>,
) -> Result<(), SqlError> {
    let backend = row.backend.to_string();
    let wire_protocol = row.wire_protocol.to_string();

    sqlx::query(
        r#"
        INSERT INTO wyrd.storage_multipart_uploads (
            id,
            data_tenant_id,
            card_uid,
            relative_path,
            storage_path,
            backend,
            wire_protocol,
            expected_sha256,
            expected_size_bytes,
            content_type,
            part_count_planned,
            part_size_bytes,
            block_count_planned,
            status,
            expires_at
        )
        VALUES (
            $1,
            wyrd.current_tenant(),
            $2,
            $3,
            $4,
            $5,
            $6,
            $7,
            $8,
            $9,
            $10,
            $11,
            $12,
            'initiating',
            now() + ($13::text || ' seconds')::interval
        )
        "#,
    )
    .bind(row.id)
    .bind(row.card_uid)
    .bind(row.relative_path)
    .bind(row.storage_path)
    .bind(backend)
    .bind(wire_protocol)
    .bind(row.expected_sha256)
    .bind(row.expected_size_bytes)
    .bind(row.content_type)
    .bind(row.part_count_planned)
    .bind(row.part_size_bytes)
    .bind(row.block_count_planned)
    .bind(row.ttl_secs)
    .execute(&mut **conn.transaction())
    .await
    .map_err(SqlError::from)?;

    Ok(())
}

/// Mark an initiating row as pending after backend initialization succeeds.
///
/// # Errors
/// Returns [`SqlError`] when the update fails or no initiating row exists.
pub async fn mark_pending(
    conn: &mut TenantConn<'_>,
    id: Uuid,
    backend_upload_id: Option<&str>,
) -> Result<(), SqlError> {
    let result = sqlx::query(
        r#"
        UPDATE wyrd.storage_multipart_uploads
        SET status = 'pending',
            backend_upload_id = $2
        WHERE id = $1
          AND status = 'initiating'
        "#,
    )
    .bind(id)
    .bind(backend_upload_id)
    .execute(&mut **conn.transaction())
    .await
    .map_err(SqlError::from)?;

    ensure_one_row(result.rows_affected(), "mark upload pending")
}

/// Find one upload row by identifier within the current tenant.
///
/// # Errors
/// Returns [`SqlError`] when the query fails or persisted enum values are invalid.
pub async fn find_by_id(
    conn: &mut TenantConn<'_>,
    id: Uuid,
) -> Result<Option<MultipartUploadRow>, SqlError> {
    let row = sqlx::query_as::<_, MultipartUploadRowDb>(
        r#"
        SELECT
            id,
            data_tenant_id,
            card_uid,
            relative_path,
            storage_path,
            backend,
            wire_protocol,
            expected_sha256,
            expected_size_bytes,
            content_type,
            part_count_planned,
            part_size_bytes,
            backend_upload_id,
            block_count_planned,
            status,
            failure_reason,
            created_at,
            completed_at,
            aborted_at,
            expires_at
        FROM wyrd.storage_multipart_uploads
        WHERE id = $1
        "#,
    )
    .bind(id)
    .fetch_optional(&mut **conn.transaction())
    .await
    .map_err(SqlError::from)?;

    row.map(MultipartUploadRow::try_from).transpose()
}

/// Find a pending upload for crash-recovery dedupe.
///
/// # Errors
/// Returns [`SqlError`] when the query fails or persisted enum values are invalid.
pub async fn find_pending_for_dedupe(
    conn: &mut TenantConn<'_>,
    card_uid: &str,
    expected_sha256: &str,
) -> Result<Option<MultipartUploadRow>, SqlError> {
    let row = sqlx::query_as::<_, MultipartUploadRowDb>(
        r#"
        SELECT
            id,
            data_tenant_id,
            card_uid,
            relative_path,
            storage_path,
            backend,
            wire_protocol,
            expected_sha256,
            expected_size_bytes,
            content_type,
            part_count_planned,
            part_size_bytes,
            backend_upload_id,
            block_count_planned,
            status,
            failure_reason,
            created_at,
            completed_at,
            aborted_at,
            expires_at
        FROM wyrd.storage_multipart_uploads
        WHERE card_uid = $1
          AND expected_sha256 = $2
          AND status = 'pending'
        FOR UPDATE
        "#,
    )
    .bind(card_uid)
    .bind(expected_sha256)
    .fetch_optional(&mut **conn.transaction())
    .await
    .map_err(SqlError::from)?;

    row.map(MultipartUploadRow::try_from).transpose()
}

/// Mark a pending upload as completed.
///
/// # Errors
/// Returns [`SqlError`] when the update fails or no pending row exists.
pub async fn mark_completed(conn: &mut TenantConn<'_>, id: Uuid) -> Result<(), SqlError> {
    let result = sqlx::query(
        r#"
        UPDATE wyrd.storage_multipart_uploads
        SET status = 'completed',
            completed_at = now()
        WHERE id = $1
          AND status = 'pending'
        "#,
    )
    .bind(id)
    .execute(&mut **conn.transaction())
    .await
    .map_err(SqlError::from)?;

    ensure_one_row(result.rows_affected(), "mark upload completed")
}

/// Mark an upload as aborted.
///
/// # Errors
/// Returns [`SqlError`] when Postgres rejects the update.
pub async fn mark_aborted(
    conn: &mut TenantConn<'_>,
    id: Uuid,
    reason: Option<&str>,
) -> Result<(), SqlError> {
    sqlx::query(
        r#"
        UPDATE wyrd.storage_multipart_uploads
        SET status = 'aborted',
            aborted_at = now(),
            failure_reason = $2
        WHERE id = $1
          AND status IN ('initiating', 'pending')
        "#,
    )
    .bind(id)
    .bind(reason)
    .execute(&mut **conn.transaction())
    .await
    .map_err(SqlError::from)?;

    Ok(())
}

/// Mark an upload as failed.
///
/// # Errors
/// Returns [`SqlError`] when Postgres rejects the update.
pub async fn mark_failed(
    conn: &mut TenantConn<'_>,
    id: Uuid,
    reason: &str,
) -> Result<(), SqlError> {
    sqlx::query(
        r#"
        UPDATE wyrd.storage_multipart_uploads
        SET status = 'failed',
            aborted_at = now(),
            failure_reason = $2
        WHERE id = $1
          AND status IN ('initiating', 'pending')
        "#,
    )
    .bind(id)
    .bind(reason)
    .execute(&mut **conn.transaction())
    .await
    .map_err(SqlError::from)?;

    Ok(())
}

fn ensure_one_row(rows_affected: u64, operation: &str) -> Result<(), SqlError> {
    if rows_affected == 1 {
        Ok(())
    } else {
        Err(SqlError::Conflict {
            detail: format!("{operation} affected {rows_affected} rows"),
        })
    }
}
