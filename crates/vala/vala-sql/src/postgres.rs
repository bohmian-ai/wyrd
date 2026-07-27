//! Production-ready Postgres handle for Vala SQL.
//!
//! `ValaPostgres` owns its runtime connection pool rather than borrowing a
//! Wyrd-owned pool by reference.

use std::time::Duration;

use secrecy::ExposeSecret;
use sqlx::PgPool;
use wyrd_spec::DataTenantId;
use wyrd_sql::dsn::ResolvedDsns;
use wyrd_sql::pool::build_pool;
use wyrd_sql::{PoolConfig, SqlError, TenantConn};

/// Runtime-ready Vala Postgres handle.
///
/// Construction applies Wyrd's prerequisite migrations, then Vala migrations,
/// through the boot-only migrator role. The returned pool is a dedicated
/// Vala/Bifrost runtime pool against the same database.
#[derive(Clone)]
pub struct ValaPostgres {
    pool: PgPool,
}

impl ValaPostgres {
    /// Apply prerequisite migrations and build the Vala runtime pool.
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

        let migration_result = async {
            wyrd_sql::migrate(&migrator).await?;
            crate::migrate(&migrator).await
        }
        .await;
        migrator.close().await;
        migration_result?;

        connect_runtime_pool(dsns).await
    }

    /// Apply Vala migrations after Wyrd SQL readiness has already completed.
    ///
    /// # Errors
    /// Returns [`SqlError`] when migration or pool construction fails.
    pub async fn connect_after_wyrd(dsns: &ResolvedDsns) -> Result<Self, SqlError> {
        let migrator = build_pool(
            dsns.migrator.expose_secret(),
            PoolConfig::migrator_from_env(),
        )
        .await
        .map_err(SqlError::Connect)?;

        let migration_result = crate::migrate(&migrator).await;
        migrator.close().await;
        migration_result?;

        connect_runtime_pool(dsns).await
    }

    /// Wrap pre-built pools into a handle. Migrations assumed already applied
    /// elsewhere. Used only by DB-free unit tests; production and DB-backed tests
    /// use `connect_from_dsns` / `connect_after_wyrd`. Gated behind
    /// `testing` / `cfg(test)`.
    #[must_use]
    pub fn from_pool(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Borrow the Vala/Bifrost runtime pool.
    #[must_use]
    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    /// Open a tenant-scoped transaction through the Vala application pool.
    ///
    /// Forge uses this owner boundary for every tenant mutation. The caller
    /// owns the transaction and must commit it explicitly.
    ///
    /// # Errors
    /// Returns [`SqlError`] when the transaction cannot be opened or the
    /// tenant binding cannot be applied.
    pub async fn tenant_conn(
        &self,
        data_tenant_id: DataTenantId,
    ) -> Result<TenantConn<'_>, SqlError> {
        TenantConn::acquire(&self.pool, data_tenant_id).await
    }
}

/// Default pool profile for Vala/Bifrost runtime SQL.
#[must_use]
pub fn vala_pool_config() -> PoolConfig {
    PoolConfig::from_env_with_suffix(
        PoolConfig {
            max_connections: 16,
            min_connections: 1,
            acquire_timeout: Duration::from_secs(10),
            idle_timeout: Some(Duration::from_secs(300)),
            max_lifetime: Some(Duration::from_secs(1_800)),
            statement_cache_capacity: 512,
            test_before_acquire: true,
        },
        "_VALA",
    )
}

async fn connect_runtime_pool(dsns: &ResolvedDsns) -> Result<ValaPostgres, SqlError> {
    let pool = build_pool(dsns.app.expose_secret(), vala_pool_config())
        .await
        .map_err(SqlError::Connect)?;
    Ok(ValaPostgres { pool })
}
