//! Telemetry setup shell.

#![deny(missing_docs)]

use tracing_subscriber::EnvFilter;

/// Telemetry setup config.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TelemetryConfig {
    /// Env filter directive.
    pub filter: String,
}

impl Default for TelemetryConfig {
    fn default() -> Self {
        Self {
            filter: "info".to_string(),
        }
    }
}

/// Initialize a tracing subscriber for server processes.
///
/// # Errors
/// Returns an error if a global subscriber was already installed.
pub fn init(config: &TelemetryConfig) -> Result<(), TelemetryError> {
    let subscriber = tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::new(config.filter.clone()))
        .finish();
    tracing::subscriber::set_global_default(subscriber).map_err(|_| TelemetryError::AlreadySet)
}

/// Telemetry setup errors.
#[derive(Debug, thiserror::Error)]
pub enum TelemetryError {
    /// A global subscriber already exists.
    #[error("global tracing subscriber already set")]
    AlreadySet,
}
