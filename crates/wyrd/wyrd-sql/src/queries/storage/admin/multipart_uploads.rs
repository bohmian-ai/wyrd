//! Cross-tenant sweeper queries for storage uploads.

use crate::error::SqlError;
use serde::Serialize;
use sqlx::PgPool;
use sqlx::types::{Uuid, chrono};
use std::time::Duration;

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

/// Fetch expired pending uploads and stale initiating rows for the sweeper.
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
    .bind(init_grace.as_secs() as i64)
    .fetch_all(admin_pool)
    .await
    .map_err(SqlError::from)?;

    Ok(rows)
}

/// Mark an upload aborted from the admin pool.
///
/// Returns the number of rows updated. Zero indicates the upload was already
/// in a terminal state (e.g., completed concurrently), which the caller should
/// treat as a best-effort no-op rather than an error.
///
/// # Errors
/// Returns [`SqlError`] when Postgres rejects the update.
pub async fn mark_aborted_admin(
    admin_pool: &PgPool,
    id: Uuid,
    reason: &str,
) -> Result<u64, SqlError> {
    let result = sqlx::query(
        r#"
        UPDATE wyrd.storage_multipart_uploads
        SET status = 'aborted',
            aborted_at = now(),
            failure_reason = $2
        WHERE id = $1
          AND status IN ('pending', 'initiating')
        "#,
    )
    .bind(id)
    .bind(reason)
    .execute(admin_pool)
    .await
    .map_err(SqlError::from)?;

    Ok(result.rows_affected())
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
