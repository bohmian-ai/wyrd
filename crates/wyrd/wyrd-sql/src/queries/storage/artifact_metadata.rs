//! Tenant-scoped CRUD for stored artifact metadata.
// raw-query grep allowlist: storage tables post-date the sqlx offline cache; run `mise run sqlx:prepare` to promote to macros.

use crate::error::SqlError;
use crate::tenant_conn::TenantConn;
use serde::Serialize;
use sqlx::types::{Uuid, chrono};
use std::str::FromStr;
use wyrd_spec::storage::StorageBackendKind;

/// Values required to write artifact metadata after upload completion.
#[derive(Debug, Clone)]
pub struct NewArtifactMetadata<'a> {
    /// Full tenant-scoped storage path.
    pub storage_path: &'a str,
    /// Artifact card UID.
    pub card_uid: &'a str,
    /// Verified byte length.
    pub size_bytes: i64,
    /// Verified base64 SHA-256 digest.
    pub sha256: &'a str,
    /// Optional content type.
    pub content_type: Option<&'a str>,
    /// Optional server-side encryption marker.
    pub sse_marker: Option<&'a str>,
    /// Configured backend.
    pub backend: StorageBackendKind,
}

/// Stored artifact metadata row.
#[derive(Debug, Clone, Serialize)]
pub struct ArtifactMetadataRow {
    /// Tenant isolation key.
    pub data_tenant_id: Uuid,
    /// Full tenant-scoped storage path.
    pub storage_path: String,
    /// Artifact card UID.
    pub card_uid: String,
    /// Verified byte length.
    pub size_bytes: i64,
    /// Verified base64 SHA-256 digest.
    pub sha256: String,
    /// Optional content type.
    pub content_type: Option<String>,
    /// Optional server-side encryption marker.
    pub sse_marker: Option<String>,
    /// Configured backend.
    pub backend: StorageBackendKind,
    /// Row creation time.
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct ArtifactMetadataRowDb {
    data_tenant_id: Uuid,
    storage_path: String,
    card_uid: String,
    size_bytes: i64,
    sha256: String,
    content_type: Option<String>,
    sse_marker: Option<String>,
    backend: String,
    created_at: chrono::DateTime<chrono::Utc>,
}

impl TryFrom<ArtifactMetadataRowDb> for ArtifactMetadataRow {
    type Error = SqlError;

    fn try_from(row: ArtifactMetadataRowDb) -> Result<Self, Self::Error> {
        let backend = StorageBackendKind::from_str(&row.backend).map_err(|_| {
            SqlError::InvariantViolation {
                detail: format!(
                    "storage_artifact_metadata.backend has invalid value {:?}",
                    row.backend
                ),
            }
        })?;

        Ok(Self {
            data_tenant_id: row.data_tenant_id,
            storage_path: row.storage_path,
            card_uid: row.card_uid,
            size_bytes: row.size_bytes,
            sha256: row.sha256,
            content_type: row.content_type,
            sse_marker: row.sse_marker,
            backend,
            created_at: row.created_at,
        })
    }
}

/// Insert or refresh artifact metadata for a completed upload.
///
/// # Errors
/// Returns [`SqlError`] when Postgres rejects the write.
pub async fn insert(
    conn: &mut TenantConn<'_>,
    row: NewArtifactMetadata<'_>,
) -> Result<(), SqlError> {
    let backend = row.backend.to_string();

    sqlx::query(
        r#"
        INSERT INTO wyrd.storage_artifact_metadata (
            data_tenant_id,
            storage_path,
            card_uid,
            size_bytes,
            sha256,
            content_type,
            sse_marker,
            backend
        )
        VALUES (
            wyrd.current_tenant(),
            $1,
            $2,
            $3,
            $4,
            $5,
            $6,
            $7
        )
        ON CONFLICT (data_tenant_id, storage_path) DO UPDATE SET
            card_uid = EXCLUDED.card_uid,
            size_bytes = EXCLUDED.size_bytes,
            sha256 = EXCLUDED.sha256,
            content_type = EXCLUDED.content_type,
            sse_marker = EXCLUDED.sse_marker,
            backend = EXCLUDED.backend
        "#,
    )
    .bind(row.storage_path)
    .bind(row.card_uid)
    .bind(row.size_bytes)
    .bind(row.sha256)
    .bind(row.content_type)
    .bind(row.sse_marker)
    .bind(backend)
    .execute(&mut **conn.transaction())
    .await
    .map_err(SqlError::from)?;

    Ok(())
}

/// Load artifact metadata by storage path within the current tenant.
///
/// # Errors
/// Returns [`SqlError`] when the query fails or persisted enum values are invalid.
pub async fn get(
    conn: &mut TenantConn<'_>,
    storage_path: &str,
) -> Result<Option<ArtifactMetadataRow>, SqlError> {
    let row = sqlx::query_as::<_, ArtifactMetadataRowDb>(
        r#"
        SELECT
            data_tenant_id,
            storage_path,
            card_uid,
            size_bytes,
            sha256,
            content_type,
            sse_marker,
            backend,
            created_at
        FROM wyrd.storage_artifact_metadata
        WHERE storage_path = $1
        "#,
    )
    .bind(storage_path)
    .fetch_optional(&mut **conn.transaction())
    .await
    .map_err(SqlError::from)?;

    row.map(ArtifactMetadataRow::try_from).transpose()
}

/// Delete artifact metadata owned by one Card after its backend objects are gone.
pub async fn delete_for_card(conn: &mut TenantConn<'_>, card_uid: &str) -> Result<(), SqlError> {
    sqlx::query(
        "DELETE FROM wyrd.storage_artifact_metadata \
          WHERE data_tenant_id = wyrd.current_tenant() AND card_uid = $1",
    )
    .bind(card_uid)
    .execute(&mut **conn.transaction())
    .await
    .map_err(SqlError::from)?;
    Ok(())
}
