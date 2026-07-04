//! Production-ready Postgres handle for Wyrd control-plane SQL.

use secrecy::ExposeSecret;
use sqlx::PgPool;
use wyrd_spec::DataTenantId;

use crate::dsn::ResolvedDsns;
use crate::pool::{PoolConfig, build_pool};
use crate::{SqlError, TenantConn};

/// Runtime-ready Wyrd Postgres handle.
///
/// Construction applies Wyrd migrations through the boot-only migrator role,
/// closes that migrator pool, then returns only runtime pools.
#[derive(Clone)]
pub struct WyrdPostgres {
    app: PgPool,
    platform_admin: Option<PgPool>,
}

impl WyrdPostgres {
    /// Apply Wyrd migrations and build runtime role pools from resolved DSNs.
    ///
    /// # Errors
    /// Returns [`SqlError`] when migration or pool construction fails.
    pub async fn connect_from_dsns(dsns: &ResolvedDsns) -> Result<Self, SqlError> {
        let migrator = build_pool(
            dsns.migrator.expose_secret(),
            PoolConfig::migrator_from_env(),
        )
        .await
        .map_err(SqlError::Connect)?;

        let migration_result = crate::migrate(&migrator).await;
        migrator.close().await;
        migration_result?;

        let app = build_pool(dsns.app.expose_secret(), PoolConfig::app_from_env())
            .await
            .map_err(SqlError::Connect)?;
        let platform_admin = match &dsns.platform_admin {
            Some(dsn) => Some(
                build_pool(dsn.expose_secret(), PoolConfig::platform_admin_from_env())
                    .await
                    .map_err(SqlError::Connect)?,
            ),
            None => None,
        };

        Ok(Self {
            app,
            platform_admin,
        })
    }

    /// Wrap pre-built pools into a handle.
    ///
    /// **Migrations are assumed already applied elsewhere.** Production and
    /// DB-backed tests use `connect_from_dsns`, which migrates. This seam exists
    /// only for DB-free unit tests that construct lazy pools and never issue a
    /// query. Gated behind `testing` / `cfg(test)` so it cannot be reached from a
    /// production build.
    #[cfg(any(test, feature = "testing"))]
    #[must_use]
    pub fn from_pools(app: PgPool, platform_admin: Option<PgPool>) -> Self {
        Self { app, platform_admin }
    }

    /// Borrow the RLS-enforced runtime app pool.
    #[must_use]
    pub fn app_pool(&self) -> &PgPool {
        &self.app
    }

    /// Borrow the optional audited platform-admin pool.
    #[must_use]
    pub fn platform_admin_pool(&self) -> Option<&PgPool> {
        self.platform_admin.as_ref()
    }

    /// Open a tenant-scoped transaction on the app pool.
    ///
    /// # Errors
    /// Returns [`SqlError`] when acquiring or binding the transaction fails.
    pub async fn tenant_conn(
        &self,
        data_tenant_id: DataTenantId,
    ) -> Result<TenantConn<'_>, SqlError> {
        TenantConn::acquire(&self.app, data_tenant_id).await
    }
}
