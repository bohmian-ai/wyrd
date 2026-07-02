use std::sync::Arc;
use std::time::Duration;

use clap::{Parser, Subcommand};
use secrecy::ExposeSecret;
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};
use wyrd_tonic::tonic_health::server::health_reporter;

use wyrd_server::boot::production_guards;
use wyrd_server::bootstrap::bootstrap_admin_key;
use wyrd_server::grpc::{
    GrpcError, GrpcRouterConfig, build_app_grpc, drive_health_status, publish_initial_health,
    serve_grpc,
};
use wyrd_server::health::readiness_loop;
use wyrd_server::shutdown::{await_drain, signal_watcher};
use wyrd_server::{
    WyrdServerConfig, build_app_state_from_config, build_router, spawn_storage_sweeper,
};
use wyrd_spec::TenantSlug;
use wyrd_sql::pool::build_app_pool;
use wyrd_sql::postgres_boot::PostgresBoot;
use wyrd_telemetry::{TelemetryGuard, init as init_telemetry};

const EX_CONFIG: i32 = 78;
const EX_SOFTWARE: i32 = 70;

/// Wyrd control-plane server.
#[derive(Debug, Parser)]
#[command(name = "wyrd-server", version, about = "Wyrd control-plane server")]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

/// Operator subcommands. The absent arm runs the server.
#[derive(Debug, Subcommand)]
enum Command {
    /// Mint the first admin API key for a fresh deployment.
    BootstrapKey {
        /// Tenant slug to bootstrap.
        #[arg(long)]
        tenant: String,
    },
}

#[tokio::main]
async fn main() {
    // Parse the CLI before any telemetry/serve init so the serve path on the
    // `None` arm stays byte-for-byte today's `run()`.
    let cli = Cli::parse();
    let result = match cli.command {
        None => run().await,
        Some(Command::BootstrapKey { tenant }) => bootstrap_key(&tenant).await,
    };

    let exit_code = match result {
        Ok(()) => 0,
        Err(BootExit::Config(err)) => {
            eprintln!("wyrd-server: config error: {err}");
            EX_CONFIG
        }
        Err(BootExit::Other(err)) => {
            eprintln!("wyrd-server: fatal error: {err}");
            EX_SOFTWARE
        }
    };
    std::process::exit(exit_code);
}

/// Mint and print the first admin API key for `tenant`.
///
/// Builds only the runtime `wyrd_app` pool — never telemetry, storage,
/// listeners, or `AppState` — runs the issuance chain in one transaction, then
/// prints the plaintext key once to stdout.
async fn bootstrap_key(tenant: &str) -> Result<(), BootExit> {
    let _config = WyrdServerConfig::load().map_err(|e| BootExit::Config(Box::new(e)))?;
    let slug = TenantSlug::new(tenant).map_err(|e| BootExit::Config(Box::new(e)))?;

    let boot = PostgresBoot::from_env()
        .await
        .map_err(|e| BootExit::Other(Box::new(e)))?;
    let dsns = boot.dsns().map_err(|e| BootExit::Other(Box::new(e)))?;
    let pool = build_app_pool(dsns.app.expose_secret())
        .await
        .map_err(|e| BootExit::Other(Box::new(e)))?;

    let key = bootstrap_admin_key(&pool, &slug)
        .await
        .map_err(|e| BootExit::Other(Box::new(e)))?;

    println!("{}", key.expose());
    Ok(())
}

enum BootExit {
    Config(Box<dyn std::error::Error + Send + Sync>),
    Other(Box<dyn std::error::Error + Send + Sync>),
}

async fn run() -> Result<(), BootExit> {
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

    // health_reporter() returns the reporter/service pair exactly once; the
    // reporter is threaded onto AppState while the service is mounted on the
    // gRPC router below.
    let (mut reporter, health_service) = health_reporter();

    let state = build_app_state_from_config(
        &config,
        shutdown.clone(),
        telemetry.clone(),
        reporter.clone(),
    )
    .await
    .map_err(|e| BootExit::Other(Box::new(e)))?;

    state
        .production_validate()
        .map_err(|e| BootExit::Config(Box::new(e)))?;

    let http_router = build_router(state.clone());

    // build_app_grpc mounts health (unauthenticated) plus the C1 ingest service
    // with auth completed in the handler. It hard-errors when no token verifier
    // is configured, so ingest is never exposed unauthenticated.
    let grpc_router = build_app_grpc(
        &state,
        health_service,
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

    // Seed gRPC health from the cached readiness snapshot before binding and
    // before drive_health_status spawns; otherwise the first probe could land
    // on the default-NOT_SERVING reporter state.
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

    let signal_handle = tokio::spawn(signal_watcher(shutdown.clone()));

    let http_serve = {
        let token = shutdown.clone();
        async move {
            axum::serve(
                listener,
                http_router.into_make_service_with_connect_info::<std::net::SocketAddr>(),
            )
            .with_graceful_shutdown(async move { token.cancelled().await })
            .await
        }
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
                // Worker exited cooperatively after the shutdown signal — not an error.
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

    // Drain HTTP if not already finished
    if http_result.is_none() {
        let result = (&mut http_serve).await;
        if let Err(ref error) = result {
            warn!(error = %error, "axum::serve returned error during drain");
        }
        http_result = Some(result);
    }

    let drain = Duration::from_millis(config.shutdown.drain_ms);
    await_drain(
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
