//! Cross-tenant sweeper queries for storage uploads.
// raw-query grep allowlist: storage tables post-date the sqlx offline cache; run `mise run sqlx:prepare` to promote to macros.

use std::time::Duration;

use crate::error::SqlError;
use serde::Serialize;
use sqlx::PgPool;
use sqlx::types::{Uuid, chrono};

/// Upload row selected for sweeper reclamation.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct ExpiredUpload {
    /// Upload row identifier.
    pub id: Uuid,
    /// Tenant isolation key.
    pub data_tenant_id: Uuid,
    /// Full tenant-scoped storage path.
    pub storage_path: String,
    /// Configured backend as stored in SQL.
    pub backend: String,
    /// Upload wire protocol as stored in SQL.
    pub wire_protocol: String,
    /// Persisted non-bearer backend upload identifier.
    pub backend_upload_id: Option<String>,
    /// Upload status as stored in SQL.
    pub status: String,
    /// Sweeper expiration time.
    pub expires_at: chrono::DateTime<chrono::Utc>,
    /// Row creation time.
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// Fetch expired pending uploads and orphaned `initiating` rows.
///
/// A `pending` row is reclaimable once its persisted expiration passes. An
/// `initiating` row has no meaningful expiration yet: it is orphaned when the
/// initiating request died before recording a backend upload, so it becomes
/// reclaimable once it is older than `init_grace`.
///
/// # Errors
/// Returns [`SqlError`] when Postgres rejects the query.
pub async fn expired_uploads_batch(
    admin_pool: &PgPool,
    limit: i64,
    init_grace: Duration,
) -> Result<Vec<ExpiredUpload>, SqlError> {
    let rows = sqlx::query_as::<_, ExpiredUpload>(
        r#"
        SELECT
            id,
            data_tenant_id,
            storage_path,
            backend,
            wire_protocol,
            backend_upload_id,
            status,
            expires_at,
            created_at
        FROM wyrd.storage_multipart_uploads
        WHERE (status = 'pending' AND expires_at < now())
           OR (status = 'initiating' AND created_at < now() - ($2::text || ' seconds')::interval)
        ORDER BY
            CASE
                WHEN status = 'pending' THEN expires_at
                ELSE created_at + ($2::text || ' seconds')::interval
            END ASC
        LIMIT $1
        "#,
    )
    .bind(limit)
    .bind(init_grace.as_secs().cast_signed())
    .fetch_all(admin_pool)
    .await
    .map_err(SqlError::from)?;

    Ok(rows)
}

/// Try to acquire the storage sweeper leader advisory lock.
///
/// # Errors
/// Returns [`SqlError`] when Postgres rejects the advisory-lock query.
pub async fn try_acquire_leader_lock(
    conn: &mut sqlx::PgConnection,
    key: i64,
) -> Result<bool, SqlError> {
    let acquired = sqlx::query_scalar::<_, bool>("SELECT pg_try_advisory_lock($1)")
        .bind(key)
        .fetch_one(conn)
        .await
        .map_err(SqlError::from)?;

    Ok(acquired)
}
