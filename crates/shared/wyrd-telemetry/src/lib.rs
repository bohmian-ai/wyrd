//! Telemetry setup shell.

#![deny(missing_docs)]

use tracing_subscriber::EnvFilter;
use wyrd_spec::error::WyrdError;

/// Telemetry setup config.
#[derive(Debug, Clone, PartialEq, serde::Deserialize)]
#[serde(deny_unknown_fields)]
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
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OtlpProtocol {
    /// OTLP over gRPC.
    #[default]
    Grpc,
    /// OTLP over HTTP/protobuf.
    HttpProtobuf,
}

/// Guard returned by [`init`] and [`init_test_only_no_global`].
///
/// Drop (or call [`TelemetryGuard::shutdown`]) to cleanly flush and shut down
/// any wired OTLP exporters before the process exits.
#[must_use = "TelemetryGuard must outlive the process and be dropped on shutdown"]
#[derive(Debug)]
pub struct TelemetryGuard {
    config: TelemetryConfig,
    #[cfg(feature = "otlp")]
    tracer_provider: Option<opentelemetry_sdk::trace::TracerProvider>,
}

impl TelemetryGuard {
    /// Borrow the configuration used to initialize telemetry.
    #[must_use]
    pub const fn config(&self) -> &TelemetryConfig {
        &self.config
    }

    /// Flush pending telemetry exports.
    ///
    /// When the `otlp` feature is enabled and an exporter is wired, this
    /// blocks until all buffered spans are flushed. Otherwise it is a no-op.
    pub fn force_flush(&self) {
        #[cfg(feature = "otlp")]
        if let Some(provider) = &self.tracer_provider {
            for result in provider.force_flush() {
                if let Err(e) = result {
                    tracing::warn!(error = %e, "OTLP force_flush error");
                }
            }
        }
    }

    /// Shut down telemetry exports.
    ///
    /// When the `otlp` feature is enabled and an exporter is wired, this
    /// flushes and shuts down the OTLP tracer provider. Otherwise it is a
    /// no-op.
    pub fn shutdown(self) {
        #[cfg(feature = "otlp")]
        if let Some(provider) = self.tracer_provider
            && let Err(e) = provider.shutdown()
        {
            tracing::warn!(error = %e, "OTLP shutdown error");
        }
    }
}

/// Resolve the effective tracing filter from multiple sources in priority order.
///
/// Priority: `WYRD_LOG` env var → `RUST_LOG` env var → `config.filter` field → `"info"`.
pub fn resolve_filter(config: &TelemetryConfig) -> EnvFilter {
    if let Ok(val) = std::env::var("WYRD_LOG")
        && !val.is_empty()
    {
        return EnvFilter::new(val);
    }
    if let Ok(val) = std::env::var("RUST_LOG")
        && !val.is_empty()
    {
        return EnvFilter::new(val);
    }
    EnvFilter::new(&config.filter)
}

/// Initialize a tracing subscriber without setting it as the global default.
///
/// Intended for use in tests that cannot tolerate an already-set global
/// subscriber. Returns a [`TelemetryGuard`]; when the guard is dropped, no
/// flush is needed because no global subscriber was installed.
pub fn init_test_only_no_global(config: TelemetryConfig) -> TelemetryGuard {
    TelemetryGuard {
        config,
        #[cfg(feature = "otlp")]
        tracer_provider: None,
    }
}

/// Initialize a tracing subscriber for server processes.
///
/// When the `otlp` feature is compiled in **and** `config.endpoint` is set,
/// a layered subscriber with an OTLP span exporter is installed. Otherwise a
/// plain `fmt` subscriber is installed.
///
/// # Errors
/// Returns an error if a global subscriber was already installed, or if the
/// OTLP exporter fails to build.
pub fn init(config: TelemetryConfig) -> Result<TelemetryGuard, WyrdError> {
    let filter = resolve_filter(&config);

    #[cfg(feature = "otlp")]
    if config.endpoint.is_some() {
        return init_with_otlp(config, filter);
    }

    let subscriber = tracing_subscriber::fmt().with_env_filter(filter).finish();
    tracing::subscriber::set_global_default(subscriber).map_err(|_| WyrdError::Conflict {
        message: "global tracing subscriber already set".to_string(),
        details: serde_json::json!({ "component": "telemetry" }),
    })?;
    Ok(TelemetryGuard {
        config,
        #[cfg(feature = "otlp")]
        tracer_provider: None,
    })
}

/// Return whether OTLP exporter support was compiled into this crate.
#[must_use]
pub const fn otlp_compiled() -> bool {
    cfg!(feature = "otlp")
}

/// Wire an OTLP span exporter and install a layered global subscriber.
#[cfg(feature = "otlp")]
fn init_with_otlp(config: TelemetryConfig, filter: EnvFilter) -> Result<TelemetryGuard, WyrdError> {
    use opentelemetry::trace::TracerProvider as _;
    use opentelemetry_otlp::WithExportConfig;
    use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

    let endpoint = config
        .endpoint
        .as_deref()
        .expect("caller guarantees endpoint is Some");
    let timeout = std::time::Duration::from_millis(config.export_timeout_ms.unwrap_or(30_000));
    let service_name = config.service_name.as_deref().unwrap_or("wyrd").to_string();
    let instance_id = ulid::Ulid::new().to_string();

    let resource = opentelemetry_sdk::Resource::new(vec![
        opentelemetry::KeyValue::new("service.name", service_name),
        opentelemetry::KeyValue::new("service.instance.id", instance_id),
    ]);

    let sampler = match config.sample_ratio {
        Some(ratio) => opentelemetry_sdk::trace::Sampler::TraceIdRatioBased(ratio),
        None => opentelemetry_sdk::trace::Sampler::AlwaysOn,
    };

    let exporter = match config.protocol {
        OtlpProtocol::Grpc => opentelemetry_otlp::SpanExporter::builder()
            .with_tonic()
            .with_endpoint(endpoint)
            .with_timeout(timeout)
            .build()
            .map_err(|e| WyrdError::Internal {
                message: format!("OTLP gRPC exporter build failed: {e}"),
                details: serde_json::json!({ "component": "telemetry" }),
            })?,
        OtlpProtocol::HttpProtobuf => {
            return Err(WyrdError::Internal {
                message: "OtlpProtocol::HttpProtobuf requires the http-proto feature in \
                          opentelemetry-otlp; recompile with that feature enabled"
                    .to_string(),
                details: serde_json::json!({
                    "component": "telemetry",
                    "protocol": "http_protobuf"
                }),
            });
        }
    };

    let provider = opentelemetry_sdk::trace::TracerProvider::builder()
        .with_batch_exporter(exporter, opentelemetry_sdk::runtime::Tokio)
        .with_sampler(sampler)
        .with_resource(resource)
        .build();

    let tracer = provider.tracer("wyrd");
    let otel_layer = tracing_opentelemetry::layer().with_tracer(tracer);

    tracing_subscriber::registry()
        .with(filter)
        .with(tracing_subscriber::fmt::layer())
        .with(otel_layer)
        .try_init()
        .map_err(|_| WyrdError::Conflict {
            message: "global tracing subscriber already set".to_string(),
            details: serde_json::json!({ "component": "telemetry" }),
        })?;

    Ok(TelemetryGuard {
        config,
        tracer_provider: Some(provider),
    })
}
