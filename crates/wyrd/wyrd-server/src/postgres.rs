//! Server-level Postgres composition for Wyrd and Vala.

use sqlx::PgPool;
use vala_sql::ValaPostgres;
use wyrd_spec::DataTenantId;
use wyrd_sql::dsn::ResolvedDsns;
use wyrd_sql::postgres_boot::{BootError, PostgresBoot};
use wyrd_sql::{SqlError, TenantConn, WyrdPostgres};

/// Errors raised while making server Postgres handles ready.
#[derive(Debug, thiserror::Error)]
pub enum ServerPostgresError {
    /// Postgres boot failed.
    #[error(transparent)]
    Boot(#[from] BootError),
    /// SQL readiness failed.
    #[error(transparent)]
    Sql(#[from] SqlError),
}

/// Runtime-ready Postgres handles for the Wyrd server.
#[derive(Clone)]
pub struct ServerPostgres {
    wyrd: WyrdPostgres,
    vala: ValaPostgres,
}

impl ServerPostgres {
    /// Build Wyrd and Vala Postgres handles from boot configuration.
    ///
    /// # Errors
    /// Returns [`ServerPostgresError`] when DSN resolution, migration, or pool
    /// construction fails.
    pub async fn connect_from_boot(boot: &PostgresBoot) -> Result<Self, ServerPostgresError> {
        let dsns = boot.dsns()?;
        Self::connect_from_dsns(&dsns).await
    }

    /// Build Wyrd and Vala Postgres handles from resolved role DSNs.
    ///
    /// # Errors
    /// Returns [`ServerPostgresError`] when migration or pool construction fails.
    pub async fn connect_from_dsns(dsns: &ResolvedDsns) -> Result<Self, ServerPostgresError> {
        let wyrd = WyrdPostgres::connect_from_dsns(dsns).await?;
        let vala = ValaPostgres::connect_after_wyrd(dsns).await?;
        Ok(Self::from_parts(wyrd, vala))
    }

    /// Compose already-built Wyrd and Vala handles.
    ///
    /// Both sub-handles must already be migration-ready. Production
    /// `connect_from_dsns` builds them the production way and calls this; tests
    /// build them via the test harness and call this. This assembles real
    /// handles — it is not a fake or pool shim.
    #[must_use]
    pub fn from_parts(wyrd: WyrdPostgres, vala: ValaPostgres) -> Self {
        Self { wyrd, vala }
    }

    /// Open a tenant-scoped transaction on the Wyrd app pool.
    ///
    /// Route-facing acquisition path. Delegates to `WyrdPostgres::tenant_conn`,
    /// preserving the RLS tenant-bind hop (`app.current_tenant` GUC).
    ///
    /// # Errors
    /// Returns [`wyrd_sql::SqlError`] when acquiring or binding the transaction fails.
    pub async fn tenant_conn(
        &self,
        data_tenant_id: DataTenantId,
    ) -> Result<TenantConn<'_>, SqlError> {
        self.wyrd.tenant_conn(data_tenant_id).await
    }

    /// Borrow the Wyrd control-plane handle.
    #[must_use]
    pub fn wyrd(&self) -> &WyrdPostgres {
        &self.wyrd
    }

    /// Borrow the Vala warehouse handle.
    #[must_use]
    pub fn vala(&self) -> &ValaPostgres {
        &self.vala
    }

    /// Borrow the RLS-enforced Wyrd app pool.
    #[must_use]
    pub fn app_pool(&self) -> &PgPool {
        self.wyrd.app_pool()
    }

    /// Borrow the optional platform-admin pool.
    #[must_use]
    pub fn platform_admin_pool(&self) -> Option<&PgPool> {
        self.wyrd.platform_admin_pool()
    }

    /// Borrow the dedicated Vala/Bifrost pool.
    #[must_use]
    pub fn vala_pool(&self) -> &PgPool {
        self.vala.pool()
    }
}
