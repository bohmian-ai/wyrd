//! Production-ready Postgres handle for Vala SQL.
//!
//! `ValaPostgres` intentionally owns its connection pools rather than borrowing
//! Wyrd-owned pools by reference. The original vala-sql design consumed pools
//! via `TenantConn` shared references; this module adds Vala-specific roles
//! (`vala_recovery`) that require dedicated pool construction at the Vala tier.

use std::env;
use std::time::Duration;

use secrecy::{ExposeSecret, SecretString};
use sqlx::PgPool;
use wyrd_sql::dsn::ResolvedDsns;
use wyrd_sql::pool::build_pool;
use wyrd_sql::{PoolConfig, SqlError};

/// Optional password env var for the `vala_recovery` role.
pub const VALA_RECOVERY_PASSWORD_ENV: &str = "VALA_RECOVERY_PASSWORD";
/// Runtime role that executes Vala recovery SECURITY DEFINER routines.
pub const VALA_RECOVERY_ROLE: &str = "vala_recovery";

/// Runtime-ready Vala Postgres handle.
///
/// Construction applies Wyrd's prerequisite migrations, then Vala migrations,
/// through the boot-only migrator role. The returned pool is a dedicated
/// Vala/Bifrost runtime pool against the same database.
#[derive(Clone)]
pub struct ValaPostgres {
    pool: PgPool,
    recovery_pool: Option<PgPool>,
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
    #[cfg(any(test, feature = "testing"))]
    #[must_use]
    pub fn from_pools(pool: PgPool, recovery_pool: Option<PgPool>) -> Self {
        Self { pool, recovery_pool }
    }

    /// Borrow the Vala/Bifrost runtime pool.
    #[must_use]
    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    /// Borrow the optional Vala recovery pool.
    #[must_use]
    pub fn recovery_pool(&self) -> Option<&PgPool> {
        self.recovery_pool.as_ref()
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
    let recovery_pool = match env::var(VALA_RECOVERY_PASSWORD_ENV).ok() {
        Some(password) => Some(connect_recovery_pool(dsns, SecretString::from(password)).await?),
        None => None,
    };

    Ok(ValaPostgres {
        pool,
        recovery_pool,
    })
}

/// Build a Vala recovery pool from a supplied role password.
///
/// # Errors
/// Returns [`SqlError`] when the DSN cannot be synthesized or the pool cannot
/// connect.
pub async fn connect_recovery_pool(
    dsns: &ResolvedDsns,
    password: SecretString,
) -> Result<PgPool, SqlError> {
    let recovery_dsn = wyrd_sql::dsn::role_dsn_from_base(&dsns.app, VALA_RECOVERY_ROLE, &password)
        .map_err(|error| SqlError::InvariantViolation {
            detail: format!("vala recovery DSN config error: {error}"),
        })?;
    build_pool(recovery_dsn.expose_secret(), vala_recovery_pool_config())
        .await
        .map_err(SqlError::Connect)
}

/// Default pool profile for Vala recovery SQL.
#[must_use]
pub fn vala_recovery_pool_config() -> PoolConfig {
    PoolConfig::from_env_with_suffix(PoolConfig::platform_admin_defaults(), "_VALA_RECOVERY")
}
