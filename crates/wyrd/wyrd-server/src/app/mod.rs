//! Application composition and process lifecycle.

use std::sync::Arc;
use std::time::Duration;

use tokio::net::TcpListener;
use tokio::task::JoinSet;
use tracing::info;
use wyrd_telemetry::TelemetryGuard;

use crate::app::metrics::{WyrdTelemetryRuntime, metrics_router, serve_metrics};
use crate::app::supervise::{TaskExit, TaskId, fallible_task, supervise, worker_task};
use crate::boot::{
    BootedServer, StateOverrides, build_state, production_guards, spawn_forge_worker,
};
use crate::config::{ServeMode, WyrdServerConfig};
use crate::state::AppState;

pub mod metrics;
pub mod peer_plane;
pub mod serve;
pub mod server;
pub mod shutdown;
pub mod supervise;

pub use serve::serve;
pub use server::{BoundServer, WyrdServer};

/// Error wrapper used by the server binary's top-level CLI dispatcher.
#[derive(Debug)]
pub enum BootExit {
    /// Configuration loading or production validation failed.
    Config(Box<dyn std::error::Error + Send + Sync>),
    /// Runtime boot or serving failed.
    Other(Box<dyn std::error::Error + Send + Sync>),
}

/// Build and run the Wyrd server process for the given transport `mode`.
///
/// `mode` is `None` to use the configured default (`config.serve.mode`), or
/// `Some(_)` to override it (the binary passes the `--mode` flag through here).
pub async fn run(mode: Option<ServeMode>) -> Result<(), BootExit> {
    let config = WyrdServerConfig::load().map_err(|e| BootExit::Config(Box::new(e)))?;
    production_guards(&config);

    let telemetry_runtime = WyrdTelemetryRuntime::install(config.telemetry.clone())
        .map_err(|error| BootExit::Other(Box::new(error)))?;
    let telemetry: Arc<TelemetryGuard> = telemetry_runtime.guard();
    let metrics_handle = config
        .metrics
        .enabled
        .then(|| telemetry_runtime.prometheus());
    info!(
        service.name = config
            .telemetry
            .service_name
            .as_deref()
            .unwrap_or("wyrd-server"),
        "wyrd-server starting"
    );

    let mode = mode.unwrap_or(config.serve.mode);

    // `coordination_runtime` is the sole owner of the dedicated Scribe executor.
    // It is bound here, outside the serving future, so it outlives the Bifrost
    // drain that `BoundServer::run` performs and is released only once serving
    // has returned. Its drop is non-blocking, which is required because this
    // frame is inside `#[tokio::main]`.
    let BootedServer {
        state,
        coordination_runtime,
    } = build_state(&config, telemetry.clone(), StateOverrides::default())
        .await
        .map_err(|e| BootExit::Other(Box::new(e)))?;

    let result = if !config.role.serves_api() {
        run_forge_worker_process(&config, state, metrics_handle).await
    } else {
        let node_id = state.bifrost.node_id().ok_or_else(|| {
            BootExit::Other("configured Bifrost node identity is unavailable".into())
        })?;
        let _role_telemetry =
            metrics::ForgeRoleTelemetryGuard::started(config.role, node_id.as_uuid());
        WyrdServer::new_with_metrics_handle(config, state, metrics_handle)
            .map_err(|e| BootExit::Other(Box::new(e)))?
            .serve(mode)
            .await
    };

    drop(coordination_runtime); // release Scribe coordination threads after drain
    drop(telemetry); // flush OTLP exporters after serving stops
    drop(telemetry_runtime);
    result
}

/// Runs the dedicated executor topology without constructing Gate or API routers.
///
/// The only optional listener is the existing metrics endpoint. Forge task
/// execution and the signal watcher share the same bounded supervisor used by
/// the server topology.
///
/// # Errors
///
/// Returns listener, worker construction, worker execution, or supervision
/// failures through [`BootExit::Other`].
async fn run_forge_worker_process(
    config: &WyrdServerConfig,
    state: AppState,
    metrics_handle: Option<metrics_exporter_prometheus::PrometheusHandle>,
) -> Result<(), BootExit> {
    let node_id = state
        .bifrost
        .node_id()
        .ok_or_else(|| BootExit::Other("configured Bifrost node identity is unavailable".into()))?;
    let _role_telemetry = metrics::ForgeRoleTelemetryGuard::started(config.role, node_id.as_uuid());
    let shutdown = state.shutdown_token.clone();
    let mut set: JoinSet<TaskExit> = JoinSet::new();
    let worker = spawn_forge_worker(
        &state,
        shutdown.clone(),
        config.forge.worker_concurrency,
        config.forge.resolved_per_tenant_active_cap(),
    )
    .map_err(|error| BootExit::Other(Box::new(error)))?;
    set.spawn(fallible_task(TaskId::Worker("forge_worker"), worker));
    if let Some(health) = state.bifrost.resource_health() {
        set.spawn(fallible_task(
            TaskId::Worker("bifrost_resource_health"),
            // Poison is a terminal the supervisor reacts to, but a healthy
            // server never publishes one, so this wait must also end on a clean
            // shutdown. Without the cancellation arm the task can never join and
            // every shutdown burns the full drain deadline before aborting it.
            {
                let shutdown = shutdown.clone();
                async move {
                    tokio::select! {
                        poisoned = health.wait_for_poison() => poisoned,
                        () = shutdown.cancelled() => Ok(()),
                    }
                }
            },
        ));
    }

    if config.metrics.enabled {
        let bind = config
            .metrics
            .resolved_bind(config.http.bind)
            .ok_or_else(|| BootExit::Other("metrics bind port arithmetic overflow".into()))?;
        let listener = TcpListener::bind(bind).await.map_err(|error| {
            BootExit::Other(format!("metrics listener failed to bind {bind}: {error}").into())
        })?;
        let handle = metrics_handle
            .ok_or_else(|| BootExit::Other("metrics recorder handle is unavailable".into()))?;
        set.spawn(fallible_task(
            TaskId::Metrics,
            serve_metrics(metrics_router(handle), listener, shutdown.clone()),
        ));
    }
    set.spawn(worker_task(
        TaskId::Signal,
        shutdown::signal_watcher(shutdown.clone()),
    ));
    let terminal = supervise(
        set,
        shutdown,
        Duration::from_millis(config.shutdown.drain_ms),
    )
    .await;
    match terminal {
        Some(message) => Err(BootExit::Other(message.into())),
        None => Ok(()),
    }
}

/// Runs the production dedicated Forge-worker process over test-built state.
///
/// Test clusters use this feature-gated entry to exercise the same worker
/// construction, supervision, cancellation, and bounded drain as [`run`]
/// without loading process-global configuration or telemetry a second time.
/// No public HTTP or gRPC listener is constructed by this runner.
///
/// # Errors
///
/// Returns listener, worker construction, worker execution, or supervision
/// failures through [`BootExit::Other`].
#[cfg(feature = "test-support")]
pub async fn run_forge_worker_process_for_test(
    config: &WyrdServerConfig,
    state: AppState,
    metrics_handle: Option<metrics_exporter_prometheus::PrometheusHandle>,
) -> Result<(), BootExit> {
    run_forge_worker_process(config, state, metrics_handle).await
}

#[cfg(test)]
/// Static composition assertions that keep API and dedicated-worker ownership separate.
mod tests {
    /// The API lifecycle has no worker-only branch, while the dedicated runner
    /// has exactly one top-level worker construction path.
    #[test]
    fn process_role_composition_has_one_dedicated_worker_runner() {
        let dedicated_runner = include_str!("mod.rs");
        let production_runner = dedicated_runner
            .split("#[cfg(test)]")
            .next()
            .expect("application module has production source before tests");
        assert_eq!(
            production_runner
                .matches("async fn run_forge_worker_process(")
                .count(),
            1,
            "one dedicated worker-only runner must own process composition"
        );
        assert_eq!(
            production_runner.matches("spawn_forge_worker(").count(),
            1,
            "dedicated worker composition must create one shared worker graph"
        );
        let boot = include_str!("../boot/mod.rs");
        assert_eq!(
            boot.matches("ForgeWorker::new(").count(),
            1,
            "boot must construct the Forge worker exactly once"
        );
        assert!(
            boot.contains("node_id.as_uuid()"),
            "normal ForgeWorker construction must use the boot-resolved physical identity"
        );

        let server = include_str!("server.rs");
        let production_server = server
            .split("#[cfg(test)]")
            .next()
            .expect("server module has production source before tests");
        assert!(
            !production_server.contains("BifrostTarget::ForgeWorker"),
            "WyrdServer bind/run must not own a dedicated worker branch"
        );
        assert!(
            server.contains("self.config.role == BifrostTarget::All"),
            "the embedded worker must be composed only for the All role"
        );
        assert_eq!(
            server.matches("spawn_maintenance_scheduler(").count(),
            1,
            "API roles must retain one supervised Forge scheduler"
        );
        assert_eq!(
            server.matches("spawn_forge_worker(").count(),
            1,
            "All must compose exactly one embedded Forge worker"
        );
    }
}
