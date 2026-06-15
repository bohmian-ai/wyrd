//! Storage access ledger helpers for request-scoped service code.

use crate::auth::Caller;
use sqlx::types::Uuid;
use wyrd_spec::actor::Actor;
use wyrd_spec::error::WyrdError;
use wyrd_spec::storage::StorageBackendKind;
use wyrd_sql::TenantConn;
use wyrd_sql::queries::storage::access_ledger::{self, NewLedgerEntry};

/// Append one storage access ledger row for a caller-scoped operation.
#[allow(clippy::too_many_arguments)]
pub async fn write(
    conn: &mut TenantConn<'_>,
    caller: &Caller,
    operation: &'static str,
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
            operation,
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
        Actor::Service { name } => format!("service:{name}"),
        Actor::Agent { id, .. } => format!("agent:{id}"),
    }
}
