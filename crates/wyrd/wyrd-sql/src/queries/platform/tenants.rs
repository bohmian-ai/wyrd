//! Platform-scope reads over the tenant directory.
//!
//! `platform.tenants` is the admin-owned directory, not tenant data, so these
//! queries take the cross-tenant [`OperatorPool`] rather than a tenant
//! connection. Callers that go on to touch tenant data must open a
//! tenant-scoped connection for that work.

use sqlx::types::Uuid;
use wyrd_spec::DataTenantId;

use crate::{OperatorPool, SqlError};

/// List every live tenant id a background owner must service.
///
/// Suspended and deleted tenants are excluded: their durable state is frozen,
/// so a sweeper must not open work against them. The order is stable by tenant
/// id so repeated sweeps visit tenants in the same sequence.
///
/// The read runs on the admin-owned [`OperatorPool`] because the application
/// role is granted no read on `platform.tenants`: a sweep that listed tenants
/// on the RLS-enforced pool would be refused by Postgres and service nobody.
///
/// The directory always holds the [`DataTenantId::SYSTEM_OWNER`] system tenant.
///
/// # Errors
/// Returns [`SqlError::Query`] when Postgres rejects the directory read, and
/// [`SqlError::InvalidDataTenantId`] when a stored id violates the Wyrd
/// UUIDv7 tenant-id contract.
pub async fn list_active_tenant_ids(
    directory: &OperatorPool,
) -> Result<Vec<DataTenantId>, SqlError> {
    let rows = sqlx::query_scalar::<_, Uuid>(
        "SELECT data_tenant_id
           FROM platform.tenants
          WHERE status = 'active'
            AND deleted_at IS NULL
          ORDER BY data_tenant_id",
    )
    .fetch_all(directory.pool())
    .await
    .map_err(SqlError::from)?;

    rows.into_iter()
        .map(DataTenantId::new)
        .collect::<Result<Vec<_>, _>>()
        .map_err(SqlError::InvalidDataTenantId)
}
