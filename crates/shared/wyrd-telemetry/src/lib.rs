//! Telemetry setup shell.

#![deny(missing_docs)]

#[cfg(feature = "test-support")]
use std::collections::BTreeMap;
#[cfg(feature = "test-support")]
use std::sync::{Arc, Mutex};

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
    tracer_provider: Option<opentelemetry_sdk::trace::SdkTracerProvider>,
}

/// Read-only representation of one finished production-pipeline span.
#[cfg(feature = "test-support")]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapturedSpan {
    /// W3C trace identifier from the production span context.
    pub trace_id: String,
    /// Exact instrumentation span name.
    pub name: String,
    /// Scrubbed span attributes keyed by their production field names.
    pub attributes: BTreeMap<String, String>,
    /// Measured wall-clock duration from the production span timestamps.
    pub duration_nanos: u64,
    /// Terminal OpenTelemetry status retained without collapsing unset and success.
    pub status: CapturedSpanStatus,
}

/// Closed terminal status of one captured OpenTelemetry span.
#[cfg(feature = "test-support")]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CapturedSpanStatus {
    /// The owner did not assign a terminal status.
    Unset,
    /// The owner explicitly marked the operation successful.
    Ok,
    /// The owner marked the operation failed, retaining its scrubbed description.
    Error(String),
}

/// Handle for inspecting spans exported by the production-shaped test pipeline.
#[cfg(feature = "test-support")]
#[derive(Debug, Clone, Default)]
pub struct TestTraceCapture {
    /// Shared finished-span storage populated by the in-memory exporter.
    spans: Arc<Mutex<Vec<opentelemetry_sdk::trace::SpanData>>>,
}

#[cfg(feature = "test-support")]
impl TestTraceCapture {
    /// Return the number of spans finished before a workload begins.
    #[must_use]
    pub fn checkpoint(&self) -> usize {
        self.spans.lock().map_or(0, |spans| spans.len())
    }

    /// Return finished spans exported at or after `checkpoint`.
    ///
    /// A poisoned capture lock yields an empty snapshot so inspection cannot
    /// make application shutdown panic.
    #[must_use]
    pub fn finished_since(&self, checkpoint: usize) -> Vec<CapturedSpan> {
        self.spans.lock().map_or_else(
            |_| Vec::new(),
            |spans| {
                spans
                    .iter()
                    .skip(checkpoint)
                    .map(|span| CapturedSpan {
                        trace_id: span.span_context.trace_id().to_string(),
                        name: span.name.to_string(),
                        attributes: span
                            .attributes
                            .iter()
                            .map(|attribute| {
                                (
                                    attribute.key.as_str().to_owned(),
                                    attribute.value.to_string(),
                                )
                            })
                            .collect(),
                        duration_nanos: span
                            .end_time
                            .duration_since(span.start_time)
                            .map_or(0, |duration| {
                                u64::try_from(duration.as_nanos()).unwrap_or(u64::MAX)
                            }),
                        status: match &span.status {
                            opentelemetry::trace::Status::Unset => CapturedSpanStatus::Unset,
                            opentelemetry::trace::Status::Ok => CapturedSpanStatus::Ok,
                            opentelemetry::trace::Status::Error { description } => {
                                CapturedSpanStatus::Error(description.to_string())
                            }
                        },
                    })
                    .collect()
            },
        )
    }
}

/// In-memory exporter plugged into the same SDK provider used by production.
#[cfg(feature = "test-support")]
#[derive(Debug, Clone)]
struct CaptureExporter {
    /// Shared destination exposed through [`TestTraceCapture`].
    spans: Arc<Mutex<Vec<opentelemetry_sdk::trace::SpanData>>>,
}

#[cfg(feature = "test-support")]
impl opentelemetry_sdk::trace::SpanExporter for CaptureExporter {
    /// Append one completed SDK export batch without network IO.
    async fn export(
        &self,
        batch: Vec<opentelemetry_sdk::trace::SpanData>,
    ) -> opentelemetry_sdk::error::OTelSdkResult {
        let spans = Arc::clone(&self.spans);
        spans
            .lock()
            .map_err(|_| {
                opentelemetry_sdk::error::OTelSdkError::InternalFailure(
                    "trace capture lock poisoned".to_owned(),
                )
            })?
            .extend(batch);
        Ok(())
    }
}

impl TelemetryGuard {
    /// Borrow the configuration used to initialize telemetry.
    #[must_use]
    pub const fn config(&self) -> &TelemetryConfig {
        &self.config
    }

    /// Flush pending telemetry exports.
    ///
    /// When an OTLP exporter is wired, this blocks until all buffered spans are
    /// flushed. Otherwise it is a no-op.
    pub fn force_flush(&self) {
        if let Some(provider) = &self.tracer_provider
            && let Err(error) = provider.force_flush()
        {
            tracing::warn!(error = %error, "OTLP force_flush error");
        }
    }

    /// Shut down telemetry exports.
    ///
    /// When an OTLP exporter is wired, this flushes and shuts down the tracer
    /// provider. Otherwise it is a no-op.
    pub fn shutdown(self) {
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
        tracer_provider: None,
    }
}

/// Initialize a tracing subscriber for server processes.
///
/// When `config.endpoint` is set, a layered subscriber with an OTLP span
/// exporter is installed. Otherwise a plain `fmt` subscriber is installed with
/// a no-op OTel provider so the OTel pipeline is always wired.
///
/// # Errors
/// Returns an error if another Rustls provider already owns the process, a
/// global subscriber was already installed, or the OTLP exporter fails to
/// build.
pub fn init(config: TelemetryConfig) -> Result<TelemetryGuard, WyrdError> {
    use opentelemetry::trace::TracerProvider as _;
    use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

    let filter = resolve_filter(&config);
    let resource = telemetry_resource(&config);
    let sampler = telemetry_sampler(&config);

    let (provider, tracer_provider) = if let Some(endpoint) = config.endpoint.as_deref() {
        let exporter = build_otlp_exporter(&config, endpoint)?;
        let p = opentelemetry_sdk::trace::SdkTracerProvider::builder()
            .with_batch_exporter(exporter)
            .with_sampler(sampler)
            .with_resource(resource)
            .build();
        let tracer = p.tracer("wyrd");
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
        (Some(p), true)
    } else {
        let subscriber = tracing_subscriber::fmt().with_env_filter(filter).finish();
        tracing::subscriber::set_global_default(subscriber).map_err(|_| WyrdError::Conflict {
            message: "global tracing subscriber already set".to_string(),
            details: serde_json::json!({ "component": "telemetry" }),
        })?;
        (None, false)
    };

    let _ = tracer_provider;
    Ok(TelemetryGuard {
        config,
        tracer_provider: provider,
    })
}

/// Install a production-shaped OpenTelemetry provider with in-memory export.
///
/// This initializer uses the same resource, sampler, SDK provider, OTel layer,
/// filter, formatter, and global subscriber path as [`init`]. Only the exporter
/// destination differs, allowing journeys to inspect completed spans without a
/// second telemetry implementation.
///
/// # Errors
///
/// Returns [`WyrdError::Conflict`] if another global subscriber is installed.
#[cfg(feature = "test-support")]
pub fn init_test_capture(
    config: TelemetryConfig,
) -> Result<(TelemetryGuard, TestTraceCapture), WyrdError> {
    use opentelemetry::trace::TracerProvider as _;
    use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

    let capture = TestTraceCapture::default();
    let exporter = CaptureExporter {
        spans: Arc::clone(&capture.spans),
    };
    let provider = opentelemetry_sdk::trace::SdkTracerProvider::builder()
        .with_simple_exporter(exporter)
        .with_sampler(telemetry_sampler(&config))
        .with_resource(telemetry_resource(&config))
        .build();
    let tracer = provider.tracer("wyrd");
    tracing_subscriber::registry()
        .with(resolve_filter(&config))
        .with(tracing_subscriber::fmt::layer())
        .with(tracing_opentelemetry::layer().with_tracer(tracer))
        .try_init()
        .map_err(|_| WyrdError::Conflict {
            message: "global tracing subscriber already set".to_owned(),
            details: serde_json::json!({ "component": "telemetry" }),
        })?;
    Ok((
        TelemetryGuard {
            config,
            tracer_provider: Some(provider),
        },
        capture,
    ))
}

/// Install the production-shaped telemetry pipeline with in-memory capture.
///
/// This name is retained for Forge/Scribe callers while the Oracle test
/// harness uses [`init_test_capture`]. Both paths share one exporter and
/// provider implementation.
#[cfg(feature = "test-support")]
pub fn init_capture(
    config: TelemetryConfig,
) -> Result<(TelemetryGuard, TestTraceCapture), WyrdError> {
    init_test_capture(config)
}

/// Build the process resource shared by production and test exporters.
fn telemetry_resource(config: &TelemetryConfig) -> opentelemetry_sdk::Resource {
    let service_name = config.service_name.as_deref().unwrap_or("wyrd").to_owned();
    opentelemetry_sdk::Resource::builder_empty()
        .with_attributes([
            opentelemetry::KeyValue::new("service.name", service_name),
            opentelemetry::KeyValue::new("service.instance.id", ulid::Ulid::new().to_string()),
        ])
        .build()
}

/// Select the production sampler from the configured ratio.
fn telemetry_sampler(config: &TelemetryConfig) -> opentelemetry_sdk::trace::Sampler {
    config.sample_ratio.map_or(
        opentelemetry_sdk::trace::Sampler::AlwaysOn,
        opentelemetry_sdk::trace::Sampler::TraceIdRatioBased,
    )
}

/// Build the configured OTLP span exporter after claiming Wyrd's TLS provider.
///
/// # Errors
///
/// Returns [`WyrdError::Internal`] when another Rustls provider already owns
/// the process or the selected OTLP exporter rejects its configuration.
fn build_otlp_exporter(
    config: &TelemetryConfig,
    endpoint: &str,
) -> Result<opentelemetry_otlp::SpanExporter, WyrdError> {
    use opentelemetry_otlp::WithExportConfig;

    wyrd_tls::install_crypto_provider().map_err(|error| WyrdError::Internal {
        message: "telemetry TLS provider initialization failed".to_owned(),
        details: serde_json::json!({ "cause": error.to_string() }),
    })?;
    let timeout = std::time::Duration::from_millis(config.export_timeout_ms.unwrap_or(30_000));
    match config.protocol {
        OtlpProtocol::Grpc => opentelemetry_otlp::SpanExporter::builder()
            .with_tonic()
            .with_endpoint(endpoint)
            .with_timeout(timeout)
            .build()
            .map_err(|e| WyrdError::Internal {
                message: format!("OTLP gRPC exporter build failed: {e}"),
                details: serde_json::json!({ "component": "telemetry" }),
            }),
        OtlpProtocol::HttpProtobuf => Err(WyrdError::Internal {
            message: "OtlpProtocol::HttpProtobuf requires the http-proto feature in \
                      opentelemetry-otlp; recompile with that feature enabled"
                .to_string(),
            details: serde_json::json!({
                "component": "telemetry",
                "protocol": "http_protobuf"
            }),
        }),
    }
}

#[cfg(all(test, feature = "test-support"))]
mod tests {
    /// The upgraded SDK pipeline exports completed spans with preserved attributes.
    #[test]
    fn capture_pipeline_preserves_span_semantics() {
        let (guard, capture) = super::init_test_capture(super::TelemetryConfig::default())
            .expect("test telemetry pipeline initializes");
        let checkpoint = capture.checkpoint();
        tracing::info_span!("telemetry.compatibility", component = "wyrd").in_scope(|| {});
        guard.force_flush();
        let spans = capture.finished_since(checkpoint);
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].name, "telemetry.compatibility");
        assert_eq!(spans[0].status, super::CapturedSpanStatus::Unset);
        assert_eq!(
            spans[0].attributes.get("component"),
            Some(&"wyrd".to_owned())
        );
        guard.shutdown();
    }
}
