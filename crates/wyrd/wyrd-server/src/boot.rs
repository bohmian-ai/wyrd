//! Server boot sequence for SQL-backed Wyrd runtime state.

use secrecy::ExposeSecret;
use wyrd_sql::{
    pool::{build_app_pool, build_migrator_pool, build_platform_admin_pool},
    postgres_boot::{BootError, PostgresBoot},
};
use wyrd_storage::{StorageHandle, settings::from_env as load_storage_settings};

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
    /// Storage boot failed.
    #[error(transparent)]
    Storage(#[from] wyrd_storage::StorageError),
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
    let storage_settings = load_storage_settings()?;
    tracing::info!(
        backend = %storage_settings.backend.kind(),
        require_encryption = storage_settings.require_encryption,
        presign_ttl_secs = storage_settings.presign_ttl.as_secs(),
        part_size_bytes = storage_settings.part_size_bytes,
        "storage settings loaded"
    );
    let storage = StorageHandle::from_settings(storage_settings).await?;
    tracing::info!(backend = %storage.backend(), "storage handle ready");

    Ok(AppState::new(pool, platform_admin_pool, storage))
}

#[cfg(test)]
mod tests {
    use super::*;
    use object_store::local::LocalFileSystem;
    use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
    use std::sync::Arc;
    use tempfile::tempdir;
    use wyrd_storage::{BackendSigner, LocalSigner, StorageHandle};

    #[tokio::test(flavor = "current_thread")]
    async fn app_state_retains_only_runtime_pools() {
        let app_pool = PgPoolOptions::new().connect_lazy_with(PgConnectOptions::new());
        let platform_admin_pool = PgPoolOptions::new().connect_lazy_with(PgConnectOptions::new());
        let root = tempdir().expect("temp dir");
        let signer = LocalSigner::new(root.path().to_path_buf()).expect("local signer");
        let object_store =
            Arc::new(LocalFileSystem::new_with_prefix(root.path()).expect("local object store"));
        let storage = Arc::new(StorageHandle::new(
            BackendSigner::Local(signer),
            object_store,
        ));
        let state = AppState::new(app_pool, Some(platform_admin_pool), storage);

        assert!(state.platform_admin_pool.is_some());
        assert_eq!(
            state.storage.backend(),
            wyrd_spec::storage::StorageBackendKind::Local
        );
    }
}
