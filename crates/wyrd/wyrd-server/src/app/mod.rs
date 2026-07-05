//! Application composition and process lifecycle.

use std::sync::Arc;
use std::time::Duration;

use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};
use wyrd_telemetry::{TelemetryGuard, init as init_telemetry};
use wyrd_tonic::tonic_health::server::health_reporter;

use crate::boot::{production_guards, spawn_storage_sweeper};
use crate::components::health::readiness_loop;
use crate::config::WyrdServerConfig;
use crate::grpc::{
    GrpcError, GrpcRouterConfig, NoopInterceptor, build_grpc_router, drive_health_status,
    publish_initial_health, serve_grpc,
};

pub mod build;
pub mod serve;
pub mod shutdown;

pub use build::build_app;
pub use serve::serve;

/// Error wrapper used by the server binary's top-level CLI dispatcher.
#[derive(Debug)]
pub enum BootExit {
    /// Configuration loading or production validation failed.
    Config(Box<dyn std::error::Error + Send + Sync>),
    /// Runtime boot or serving failed.
    Other(Box<dyn std::error::Error + Send + Sync>),
}

/// Build and run the Wyrd server process.
pub async fn run() -> Result<(), BootExit> {
    enterprise_on_start();

    let config = WyrdServerConfig::load().map_err(|e| BootExit::Config(Box::new(e)))?;

    production_guards(&config);

    let telemetry: Arc<TelemetryGuard> = Arc::new(
        init_telemetry(config.telemetry.clone()).map_err(|e| BootExit::Other(Box::new(e)))?,
    );
    info!(
        service.name = config
            .telemetry
            .service_name
            .as_deref()
            .unwrap_or("wyrd-server"),
        "wyrd-server starting"
    );

    let shutdown = CancellationToken::new();

    let (mut reporter, health_service) = health_reporter();

    let (http_router, state) = build_app(
        &config,
        shutdown.clone(),
        telemetry.clone(),
        reporter.clone(),
    )
    .await
    .map_err(|e| BootExit::Other(Box::new(e)))?;

    let grpc_router = build_grpc_router(
        health_service,
        NoopInterceptor,
        GrpcRouterConfig {
            reflection_enabled: config.grpc.reflection_enabled,
        },
    )
    .map_err(|e| BootExit::Other(Box::new(e)))?;

    let mut workers: tokio::task::JoinSet<()> = tokio::task::JoinSet::new();

    if let Some(sweeper) =
        spawn_storage_sweeper(&state, shutdown.clone()).map_err(|e| BootExit::Other(Box::new(e)))?
    {
        workers.spawn(async move {
            match sweeper.await {
                Ok(()) => {}
                Err(join_error) if join_error.is_panic() => {
                    std::panic::resume_unwind(join_error.into_panic());
                }
                Err(join_error) => {
                    panic!("storage sweeper task aborted unexpectedly: {join_error}");
                }
            }
        });
    }

    workers.spawn(readiness_loop(
        state.clone(),
        Duration::from_millis(config.readiness.tick_ms),
        Duration::from_millis(config.readiness.probe_timeout_ms),
        shutdown.clone(),
    ));

    publish_initial_health(&state.readiness, &mut reporter).await;

    workers.spawn(drive_health_status(
        state.readiness.clone(),
        reporter.clone(),
        shutdown.clone(),
    ));

    let http_addr = config.http.bind;
    let listener = TcpListener::bind(http_addr)
        .await
        .map_err(|e| BootExit::Other(Box::new(e)))?;
    info!(bind = %http_addr, "HTTP server listening");

    let grpc_handle = tokio::spawn({
        let token = shutdown.clone();
        let bind = config.grpc.bind;
        async move { serve_grpc(grpc_router, bind, token).await }
    });

    let signal_handle = tokio::spawn(shutdown::signal_watcher(shutdown.clone()));

    let http_serve = {
        let token = shutdown.clone();
        async move { serve(http_router, listener, token).await }
    };
    tokio::pin!(http_serve);

    let mut http_result: Option<Result<(), std::io::Error>> = None;
    let mut grpc_result: Option<Result<Result<(), GrpcError>, tokio::task::JoinError>> = None;
    let mut terminal_error: Option<Box<dyn std::error::Error + Send + Sync>> = None;

    tokio::select! {
        result = &mut http_serve => {
            if let Err(ref error) = result {
                warn!(error = %error, "axum::serve returned error");
            }
            http_result = Some(result);
            shutdown.cancel();
        }
        joined = grpc_handle => {
            match &joined {
                Ok(Ok(())) => {}
                Ok(Err(grpc_error)) => {
                    warn!(error = %grpc_error, "gRPC serve returned error");
                    let msg = format!("gRPC serve failed: {grpc_error}");
                    terminal_error.get_or_insert_with(|| {
                        Box::<dyn std::error::Error + Send + Sync>::from(msg)
                    });
                }
                Err(join_error) => {
                    warn!(error = %join_error, "gRPC serve task aborted");
                    let msg = format!("gRPC serve task aborted: {join_error}");
                    terminal_error.get_or_insert_with(|| {
                        Box::<dyn std::error::Error + Send + Sync>::from(msg)
                    });
                }
            }
            grpc_result = Some(joined);
            shutdown.cancel();
        }
        Some(joined) = workers.join_next() => {
            if shutdown.is_cancelled() {
            } else {
                let message = match &joined {
                    Ok(()) => {
                        warn!("background worker exited before shutdown signal; cancelling");
                        "background worker exited before shutdown signal".to_owned()
                    }
                    Err(error) => {
                        warn!(error = %error, "background worker terminated with error; cancelling");
                        format!("background worker terminated: {error}")
                    }
                };
                terminal_error.get_or_insert_with(|| {
                    Box::<dyn std::error::Error + Send + Sync>::from(message)
                });
            }
            shutdown.cancel();
        }
        _ = shutdown.cancelled() => {}
    }

    if http_result.is_none() {
        let result = (&mut http_serve).await;
        if let Err(ref error) = result {
            warn!(error = %error, "axum::serve returned error during drain");
        }
        http_result = Some(result);
    }

    let drain = Duration::from_millis(config.shutdown.drain_ms);
    shutdown::await_drain(
        grpc_result,
        workers,
        signal_handle,
        drain,
        &mut terminal_error,
    )
    .await;

    info!("wyrd-server shutdown complete");
    drop(telemetry);

    if let Some(error) = terminal_error {
        return Err(BootExit::Other(error));
    }
    http_result
        .unwrap_or(Ok(()))
        .map_err(|e| BootExit::Other(Box::new(e)))
}

#[cfg(feature = "enterprise")]
fn enterprise_on_start() {
    wyrd_enterprise::on_server_start();
}

#[cfg(not(feature = "enterprise"))]
fn enterprise_on_start() {}
