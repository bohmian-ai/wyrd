//! Platform-scope reads over the tenant directory.
//!
//! `platform.tenants` is the admin-owned directory, not tenant data, so these
//! queries take the cross-tenant [`OperatorPool`] rather than a tenant
//! connection. Callers that go on to touch tenant data must open a
//! tenant-scoped connection for that work.

use sqlx::types::Uuid;
use wyrd_spec::DataTenantId;

use crate::row_types::platform::TenantRow;
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

/// Read the directory row for one tenant, including non-active states.
///
/// Callers that must decide whether a tenant may be acted on need to see
/// `suspended`, `failed`, and `provisioning` rather than have them filtered
/// out, so this deliberately does not narrow by status. A soft-deleted tenant
/// is excluded: its row survives only as history and nothing may act on it.
///
/// # Errors
/// Returns [`SqlError::Query`] when Postgres rejects the directory read.
pub async fn tenant_by_id(
    directory: &OperatorPool,
    data_tenant_id: DataTenantId,
) -> Result<Option<TenantRow>, SqlError> {
    sqlx::query_as::<_, TenantRow>(
        "SELECT data_tenant_id, slug, display_name, status, plan,
                created_at, updated_at, deleted_at
           FROM platform.tenants
          WHERE data_tenant_id = $1 AND deleted_at IS NULL",
    )
    .bind(data_tenant_id.as_uuid())
    .fetch_optional(directory.pool())
    .await
    .map_err(SqlError::from)
}

/// List the whole live tenant directory, newest first.
///
/// Unlike [`list_active_tenant_ids`], which exists for background sweeps, this
/// is the administrative view: every lifecycle state is included because an
/// operator's first question about a tenant is usually why it is not active.
/// Soft-deleted rows stay hidden.
///
/// # Errors
/// Returns [`SqlError::Query`] when Postgres rejects the directory read.
pub async fn list_tenants(directory: &OperatorPool) -> Result<Vec<TenantRow>, SqlError> {
    sqlx::query_as::<_, TenantRow>(
        "SELECT data_tenant_id, slug, display_name, status, plan,
                created_at, updated_at, deleted_at
           FROM platform.tenants
          WHERE deleted_at IS NULL
          ORDER BY created_at DESC, data_tenant_id",
    )
    .fetch_all(directory.pool())
    .await
    .map_err(SqlError::from)
}
