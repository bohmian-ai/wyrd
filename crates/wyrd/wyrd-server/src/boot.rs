//! Server boot sequence for SQL-backed Wyrd runtime state.

use secrecy::ExposeSecret;
use wyrd_sql::{
    pool::{build_app_pool, build_migrator_pool, build_platform_admin_pool},
    postgres_boot::{BootError, PostgresBoot},
};

use crate::state::AppState;

/// Errors raised while assembling server state.
#[derive(Debug, thiserror::Error)]
pub enum ServerBootError {
    /// Postgres boot or pool construction failed.
    #[error(transparent)]
    Postgres(#[from] BootError),
    /// Runtime pool construction failed.
    #[error("database pool construction failed")]
    PoolConnect(#[source] sqlx::Error),
    /// SQL migrations failed.
    #[error(transparent)]
    Sql(#[from] wyrd_sql::error::SqlError),
}

/// Resolve database configuration, run migrations, and assemble runtime state.
///
/// The boot-only migrator pool is closed before runtime pools are constructed
/// so a BYPASSRLS connection cannot survive into request handling.
///
/// # Errors
/// Returns [`ServerBootError`] when database boot, migration, or runtime pool
/// construction fails.
pub async fn build_app_state() -> Result<AppState, ServerBootError> {
    let boot = PostgresBoot::from_env().await?;
    build_app_state_from_boot(&boot).await
}

/// Assemble runtime state from a resolved Postgres boot mode.
///
/// # Errors
/// Returns [`ServerBootError`] when DSN resolution, migrations, or runtime
/// pool construction fails.
pub async fn build_app_state_from_boot(boot: &PostgresBoot) -> Result<AppState, ServerBootError> {
    let dsns = boot.dsns()?;
    let migrator_pool = build_migrator_pool(dsns.migrator.expose_secret())
        .await
        .map_err(ServerBootError::PoolConnect)?;

    let migration_result: Result<(), wyrd_sql::error::SqlError> = async {
        wyrd_sql::migrate(&migrator_pool).await?;
        vala_sql::migrate(&migrator_pool).await?;
        Ok(())
    }
    .await;
    migrator_pool.close().await;
    migration_result?;

    let pool = build_app_pool(dsns.app.expose_secret())
        .await
        .map_err(ServerBootError::PoolConnect)?;
    let platform_admin_pool = match dsns.platform_admin {
        Some(dsn) => Some(
            build_platform_admin_pool(dsn.expose_secret())
                .await
                .map_err(ServerBootError::PoolConnect)?,
        ),
        None => None,
    };

    Ok(AppState::new(pool, platform_admin_pool))
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::postgres::{PgConnectOptions, PgPoolOptions};

    #[tokio::test(flavor = "current_thread")]
    async fn app_state_retains_only_runtime_pools() {
        let app_pool = PgPoolOptions::new().connect_lazy_with(PgConnectOptions::new());
        let platform_admin_pool = PgPoolOptions::new().connect_lazy_with(PgConnectOptions::new());
        let state = AppState::new(app_pool, Some(platform_admin_pool));

        assert!(state.platform_admin_pool.is_some());
    }
}
