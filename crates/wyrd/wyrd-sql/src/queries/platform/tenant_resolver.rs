//! Runtime tenant slug resolver for the auth boundary.

use sqlx::{Error as SqlxError, types::Uuid};
use wyrd_spec::{DataTenantId, TenantSlug};

use crate::{OperatorPool, SqlError, TenantConn};

/// Resolve a URL tenant slug to its tenant UUID through the SECURITY DEFINER
/// `platform.resolve_tenant_by_slug` bridge on the operator pool.
///
/// A slug names no tenant yet, so this pre-tenant directory read runs on the
/// audited `wyrd_platform_admin` role rather than a tenant transaction. It is
/// crate-private: [`crate::WyrdPostgres::resolve_tenant_slug`] is the single
/// resolver every login, workload, and boot caller goes through, and it fails
/// closed when no operator pool is configured.
///
/// Returns `Ok(None)` when the slug is absent, suspended, or deleted.
///
/// # Errors
/// Returns [`SqlError::Query`] when Postgres rejects the resolver query.
/// Returns [`SqlError::InvalidDataTenantId`] if stored tenant data violates the
/// Wyrd UUIDv7 tenant-id contract.
pub(crate) async fn resolve_by_slug(
    operator: &OperatorPool,
    slug: &TenantSlug,
) -> Result<Option<DataTenantId>, SqlError> {
    // Dynamic query is intentional until the workspace has a refreshed SQLx
    // offline bundle for this hot-path function; the raw-query grep allowlist
    // keeps this exception explicit.
    let tenant_uuid =
        sqlx::query_scalar::<_, Option<Uuid>>("SELECT platform.resolve_tenant_by_slug($1)")
            .bind(slug.as_str())
            .fetch_one(operator.pool())
            .await
            .map_err(SqlError::from)?;

    tenant_uuid
        .map(DataTenantId::new)
        .transpose()
        .map_err(SqlError::InvalidDataTenantId)
}

/// Report whether a tenant's lifecycle state admits its credentials.
///
/// The authentication path's one question about the tenant directory, answered
/// through a SECURITY DEFINER function so the `wyrd_app` role never gains read
/// access to `platform.tenants` itself. A tenant that is provisioning, failed,
/// suspended, or deleted admits nothing; only an active, undeleted tenant does.
///
/// Takes the row-level-secured connection because it is called on the exchange
/// path, which has no operator boundary and must not acquire one.
///
/// # Errors
/// Returns a SQLx error when the call fails. A failure is not a refusal: the
/// caller must fail closed rather than treat an unavailable directory as an
/// inactive tenant or an active one.
pub async fn tenant_admits_credentials(
    conn: &mut TenantConn<'_>,
    tenant: DataTenantId,
) -> Result<bool, SqlxError> {
    sqlx::query_scalar::<_, bool>("SELECT platform.tenant_admits_credentials($1)")
        .bind(tenant.as_uuid())
        .fetch_one(&mut **conn.transaction())
        .await
}
