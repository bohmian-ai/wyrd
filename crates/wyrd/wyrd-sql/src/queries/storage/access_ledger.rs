//! Tenant-scoped append-only storage access ledger.

use crate::error::SqlError;
use crate::tenant_conn::TenantConn;
use sqlx::types::Uuid;
use wyrd_spec::storage::StorageBackendKind;

/// Storage access ledger entry to append.
#[derive(Debug, Clone)]
pub struct NewLedgerEntry<'a> {
    /// Subject identity that performed the operation.
    pub subject_id: &'a str,
    /// Operation tag accepted by the SQL check constraint.
    pub operation: &'a str,
    /// Optional upload identifier.
    pub upload_id: Option<Uuid>,
    /// Full tenant-scoped storage path.
    pub storage_path: &'a str,
    /// Configured backend.
    pub backend: StorageBackendKind,
    /// Returned HTTP status, or a worker status code.
    pub status_code: i32,
    /// Optional stable error code.
    pub error_code: Option<&'a str>,
    /// Request correlation identifier.
    pub request_id: &'a str,
}

/// Append one storage access ledger entry.
///
/// # Errors
/// Returns [`SqlError`] when Postgres rejects the write.
pub async fn append(conn: &mut TenantConn<'_>, entry: NewLedgerEntry<'_>) -> Result<(), SqlError> {
    let backend = entry.backend.to_string();

    sqlx::query(
        r#"
        INSERT INTO wyrd.storage_access_ledger (
            data_tenant_id,
            subject_id,
            operation,
            upload_id,
            storage_path,
            backend,
            status_code,
            error_code,
            request_id
        )
        VALUES (
            wyrd.current_tenant(),
            $1,
            $2,
            $3,
            $4,
            $5,
            $6,
            $7,
            $8
        )
        "#,
    )
    .bind(entry.subject_id)
    .bind(entry.operation)
    .bind(entry.upload_id)
    .bind(entry.storage_path)
    .bind(backend)
    .bind(entry.status_code)
    .bind(entry.error_code)
    .bind(entry.request_id)
    .execute(&mut **conn.transaction())
    .await
    .map_err(SqlError::from)?;

    Ok(())
}
