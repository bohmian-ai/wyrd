//! Application composition and process lifecycle.

use std::sync::Arc;
use std::time::Duration;

use tokio::net::TcpListener;
use tokio::task::JoinSet;
use tracing::info;
use wyrd_telemetry::TelemetryGuard;

use crate::app::metrics::{WyrdTelemetryRuntime, metrics_router, serve_metrics};
use crate::app::supervise::{TaskExit, TaskId, fallible_task, supervise, worker_task};
use crate::boot::{StateOverrides, build_state, production_guards, spawn_forge_worker};
use crate::config::{ServeMode, WyrdServerConfig};
use crate::state::AppState;

pub mod metrics;
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

    let state = build_state(&config, telemetry.clone(), StateOverrides::default())
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
            async move { health.wait_for_poison().await },
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
        assert!(
            boot.contains(".bifrost_node_id()"),
            "normal worker composition must obtain configured physical identity from AppState"
        );
        assert!(
            boot.contains("node_id.as_uuid()"),
            "normal ForgeWorker construction must use configured physical identity"
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

#[cfg(test)]
/// PostgreSQL-backed dedicated-worker lifecycle regressions.
mod pg_tests {
    use std::net::{SocketAddr, TcpListener as StdTcpListener};
    use std::process::Command;

    use super::*;
    use crate::config::BifrostTarget;

    /// Reserves an ephemeral loopback address long enough to learn a distinct
    /// port for one dedicated-runner listener assertion.
    fn available_loopback_addr() -> SocketAddr {
        let listener = StdTcpListener::bind("127.0.0.1:0")
            .expect("test loopback listener reserves an ephemeral port");
        listener
            .local_addr()
            .expect("reserved listener reports its local address")
    }

    /// The dedicated runner exposes metrics only, creates one bounded worker,
    /// and drains after the real process signal path without opening API ports.
    #[tokio::test]
    async fn dedicated_forge_worker_process_is_metrics_only_and_drains_bounded() {
        let (state, _publisher) = crate::boot::pg_tests::composed_test_state().await;
        let http_addr = available_loopback_addr();
        let grpc_addr = available_loopback_addr();
        let metrics_addr = available_loopback_addr();
        let config = WyrdServerConfig {
            role: BifrostTarget::ForgeWorker,
            http: crate::config::HttpConfig { bind: http_addr },
            grpc: crate::config::GrpcConfig {
                bind: grpc_addr,
                ..crate::config::GrpcConfig::default()
            },
            forge: crate::config::ForgeRuntimeConfig {
                worker_concurrency: 1,
                ..crate::config::ForgeRuntimeConfig::default()
            },
            metrics: crate::config::MetricsConfig {
                enabled: true,
                bind: Some(metrics_addr),
            },
            shutdown: crate::config::ShutdownConfig { drain_ms: 100 },
            ..WyrdServerConfig::default()
        };

        let runner = tokio::spawn(async move {
            run_forge_worker_process(&config, state, Some(metrics::test_prometheus_handle())).await
        });
        let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
        loop {
            if std::net::TcpStream::connect(metrics_addr).is_ok() {
                break;
            }
            if runner.is_finished() {
                let result = runner
                    .await
                    .expect("dedicated runner task reports its early exit");
                panic!("dedicated runner exited before metrics listener startup: {result:?}");
            }
            assert!(
                tokio::time::Instant::now() < deadline,
                "dedicated metrics listener did not start"
            );
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert!(
            StdTcpListener::bind(http_addr).is_ok(),
            "dedicated runner must not bind HTTP"
        );
        assert!(
            StdTcpListener::bind(grpc_addr).is_ok(),
            "dedicated runner must not bind gRPC"
        );

        let status = Command::new("kill")
            .args(["-TERM", &std::process::id().to_string()])
            .status()
            .expect("test process can deliver SIGTERM to its Tokio signal watcher");
        assert!(status.success(), "SIGTERM delivery must succeed");
        let result = tokio::time::timeout(Duration::from_secs(2), runner)
            .await
            .expect("dedicated runner must drain within its bounded shutdown window")
            .expect("dedicated runner task joins");
        assert!(
            result.is_ok(),
            "signal-driven drain must succeed: {result:?}"
        );
    }
}
