use std::net::SocketAddr;
use tokio::sync::watch;
use wyrd_server::AppState;
use wyrd_storage::sweeper::{Sweeper, SweeperConfig};

const PORT_ENV: &str = "WYRD_SERVER_PORT";
const DEFAULT_PORT: u16 = 8080;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    enterprise_on_start();

    let state = wyrd_server::build_app_state().await?;
    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let sweeper_handle = spawn_storage_sweeper(&state, shutdown_rx)?;
    let port: u16 = std::env::var(PORT_ENV)
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(DEFAULT_PORT);
    let app = wyrd_server::router(state);
    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal(shutdown_tx.clone()))
        .await?;
    let _ = shutdown_tx.send(true);
    if let Some(handle) = sweeper_handle {
        let _ = handle.await;
    }
    Ok(())
}

fn spawn_storage_sweeper(
    state: &AppState,
    shutdown: watch::Receiver<bool>,
) -> Result<Option<tokio::task::JoinHandle<()>>, wyrd_storage::StorageError> {
    let cfg = SweeperConfig::from_env()?;
    if !cfg.enabled {
        tracing::info!("storage sweeper disabled via WYRD_STORAGE_SWEEPER_ENABLED=false");
        return Ok(None);
    }

    let Some(admin_pool) = state.platform_admin_pool.clone() else {
        tracing::warn!("storage sweeper skipped because platform admin pool is unavailable");
        return Ok(None);
    };

    let sweeper = Sweeper::new(
        std::sync::Arc::clone(&state.storage),
        admin_pool,
        cfg,
        shutdown,
    );
    Ok(Some(tokio::spawn(async move { sweeper.run().await })))
}

async fn shutdown_signal(shutdown_tx: watch::Sender<bool>) {
    if let Err(error) = tokio::signal::ctrl_c().await {
        tracing::warn!(error = %error, "failed to listen for shutdown signal");
    }
    let _ = shutdown_tx.send(true);
}

#[cfg(feature = "enterprise")]
fn enterprise_on_start() {
    wyrd_enterprise::on_server_start();
}

#[cfg(not(feature = "enterprise"))]
fn enterprise_on_start() {}
