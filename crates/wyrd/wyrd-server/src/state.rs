//! Shared axum application state.

use sqlx::PgPool;

/// Process-wide handle registry. One instance is shared by all HTTP handlers.
///
/// Foundation commits append their owned handles here, for example storage,
/// auth verification, and policy evaluation. This skeleton ships only the
/// runtime database pools that already survive boot.
#[derive(Clone)]
pub struct AppState {
    /// Runtime `wyrd_app` pool. Tenant-scoped traffic uses this pool and RLS
    /// applies on tenant tables.
    pub pool: PgPool,
    /// Optional audited `wyrd_platform_admin` pool for cross-tenant platform
    /// operations. Dedicated deployments may leave this unset.
    pub platform_admin_pool: Option<PgPool>,
}

impl AppState {
    /// Build runtime state from the pools that survive boot.
    #[must_use]
    pub fn new(pool: PgPool, platform_admin_pool: Option<PgPool>) -> Self {
        Self {
            pool,
            platform_admin_pool,
        }
    }
}
