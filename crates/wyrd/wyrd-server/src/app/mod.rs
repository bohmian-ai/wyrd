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

    // `coordination_runtime` and `compaction_runtime` are the sole owners of
    // the dedicated Scribe and Forge executors. They are bound here, outside
    // the serving future, so they outlive both the Bifrost drain that
    // `BoundServer::run` performs and the supervised Forge worker, and are
    // released only once serving has returned. Their drops are non-blocking,
    // which is required because this frame is inside `#[tokio::main]`.
    let BootedServer {
        state,
        coordination_runtime,
        compaction_runtime,
    } = build_state(&config, telemetry.clone(), StateOverrides::default())
        .await
        .map_err(|e| BootExit::Other(Box::new(e)))?;

    let result = if !config.role.serves_api() {
        run_forge_worker_process(&config, state, metrics_handle).await
    } else {
        state.bifrost.node_id().ok_or_else(|| {
            BootExit::Other("configured Bifrost node identity is unavailable".into())
        })?;
        WyrdServer::new_with_metrics_handle(config, state, metrics_handle)
            .map_err(|e| BootExit::Other(Box::new(e)))?
            .serve(mode)
            .await
    };

    drop(compaction_runtime); // release Forge compaction threads after supervision drains
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
    state
        .bifrost
        .node_id()
        .ok_or_else(|| BootExit::Other("configured Bifrost node identity is unavailable".into()))?;
    let shutdown = state.shutdown_token.clone();
    let mut set: JoinSet<TaskExit> = JoinSet::new();
    let worker = spawn_forge_worker(&state, shutdown.clone())
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
    /// The compaction runtime outlives every path that supervises Forge work.
    ///
    /// The owner's drop detaches its threads instead of joining them, so a
    /// runner that has written output objects and is about to Prepare its
    /// operation would be abandoned mid-flight if the executor were released
    /// first. Ownership therefore sits outside the serving future in `run`, and
    /// the release must be sequenced strictly after serving — which is the
    /// frame that both the API topology and the dedicated worker runner drain
    /// their supervision inside.
    ///
    /// # Panics
    ///
    /// Panics when the owner is bound inside the serving future, released
    /// before serving returns, or released after the Scribe coordination
    /// runtime it must precede.
    #[test]
    fn forge_runtime_lives_until_worker_supervision_drains() {
        let production = include_str!("mod.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("application module has production source before tests");

        let bind = production
            .find("compaction_runtime,\n    } = build_state(")
            .expect("run binds the compaction runtime out of BootedServer");
        let serve = production
            .find("let result = if !config.role.serves_api()")
            .expect("run serves after binding its runtimes");
        let release = production
            .find("drop(compaction_runtime);")
            .expect("run releases the compaction runtime");
        let coordination = production
            .find("drop(coordination_runtime);")
            .expect("run releases the coordination runtime");

        assert!(
            bind < serve,
            "the executor must be owned outside the serving future, not inside it"
        );
        assert!(
            serve < release,
            "serving — which drains Forge supervision — must return before release"
        );
        assert!(
            release < coordination,
            "Forge threads release before the Scribe coordination threads they may still call"
        );
        assert!(
            production[serve..release].contains(".await"),
            "the release must follow an awaited serving frame, not a detached spawn"
        );
        assert_eq!(
            production.matches("drop(compaction_runtime);").count(),
            1,
            "exactly one release site keeps the ordering above auditable"
        );

        // Both supervised topologies drain inside that awaited frame.
        assert!(
            production.contains("run_forge_worker_process(&config, state, metrics_handle).await"),
            "the dedicated worker runner is awaited by the serving frame"
        );
        assert!(
            production.contains("let terminal = supervise("),
            "the dedicated runner drains its Forge supervision before returning"
        );

        // The harness that composes the same graph honours the same ordering by
        // owning the runtime for the whole server lifetime.
        let harness = include_str!("../../../wyrd-testing/src/server.rs");
        assert!(
            harness.contains("_compaction_runtime: wyrd_server::state::ForgeCompactionRuntime"),
            "the test harness must retain the executor for the server's lifetime"
        );
    }

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
