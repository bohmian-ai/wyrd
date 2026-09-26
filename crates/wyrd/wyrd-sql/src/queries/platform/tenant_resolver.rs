//! Runtime tenant slug resolver for the auth boundary.

use sqlx::{Error as SqlxError, PgPool, types::Uuid};
use wyrd_spec::{DataTenantId, TenantSlug};

use crate::{SqlError, TenantConn};

/// Resolve a URL tenant slug to its tenant UUID using the SECURITY DEFINER
/// bridge granted to the runtime `wyrd_app` role.
///
/// Returns `Ok(None)` when the slug is absent, suspended, or deleted.
///
/// # Errors
/// Returns [`SqlError::Query`] when Postgres rejects the resolver query.
/// Returns [`SqlError::InvalidDataTenantId`] if stored tenant data violates the
/// Wyrd UUIDv7 tenant-id contract.
pub async fn resolve_by_slug_for_app(
    pool: &PgPool,
    slug: &TenantSlug,
) -> Result<Option<DataTenantId>, SqlError> {
    // Dynamic query is intentional until the workspace has a refreshed SQLx
    // offline bundle for this new hot-path function. Plan 12's raw-query grep
    // allowlist should keep this exception explicit.
    let tenant_uuid =
        sqlx::query_scalar::<_, Option<Uuid>>("SELECT platform.resolve_tenant_by_slug($1)")
            .bind(slug.as_str())
            .fetch_one(pool)
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
