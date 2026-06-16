//! Storage access ledger helpers for request-scoped service code.

use crate::auth::Caller;
use sqlx::types::Uuid;
use wyrd_spec::actor::Actor;
use wyrd_spec::error::WyrdError;
use wyrd_spec::storage::StorageBackendKind;
use wyrd_sql::TenantConn;
use wyrd_sql::queries::storage::access_ledger::{self, NewLedgerEntry};

/// Typed enum of every storage operation written to the audit ledger via `write`.
///
/// Exhaustiveness is enforced at compile time. Every variant must have a
/// corresponding entry in the SQL CHECK constraint on `storage_access_ledger.operation`.
/// The sweeper writes `sweeper_abort` through `write_storage_event` directly and
/// cannot use this enum (it lives in a different crate).
pub(crate) enum UploadAuditOperation {
    UploadInit,
    UploadComplete,
    UploadAbort,
    DownloadInit,
}

impl UploadAuditOperation {
    pub(crate) fn as_sql_str(&self) -> &'static str {
        match self {
            Self::UploadInit => "upload_init",
            Self::UploadComplete => "upload_complete",
            Self::UploadAbort => "upload_abort",
            Self::DownloadInit => "download_init",
        }
    }
}

/// Append one storage access ledger row for a caller-scoped operation.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn write(
    conn: &mut TenantConn<'_>,
    caller: &Caller,
    operation: UploadAuditOperation,
    upload_id: Option<Uuid>,
    storage_path: &str,
    backend: StorageBackendKind,
    status_code: i32,
    error_code: Option<&str>,
) -> Result<(), WyrdError> {
    let subject_id = subject_id(caller);
    access_ledger::append(
        conn,
        NewLedgerEntry {
            subject_id: &subject_id,
            operation: operation.as_sql_str(),
            upload_id,
            storage_path,
            backend,
            status_code,
            error_code,
            request_id: caller.request_id.as_str(),
        },
    )
    .await
    .map_err(crate::storage::service::map_sql_error)
}

fn subject_id(caller: &Caller) -> String {
    match &caller.principal.actor {
        Actor::User { id, .. } => format!("user:{id}"),
        Actor::Service { client_id, .. } => format!("service:{client_id}"),
        Actor::Agent { id, .. } => format!("agent:{id}"),
    }
}
