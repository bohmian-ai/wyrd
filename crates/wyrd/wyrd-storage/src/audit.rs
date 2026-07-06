//! Storage access ledger helpers for request-scoped service code.

use sqlx::types::Uuid;
use wyrd_spec::error::WyrdError;
use wyrd_spec::storage::StorageBackendKind;
use wyrd_sql::TenantConn;
use wyrd_sql::queries::storage::access_ledger::{self, NewLedgerEntry};

use crate::service::{StorageCaller, StoragePrincipalKind, StorageSubject};

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
    caller: &StorageCaller,
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
    .map_err(|error| crate::service::map_sql_error(&error))
}

fn subject_id(caller: &StorageCaller) -> String {
    ledger_subject(&caller.subject)
}

fn ledger_subject(subject: &StorageSubject) -> String {
    match subject.kind {
        StoragePrincipalKind::User => format!("user:{}", subject.principal_id),
        StoragePrincipalKind::Service => format!("service:{}", subject.principal_id),
        StoragePrincipalKind::Agent => format!("agent:{}", subject.principal_id),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ledger_subject_prefixes_principal_kind() {
        let principal_id = uuid::Uuid::now_v7();

        assert_eq!(
            ledger_subject(&StorageSubject {
                principal_id,
                kind: StoragePrincipalKind::User,
            }),
            format!("user:{principal_id}")
        );
        assert_eq!(
            ledger_subject(&StorageSubject {
                principal_id,
                kind: StoragePrincipalKind::Service,
            }),
            format!("service:{principal_id}")
        );
        assert_eq!(
            ledger_subject(&StorageSubject {
                principal_id,
                kind: StoragePrincipalKind::Agent,
            }),
            format!("agent:{principal_id}")
        );
    }
}
