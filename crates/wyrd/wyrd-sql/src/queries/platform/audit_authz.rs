// raw-query grep allowlist: platform administrative tables post-date the sqlx offline cache; run `mise run sqlx:prepare` to promote to macros.
//! Query slot for `platform.audit_authz`.
//!
//! The append takes the caller's open transaction rather than a pool, because
//! an authorization audit row is only meaningful if it commits or rolls back
//! with the decision it records. The caller owns the transaction lifecycle and
//! this function never commits or rolls back, matching the composition rule the
//! tenant-scoped writers follow.

use sqlx::types::Uuid;
use sqlx::{Postgres, Transaction};
use wyrd_spec::DataTenantId;

use crate::SqlError;

/// One platform-plane authorization decision, ready to append.
#[derive(Debug, Clone)]
pub struct PlatformAuthzAudit<'a> {
    /// Request correlator shared with the response and traces.
    pub request_id: &'a str,
    /// Platform principal the decision was made against.
    pub principal_id: Uuid,
    /// Credential that authenticated the request, when one is known.
    ///
    /// Recorded so an operator can answer which credential was used, which is
    /// what makes a compromised credential traceable without conflating it with
    /// the identity it authenticated.
    pub credential_id: Option<Uuid>,
    /// Permission resource label.
    pub resource: &'a str,
    /// Permission action label.
    pub action: &'a str,
    /// Tenant the operation was about, when it named one.
    pub target_tenant_id: Option<DataTenantId>,
    /// Denial reason, present exactly when the decision was a denial.
    pub deny_reason: Option<&'a str>,
}

/// Append one platform authorization decision to the audit log.
///
/// Both outcomes are recorded: `deny_reason` present means a denial, absent
/// means an allowance. The database enforces that agreement, so a caller cannot
/// record an allowance carrying a reason or a denial without one.
///
/// # Errors
/// Returns [`SqlError::Query`] when the append fails. The caller must treat
/// that as fatal to the operation and roll back: an authorization decision that
/// cannot be recorded must not proceed.
pub async fn insert_platform_authz_audit(
    tx: &mut Transaction<'_, Postgres>,
    entry: PlatformAuthzAudit<'_>,
) -> Result<(), SqlError> {
    sqlx::query(
        "INSERT INTO platform.audit_authz
             (request_id, principal_id, credential_id, resource, action,
              target_tenant_id, decision, deny_reason)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
    )
    .bind(entry.request_id)
    .bind(entry.principal_id)
    .bind(entry.credential_id)
    .bind(entry.resource)
    .bind(entry.action)
    .bind(entry.target_tenant_id.map(|tenant| tenant.as_uuid()))
    .bind(if entry.deny_reason.is_some() {
        "deny"
    } else {
        "allow"
    })
    .bind(entry.deny_reason)
    .execute(&mut **tx)
    .await
    .map_err(SqlError::from)?;
    Ok(())
}
