//! Runtime tenant slug resolver for the auth boundary.

use sqlx::{PgPool, types::Uuid};
use wyrd_spec::{DataTenantId, TenantSlug};

use crate::SqlError;

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
