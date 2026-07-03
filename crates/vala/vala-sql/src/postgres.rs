//! Production-ready Postgres handle for Vala SQL.

use std::time::Duration;

use secrecy::ExposeSecret;
use sqlx::PgPool;
use wyrd_sql::dsn::ResolvedDsns;
use wyrd_sql::pool::build_pool;
use wyrd_sql::{PoolConfig, SqlError};

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
        Self::connect_inner(dsns, true).await
    }

    /// Apply Vala migrations after Wyrd SQL readiness has already completed.
    ///
    /// # Errors
    /// Returns [`SqlError`] when migration or pool construction fails.
    pub async fn connect_after_wyrd(dsns: &ResolvedDsns) -> Result<Self, SqlError> {
        Self::connect_inner(dsns, false).await
    }

    async fn connect_inner(
        dsns: &ResolvedDsns,
        run_wyrd_prerequisite: bool,
    ) -> Result<Self, SqlError> {
        let migrator = build_pool(
            dsns.migrator.expose_secret(),
            PoolConfig::migrator_from_env(),
        )
        .await
        .map_err(SqlError::Connect)?;

        let migration_result = async {
            if run_wyrd_prerequisite {
                wyrd_sql::migrate(&migrator).await?;
            }
            crate::migrate(&migrator).await
        }
        .await;
        migrator.close().await;
        migration_result?;

        let pool = build_pool(dsns.app.expose_secret(), vala_pool_config())
            .await
            .map_err(SqlError::Connect)?;

        Ok(Self { pool })
    }

    /// Borrow the Vala/Bifrost runtime pool.
    #[must_use]
    pub fn pool(&self) -> &PgPool {
        &self.pool
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
