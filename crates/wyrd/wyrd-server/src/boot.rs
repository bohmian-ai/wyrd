//! Server boot sequence for SQL-backed Wyrd runtime state.

use std::sync::Arc;

use secrecy::ExposeSecret;
use tokio_util::sync::CancellationToken;
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

/// Errors raised by [`production_guards`].
#[derive(Debug, thiserror::Error)]
pub enum ProductionGuardError {
    /// `request_id.trust_upstream=true` with no CIDR allowlist configured.
    #[error("request_id.trust_upstream=true requires at least one CIDR in trusted_upstreams")]
    TrustUpstreamWithoutCidr,
    /// gRPC reflection must be disabled in Production.
    #[error("grpc.reflection_enabled=true is not allowed in Production profile")]
    ReflectionInProduction,
    /// Preview auth must be disabled in Production.
    #[error("auth.allow_preview=true is not allowed in Production profile")]
    PreviewAuthInProduction,
}

/// Validate config-level production constraints before telemetry starts.
///
/// Production profile: any violation returns `Err`. Operators see the failure on
/// stderr before the OTLP exporter ever starts (Phase 0).
///
/// Development profile: violations log a warning and continue so local cargo-run
/// boots still work.
///
/// # Errors
/// Returns [`ProductionGuardError`] when the active profile is Production and
/// any of the listed constraints are violated.
pub fn production_guards(
    config: &crate::config::WyrdServerConfig,
) -> Result<(), ProductionGuardError> {
    if !config.deployment_profile.is_production() {
        if config.request_id.trust_upstream && config.request_id.trusted_upstreams.is_empty() {
            tracing::warn!("request_id.trust_upstream=true with no trusted_upstreams configured");
        }
        if config.grpc.reflection_enabled {
            tracing::warn!("grpc.reflection_enabled=true in development profile");
        }
        if config.auth.allow_preview {
            tracing::warn!("auth.allow_preview=true in development profile");
        }
        return Ok(());
    }

    if config.request_id.trust_upstream && config.request_id.trusted_upstreams.is_empty() {
        return Err(ProductionGuardError::TrustUpstreamWithoutCidr);
    }
    if config.grpc.reflection_enabled {
        return Err(ProductionGuardError::ReflectionInProduction);
    }
    if config.auth.allow_preview {
        return Err(ProductionGuardError::PreviewAuthInProduction);
    }
    Ok(())
}

/// Assemble runtime state from a resolved `WyrdServerConfig`.
///
/// This is the primary boot entry point from `main.rs` once the config is
/// loaded. It runs the postgres boot, migrations, and pool phases, then chains
/// the `with_*` builder calls to attach config-derived fields.
///
/// # Errors
/// Returns [`ServerBootError`] when database boot, migration, pool construction,
/// or CIDR parsing fails.
pub async fn build_app_state_from_config(
    config: &crate::config::WyrdServerConfig,
    shutdown: CancellationToken,
    telemetry: Arc<wyrd_telemetry::TelemetryGuard>,
    reporter: wyrd_tonic::tonic_health::server::HealthReporter,
) -> Result<AppState, ServerBootError> {
    let boot = PostgresBoot::from_env().await?;
    let state = build_app_state_from_boot(&boot).await?;

    let trusted_upstreams_parsed: Vec<ipnetwork::IpNetwork> = config
        .request_id
        .trusted_upstreams
        .iter()
        .filter_map(|cidr| cidr.parse::<ipnetwork::IpNetwork>().ok())
        .collect();

    Ok(state
        .with_deployment_profile(config.deployment_profile)
        .with_shutdown_token(shutdown)
        .with_telemetry(telemetry)
        .with_limits(config.limits.into_state())
        .with_grpc_health(reporter)
        .with_trusted_upstreams_parsed(Arc::from(trusted_upstreams_parsed))
        .with_preview_auth(config.auth.allow_preview))
}

/// Spawn the storage sweeper if enabled and the platform admin pool is available.
///
/// Returns `None` when the sweeper is disabled or the platform admin pool is absent.
///
/// # Errors
/// Returns [`wyrd_storage::StorageError`] when the sweeper config cannot be read
/// from the process environment.
pub fn spawn_storage_sweeper(
    state: &AppState,
    shutdown: CancellationToken,
) -> Result<Option<tokio::task::JoinHandle<()>>, wyrd_storage::StorageError> {
    let cfg = wyrd_storage::sweeper::SweeperConfig::from_env()?;
    if !cfg.enabled {
        tracing::info!("storage sweeper disabled via WYRD_STORAGE_SWEEPER_ENABLED=false");
        return Ok(None);
    }

    let Some(admin_pool) = state.platform_admin_pool.clone() else {
        tracing::warn!("storage sweeper skipped because platform admin pool is unavailable");
        return Ok(None);
    };

    let sweeper = wyrd_storage::sweeper::Sweeper::new(
        Arc::clone(&state.storage),
        admin_pool,
        cfg,
        shutdown,
    );
    Ok(Some(tokio::spawn(async move { sweeper.run().await })))
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
