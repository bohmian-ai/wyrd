//! Cross-domain audit write helpers.
//!
//! Platform audit rows live in `platform.audit_log`; storage access events live
//! in `wyrd.storage_access_ledger` through the tenant-scoped writer below.

use crate::error::SqlError;
use crate::tenant_conn::TenantConn;
use sqlx::types::Uuid;
use wyrd_spec::storage::StorageBackendKind;

/// Tenant-scoped storage audit event.
#[derive(Debug, Clone)]
pub struct StorageAuditEvent<'a> {
    /// Subject identity that performed the operation.
    pub subject_id: &'a str,
    /// Operation tag accepted by the storage ledger constraint.
    pub operation: &'a str,
    /// Full tenant-scoped storage path.
    pub storage_path: &'a str,
    /// Returned HTTP status, or worker status code.
    pub status_code: i32,
    /// Optional stable error code.
    pub error_code: Option<&'a str>,
    /// Request correlation identifier.
    pub request_id: &'a str,
    /// Configured backend.
    pub backend: StorageBackendKind,
    /// Optional upload identifier.
    pub upload_id: Option<Uuid>,
}

/// Write one tenant-scoped storage audit event.
///
/// # Errors
/// Returns [`SqlError`] when Postgres rejects the ledger write.
pub async fn write_storage_event(
    conn: &mut TenantConn<'_>,
    event: StorageAuditEvent<'_>,
) -> Result<(), SqlError> {
    crate::queries::storage::access_ledger::append(
        conn,
        crate::queries::storage::access_ledger::NewLedgerEntry {
            subject_id: event.subject_id,
            operation: event.operation,
            upload_id: event.upload_id,
            storage_path: event.storage_path,
            backend: event.backend,
            status_code: event.status_code,
            error_code: event.error_code,
            request_id: event.request_id,
        },
    )
    .await
}
