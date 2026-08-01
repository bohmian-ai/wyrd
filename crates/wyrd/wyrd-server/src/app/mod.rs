//! Application composition and process lifecycle.

use std::sync::Arc;
use std::time::Duration;

use tokio::net::TcpListener;
use tokio::task::JoinSet;
use tracing::info;
use wyrd_telemetry::{TelemetryGuard, init as init_telemetry};

use crate::app::metrics::{install_recorder, metrics_router, serve_metrics};
use crate::app::supervise::{TaskExit, TaskId, fallible_task, supervise, worker_task};
use crate::boot::{StateOverrides, build_state, production_guards, spawn_forge_worker};
use crate::config::{ForgeProcessRole, ServeMode, WyrdServerConfig};
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

    let telemetry: Arc<TelemetryGuard> = Arc::new(
        init_telemetry(config.telemetry.clone()).map_err(|e| BootExit::Other(Box::new(e)))?,
    );
    let metrics_handle = if config.metrics.enabled {
        Some(install_recorder().map_err(|e| BootExit::Other(Box::new(e)))?)
    } else {
        None
    };
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

    let result = if config.role == ForgeProcessRole::ForgeWorker {
        run_forge_worker_process(&config, state, metrics_handle).await
    } else {
        WyrdServer::new_with_metrics_handle(config, state, metrics_handle)
            .map_err(|e| BootExit::Other(Box::new(e)))?
            .serve(mode)
            .await
    };

    drop(telemetry); // flush OTLP exporters after serving stops
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
    let shutdown = state.shutdown_token.clone();
    let mut set: JoinSet<TaskExit> = JoinSet::new();
    let worker = spawn_forge_worker(&state, shutdown.clone(), config.forge.worker_concurrency)
        .map_err(|error| BootExit::Other(Box::new(error)))?;
    set.spawn(fallible_task(TaskId::Worker("forge_worker"), worker));

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
