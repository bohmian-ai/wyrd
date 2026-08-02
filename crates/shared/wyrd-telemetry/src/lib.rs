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
    tracer_provider: Option<opentelemetry_sdk::trace::TracerProvider>,
}

/// Read-only representation of one production-pipeline span captured in tests.
#[cfg(feature = "test-support")]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CapturedSpan {
    /// Exact instrumentation span name.
    pub name: String,
    /// Scrubbed production attributes indexed by their field names.
    pub attributes: BTreeMap<String, String>,
    /// Span duration measured by the configured provider.
    pub duration_nanos: u64,
}

/// Read-only handle for spans exported through the test runtime's production
/// tracer-provider construction.
#[cfg(feature = "test-support")]
#[derive(Clone, Debug)]
pub struct TestTraceCapture {
    /// Same-provider exporter storage retained outside production owner code.
    spans: Arc<Mutex<Vec<opentelemetry_sdk::export::trace::SpanData>>>,
}

/// Minimal in-memory exporter used without enabling the SDK's broad testing feature.
#[cfg(feature = "test-support")]
#[derive(Clone, Debug, Default)]
struct TestSpanExporter {
    /// Finished spans emitted by the production-shaped tracer provider.
    spans: Arc<Mutex<Vec<opentelemetry_sdk::export::trace::SpanData>>>,
}

#[cfg(feature = "test-support")]
impl opentelemetry_sdk::export::trace::SpanExporter for TestSpanExporter {
    /// Append one provider-owned export batch to the shared capture.
    fn export(
        &mut self,
        mut batch: Vec<opentelemetry_sdk::export::trace::SpanData>,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<Output = opentelemetry_sdk::export::trace::ExportResult>
                + Send
                + 'static,
        >,
    > {
        let result = self
            .spans
            .lock()
            .map(|mut spans| spans.append(&mut batch))
            .map_err(opentelemetry::trace::TraceError::from);
        Box::pin(std::future::ready(result))
    }
}

#[cfg(feature = "test-support")]
impl TestTraceCapture {
    /// Return the current finished-span position for a later delta.
    #[must_use]
    pub fn checkpoint(&self) -> usize {
        self.spans.lock().map_or(0, |spans| spans.len())
    }

    /// Return all spans finished through the test runtime's installed provider.
    ///
    /// # Errors
    ///
    /// Returns the exporter lock error when a concurrent export prevents a
    /// consistent read.
    pub fn finished_spans(
        &self,
    ) -> opentelemetry::trace::TraceResult<Vec<opentelemetry_sdk::export::trace::SpanData>> {
        self.spans
            .lock()
            .map(|spans| spans.clone())
            .map_err(opentelemetry::trace::TraceError::from)
    }

    /// Return spans finished at or after one checkpoint.
    #[must_use]
    pub fn finished_since(&self, checkpoint: usize) -> Vec<CapturedSpan> {
        self.finished_spans().map_or_else(
            |_| Vec::new(),
            |spans| {
                spans
                    .into_iter()
                    .skip(checkpoint)
                    .map(|span| CapturedSpan {
                        name: span.name.to_string(),
                        attributes: span
                            .attributes
                            .into_iter()
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
                    })
                    .collect()
            },
        )
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
/// Returns an error if a global subscriber was already installed, or if the
/// OTLP exporter fails to build.
pub fn init(config: TelemetryConfig) -> Result<TelemetryGuard, WyrdError> {
    use opentelemetry::trace::TracerProvider as _;
    use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

    let filter = resolve_filter(&config);
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

    let (provider, tracer_provider) = if let Some(endpoint) = config.endpoint.as_deref() {
        let exporter = build_otlp_exporter(&config, endpoint)?;
        let p = opentelemetry_sdk::trace::TracerProvider::builder()
            .with_batch_exporter(exporter, opentelemetry_sdk::runtime::Tokio)
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

/// Install the normal Wyrd provider construction with an in-memory span
/// exporter for deterministic test and benchmark capture.
///
/// This intentionally changes only the exporter. Resource, sampling, global
/// subscriber, and tracing-layer composition remain the production shape.
///
/// # Errors
///
/// Returns a conflict when another global tracing subscriber is already
/// installed.
#[cfg(feature = "test-support")]
pub fn init_capture(
    config: TelemetryConfig,
) -> Result<(TelemetryGuard, TestTraceCapture), WyrdError> {
    use opentelemetry::trace::TracerProvider as _;
    use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

    let filter = resolve_filter(&config);
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
    let exporter = TestSpanExporter::default();
    let spans = Arc::clone(&exporter.spans);
    let provider = opentelemetry_sdk::trace::TracerProvider::builder()
        .with_simple_exporter(exporter.clone())
        .with_sampler(sampler)
        .with_resource(resource)
        .build();
    let tracer = provider.tracer("wyrd");
    tracing_subscriber::registry()
        .with(filter)
        .with(tracing_subscriber::fmt::layer())
        .with(tracing_opentelemetry::layer().with_tracer(tracer))
        .try_init()
        .map_err(|_| WyrdError::Conflict {
            message: "global tracing subscriber already set".to_string(),
            details: serde_json::json!({ "component": "telemetry" }),
        })?;
    Ok((
        TelemetryGuard {
            config,
            tracer_provider: Some(provider),
        },
        TestTraceCapture { spans },
    ))
}

fn build_otlp_exporter(
    config: &TelemetryConfig,
    endpoint: &str,
) -> Result<opentelemetry_otlp::SpanExporter, WyrdError> {
    use opentelemetry_otlp::WithExportConfig;

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
