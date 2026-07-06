//! Application composition and process lifecycle.

use std::sync::Arc;

use tracing::info;
use wyrd_telemetry::{TelemetryGuard, init as init_telemetry};

use crate::boot::{StateOverrides, build_state, production_guards};
use crate::config::{ServeMode, WyrdServerConfig};

pub mod metrics;
pub mod serve;
pub mod server;
pub mod shutdown;
pub mod supervise;

pub use serve::serve;
pub use server::WyrdServer;

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
    enterprise_on_start();

    let config = WyrdServerConfig::load().map_err(|e| BootExit::Config(Box::new(e)))?;
    production_guards(&config);

    let telemetry: Arc<TelemetryGuard> = Arc::new(
        init_telemetry(config.telemetry.clone()).map_err(|e| BootExit::Other(Box::new(e)))?,
    );
    info!(
        service.name = config.telemetry.service_name.as_deref().unwrap_or("wyrd-server"),
        "wyrd-server starting"
    );

    let mode = mode.unwrap_or(config.serve.mode);

    let state = build_state(&config, telemetry.clone(), StateOverrides::default())
        .await
        .map_err(|e| BootExit::Other(Box::new(e)))?;

    let result = WyrdServer::new(config, state)
        .map_err(|e| BootExit::Other(Box::new(e)))?
        .serve(mode)
        .await;

    drop(telemetry); // flush OTLP exporters after serving stops
    result
}

#[cfg(feature = "enterprise")]
fn enterprise_on_start() {
    wyrd_enterprise::on_server_start();
}

#[cfg(not(feature = "enterprise"))]
fn enterprise_on_start() {}
