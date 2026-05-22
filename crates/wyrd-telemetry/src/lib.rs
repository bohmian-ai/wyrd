//! Telemetry setup shell.

#![deny(missing_docs)]

use tracing_subscriber::EnvFilter;
use wyrd_spec::error::WyrdError;

/// Telemetry setup config.
#[derive(Debug, Clone, PartialEq)]
pub struct TelemetryConfig {
    /// Env filter directive.
    pub filter: String,
    /// Optional OTLP endpoint URL.
    pub endpoint: Option<String>,
    /// Logical service name reported by exporters.
    pub service_name: Option<String>,
    /// OTLP transport protocol.
    pub protocol: OtlpProtocol,
    /// Optional sampling ratio in the `0.0..=1.0` range.
    pub sample_ratio: Option<f64>,
    /// Optional exporter timeout in milliseconds.
    pub export_timeout_ms: Option<u64>,
}

impl Default for TelemetryConfig {
    fn default() -> Self {
        Self {
            filter: "info".to_string(),
            endpoint: None,
            service_name: None,
            protocol: OtlpProtocol::Grpc,
            sample_ratio: None,
            export_timeout_ms: None,
        }
    }
}

/// OTLP exporter protocol selection.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum OtlpProtocol {
    /// OTLP over gRPC.
    #[default]
    Grpc,
    /// OTLP over HTTP/protobuf.
    HttpProtobuf,
}

/// Guard returned by [`init`].
#[must_use = "TelemetryGuard must outlive the process and be dropped on shutdown"]
#[derive(Debug)]
pub struct TelemetryGuard {
    config: TelemetryConfig,
}

impl TelemetryGuard {
    /// Borrow the configuration used to initialize telemetry.
    #[must_use]
    pub const fn config(&self) -> &TelemetryConfig {
        &self.config
    }

    /// Flush pending telemetry exports.
    ///
    /// Phase 1 has no exporter wiring, so this is a no-op.
    pub fn force_flush(&self) {}

    /// Shut down telemetry exports.
    ///
    /// Phase 1 has no exporter wiring, so this is a no-op.
    pub fn shutdown(self) {}
}

/// Initialize a tracing subscriber for server processes.
///
/// # Errors
/// Returns an error if a global subscriber was already installed.
pub fn init(config: TelemetryConfig) -> Result<TelemetryGuard, WyrdError> {
    let subscriber = tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::new(config.filter.as_str()))
        .finish();
    tracing::subscriber::set_global_default(subscriber).map_err(|_| WyrdError::Conflict {
        message: "global tracing subscriber already set".to_string(),
        details: serde_json::json!({ "component": "telemetry" }),
    })?;
    Ok(TelemetryGuard { config })
}

/// Return whether OTLP exporter support was compiled into this crate.
#[must_use]
pub const fn otlp_compiled() -> bool {
    cfg!(feature = "otlp")
}
