// raw-query grep allowlist: platform administrative tables post-date the sqlx offline cache; run `mise run sqlx:prepare` to promote to macros.
//! Tenant provisioning writes against the tenant directory.
//!
//! Provisioning spans two boundaries: the directory row lives at platform scope
//! behind the operator role, while the tenant's own principal, roles, and
//! credential live under row-level security inside the tenant. One transaction
//! cannot cover both, so the directory row is created in a non-usable state
//! first and only promoted once the tenant-scoped work has committed. A failure
//! anywhere in between leaves a tenant that is visibly incomplete rather than
//! one that looks live but has no way in.

use sqlx::{Postgres, Transaction};
use wyrd_spec::DataTenantId;

use crate::{OperatorPool, SqlError};

/// Create a tenant directory row in the provisioning state.
///
/// Takes the caller's transaction so the row is created in the same transaction
/// that recorded the authorization decision permitting it. The tenant is not
/// usable on commit: no consumer that filters on live tenants will see it.
///
/// # Errors
/// Returns [`SqlError::Query`] when the insert fails, including when `slug` is
/// already taken — which is how a concurrent creation for the same tenant
/// converges on one row rather than two.
pub async fn insert_provisioning_tenant(
    tx: &mut Transaction<'_, Postgres>,
    data_tenant_id: DataTenantId,
    slug: &str,
    display_name: &str,
) -> Result<(), SqlError> {
    sqlx::query(
        "INSERT INTO platform.tenants (data_tenant_id, slug, display_name, status)
         VALUES ($1, $2, $3, 'provisioning')",
    )
    .bind(data_tenant_id.as_uuid())
    .bind(slug)
    .bind(display_name)
    .execute(&mut **tx)
    .await
    .map_err(SqlError::from)?;
    Ok(())
}

/// Promote a provisioning tenant to active.
///
/// Only a tenant still in the provisioning state is promoted, so a retry that
/// races a completed attempt promotes nothing rather than resurrecting a
/// suspended or failed tenant. Returns whether this call performed the
/// promotion.
///
/// # Errors
/// Returns [`SqlError::Query`] when the update fails.
pub async fn mark_tenant_active(
    pool: &OperatorPool,
    data_tenant_id: DataTenantId,
) -> Result<bool, SqlError> {
    let result = sqlx::query(
        "UPDATE platform.tenants
            SET status = 'active', updated_at = now()
          WHERE data_tenant_id = $1 AND status = 'provisioning'",
    )
    .bind(data_tenant_id.as_uuid())
    .execute(pool.pool())
    .await
    .map_err(SqlError::from)?;
    Ok(result.rows_affected() == 1)
}

/// Record that provisioning did not complete.
///
/// Leaves the tenant visibly incomplete with the reason attached, so an
/// operator can see why it never became usable and a retry knows it is resuming
/// rather than starting fresh.
///
/// # Errors
/// Returns [`SqlError::Query`] when the update fails.
pub async fn mark_tenant_failed(
    pool: &OperatorPool,
    data_tenant_id: DataTenantId,
    reason: &str,
) -> Result<(), SqlError> {
    sqlx::query(
        "UPDATE platform.tenants
            SET status = 'failed', provisioning_failed_reason = $2, updated_at = now()
          WHERE data_tenant_id = $1 AND status = 'provisioning'",
    )
    .bind(data_tenant_id.as_uuid())
    .bind(reason)
    .execute(pool.pool())
    .await
    .map_err(SqlError::from)?;
    Ok(())
}

/// Set a tenant's suspension state without destroying it.
///
/// Suspension freezes a tenant: its principals stop authenticating while every
/// row, grant, and credential survives, so resuming restores exactly what was
/// there. Returns whether the transition applied.
///
/// # Errors
/// Returns [`SqlError::Query`] when the update fails.
pub async fn set_tenant_suspended(
    pool: &OperatorPool,
    data_tenant_id: DataTenantId,
    suspended: bool,
) -> Result<bool, SqlError> {
    let (next, expected) = if suspended {
        ("suspended", "active")
    } else {
        ("active", "suspended")
    };
    let result = sqlx::query(
        "UPDATE platform.tenants
            SET status = $2, updated_at = now()
          WHERE data_tenant_id = $1 AND status = $3",
    )
    .bind(data_tenant_id.as_uuid())
    .bind(next)
    .bind(expected)
    .execute(pool.pool())
    .await
    .map_err(SqlError::from)?;
    Ok(result.rows_affected() == 1)
}
