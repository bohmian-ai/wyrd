//! Platform-scope reads over the tenant directory.
//!
//! `platform.tenants` is the admin-owned directory, not tenant data, so these
//! queries take a pool rather than a tenant connection. Callers that go on to
//! touch tenant data must open a tenant-scoped connection for that work.

use sqlx::PgPool;
use sqlx::types::Uuid;
use wyrd_spec::DataTenantId;

use crate::SqlError;

/// List every live tenant id a background owner must service.
///
/// Suspended and deleted tenants are excluded: their durable state is frozen,
/// so a sweeper must not open work against them. The order is stable by tenant
/// id so repeated sweeps visit tenants in the same sequence.
///
/// # Errors
/// Returns [`SqlError::Query`] when Postgres rejects the directory read, and
/// [`SqlError::InvalidDataTenantId`] when a stored id violates the Wyrd
/// UUIDv7 tenant-id contract.
pub async fn list_active_tenant_ids(pool: &PgPool) -> Result<Vec<DataTenantId>, SqlError> {
    let rows = sqlx::query_scalar::<_, Uuid>(
        "SELECT data_tenant_id
           FROM platform.tenants
          WHERE status = 'active'
            AND deleted_at IS NULL
          ORDER BY data_tenant_id",
    )
    .fetch_all(pool)
    .await
    .map_err(SqlError::from)?;

    rows.into_iter()
        .map(DataTenantId::new)
        .collect::<Result<Vec<_>, _>>()
        .map_err(SqlError::InvalidDataTenantId)
}
