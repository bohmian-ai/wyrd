//! Telemetry setup shell.

#![deny(missing_docs)]

mod context;
mod export;

pub use context::{
    caller_sampled, continued_context, current_context, inbound_context, serve_linked_work,
    serve_request,
};
pub use export::{TenantSpanRoute, TenantSpanSink, bind_operation_route, bind_tenant_route};

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
    /// This span's own identifier, which its children name as their parent.
    pub span_id: String,
    /// Identifier of the enclosing span; the invalid all-zero id for a root.
    pub parent_span_id: String,
    /// Exact instrumentation span name.
    pub name: String,
    /// OpenTelemetry span kind, such as `Server` for a request span or
    /// `Internal` for work inside it.
    pub kind: String,
    /// Scrubbed span attributes keyed by their production field names.
    pub attributes: BTreeMap<String, String>,
    /// Measured wall-clock duration from the production span timestamps.
    pub duration_nanos: u64,
    /// Terminal OpenTelemetry status retained without collapsing unset and success.
    pub status: CapturedSpanStatus,
    /// Events recorded inside this span, in emission order.
    pub events: Vec<CapturedSpanEvent>,
    /// Spans this span links to, as `(trace_id, span_id)` hex pairs in link
    /// order; a link records causality without making the target a parent.
    pub links: Vec<(String, String)>,
}

/// One `tracing` event the production pipeline attached to a finished span.
#[cfg(feature = "test-support")]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapturedSpanEvent {
    /// Event message as exported by the OpenTelemetry layer.
    pub name: String,
    /// Scrubbed event fields, including the exported `level`.
    pub attributes: BTreeMap<String, String>,
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
                        span_id: span.span_context.span_id().to_string(),
                        parent_span_id: span.parent_span_id.to_string(),
                        name: span.name.to_string(),
                        kind: format!("{:?}", span.span_kind),
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
                        events: span
                            .events
                            .iter()
                            .map(|event| CapturedSpanEvent {
                                name: event.name.to_string(),
                                attributes: event
                                    .attributes
                                    .iter()
                                    .map(|attribute| {
                                        (
                                            attribute.key.as_str().to_owned(),
                                            attribute.value.to_string(),
                                        )
                                    })
                                    .collect(),
                            })
                            .collect(),
                        links: span
                            .links
                            .iter()
                            .map(|link| {
                                (
                                    link.span_context.trace_id().to_string(),
                                    link.span_context.span_id().to_string(),
                                )
                            })
                            .collect(),
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
    /// Append one completed SDK export batch without network IO; the append
    /// completes before the returned future is polled.
    fn export(
        &self,
        batch: Vec<opentelemetry_sdk::trace::SpanData>,
    ) -> impl std::future::Future<Output = opentelemetry_sdk::error::OTelSdkResult> + Send {
        let appended = self
            .spans
            .lock()
            .map(|mut spans| spans.extend(batch))
            .map_err(|_| {
                opentelemetry_sdk::error::OTelSdkError::InternalFailure(
                    "trace capture lock poisoned".to_owned(),
                )
            });
        std::future::ready(appended)
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

/// Link `span` to the remote span `span_id` of trace `trace_id`.
///
/// The link records that the remote span caused the work `span` measures
/// without adopting it as a parent, so `span` keeps its own trace. The remote
/// context carries whether its producer `sampled` it. A link added before
/// `span` is first entered is present when it starts, so a sampled one makes
/// `span` eligible for its tenant's copy. Without an OpenTelemetry layer in
/// the subscriber, or on a disabled span, this does nothing.
pub fn link_remote_span(
    span: &tracing::Span,
    trace_id: &wyrd_spec::vala::ids::TraceId,
    span_id: &wyrd_spec::vala::ids::SpanId,
    sampled: bool,
) {
    use opentelemetry::trace::{SpanContext, TraceFlags, TraceState};
    use tracing_opentelemetry::OpenTelemetrySpanExt as _;

    span.add_link(SpanContext::new(
        opentelemetry::trace::TraceId::from_bytes(*trace_id.as_bytes()),
        opentelemetry::trace::SpanId::from_bytes(*span_id.as_bytes()),
        if sampled {
            TraceFlags::SAMPLED
        } else {
            TraceFlags::default()
        },
        true,
        TraceState::default(),
    ));
}

/// Name of the span [`record_timing`] emits.
pub const TIMING_SPAN: &str = "wyrd.timing";

/// Records one request's server timing at an owning boundary as a closed
/// [`TIMING_SPAN`] child of `parent`.
///
/// `elapsed` is the owner's monotonic time from handler entry to `boundary`
/// (result ready, write acknowledged, or response handoff), and `excluded`
/// the union of the waits the owner does not count as its own work, such as
/// provider exchanges or Verifier blocks, when the boundary has any. Both are
/// exact integer nanoseconds, `elapsed_ns` and `excluded_ns`, with the
/// closed `boundary` and `outcome` labels. The span reaches the operator's
/// OTLP export like any other sampled span, so a collector reads every
/// request's observation before any metrics histogram aggregates it; its own
/// start and end times carry no measurement. The nanoseconds are recorded
/// as `i64`, which the OpenTelemetry layer exports as an OTLP integer (it
/// would export a `u64` field as its debug string); a duration beyond
/// `i64::MAX` nanoseconds saturates. Without an OpenTelemetry layer, or when the
/// filter disables the span, this does nothing.
pub fn record_timing(
    parent: &tracing::Span,
    boundary: &'static str,
    outcome: &'static str,
    elapsed: std::time::Duration,
    excluded: Option<std::time::Duration>,
) {
    let nanos =
        |duration: std::time::Duration| i64::try_from(duration.as_nanos()).unwrap_or(i64::MAX);
    let span = tracing::info_span!(
        parent: parent,
        "wyrd.timing",
        boundary,
        outcome,
        elapsed_ns = nanos(elapsed),
        excluded_ns = tracing::field::Empty,
    );
    if let Some(excluded) = excluded {
        span.record("excluded_ns", nanos(excluded));
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
/// Installs one global subscriber: the configured filter, formatted log
/// output, and an OpenTelemetry layer over the process tracer provider. The
/// provider is always built, so tenant span export works with or without an
/// operator destination. When `config.endpoint` is set, the spans the server
/// sampler samples are also batched to that OTLP endpoint.
///
/// # Errors
/// Returns an error if another Rustls provider already owns the process, a
/// global subscriber was already installed, or the OTLP exporter fails to
/// build.
pub fn init(config: TelemetryConfig) -> Result<TelemetryGuard, WyrdError> {
    use opentelemetry::trace::TracerProvider as _;
    use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

    let operator = config
        .endpoint
        .as_deref()
        .map(|endpoint| build_otlp_exporter(&config, endpoint))
        .transpose()?
        .map(|exporter| {
            export::OperatorExport(
                opentelemetry_sdk::trace::BatchSpanProcessor::builder(exporter).build(),
            )
        });
    let provider = export::tracer_provider(
        telemetry_sampler(&config),
        telemetry_resource(&config),
        operator,
    );
    tracing_subscriber::registry()
        .with(resolve_filter(&config))
        .with(tracing_subscriber::fmt::layer())
        .with(tracing_opentelemetry::layer().with_tracer(provider.tracer("wyrd")))
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

/// Install a production-shaped OpenTelemetry provider with in-memory export.
///
/// This initializer uses the same resource, sampler, SDK provider, tenant
/// export, OTel layer, filter, formatter, and global subscriber path as
/// [`init`]. The operator's exporter destination differs, allowing journeys to inspect completed spans without a
/// second telemetry implementation, and every captured span also carries its
/// `tracing` level as the `level` attribute.
///
/// Capture is unfiltered: every span and event the code emits reaches the
/// exporter, so what a test can assert on never depends on `WYRD_LOG`. Only
/// the formatted log output is filtered, by [`resolve_filter`].
///
/// # Errors
///
/// Returns [`WyrdError::Conflict`] if another global subscriber is installed.
#[cfg(feature = "test-support")]
pub fn init_test_capture(
    config: TelemetryConfig,
) -> Result<(TelemetryGuard, TestTraceCapture), WyrdError> {
    use opentelemetry::trace::TracerProvider as _;
    use tracing_subscriber::{Layer as _, layer::SubscriberExt, util::SubscriberInitExt};

    let capture = TestTraceCapture::default();
    let exporter = CaptureExporter {
        spans: Arc::clone(&capture.spans),
    };
    let provider = export::tracer_provider(
        telemetry_sampler(&config),
        telemetry_resource(&config),
        Some(export::OperatorExport(
            opentelemetry_sdk::trace::SimpleSpanProcessor::new(exporter),
        )),
    );
    let tracer = provider.tracer("wyrd");
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer().with_filter(resolve_filter(&config)))
        .with(
            tracing_opentelemetry::layer()
                .with_tracer(tracer)
                .with_level(true),
        )
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

/// Select the operator sampling policy from the configured ratio.
///
/// The process sampler applies it without reading any client-supplied trace
/// field; see the `export` module.
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

    /// A timing span reaches the exporter under its request span with exact
    /// nanosecond components, and omits the excluded component it lacks.
    #[test]
    fn timing_spans_carry_exact_nanoseconds() {
        let (guard, capture) = super::init_test_capture(super::TelemetryConfig::default())
            .expect("test telemetry pipeline initializes");
        let checkpoint = capture.checkpoint();
        let request = tracing::info_span!("request");
        super::record_timing(
            &request,
            "gateway.answer",
            "success",
            std::time::Duration::from_nanos(500_001),
            Some(std::time::Duration::from_nanos(1)),
        );
        super::record_timing(
            &tracing::Span::none(),
            "bifrost.query",
            "success",
            std::time::Duration::from_nanos(7),
            None,
        );
        drop(request);
        guard.force_flush();
        let spans = capture.finished_since(checkpoint);
        let timings: Vec<_> = spans
            .iter()
            .filter(|span| span.name == super::TIMING_SPAN)
            .collect();
        assert_eq!(timings.len(), 2);
        let field = |index: usize, key: &str| timings[index].attributes.get(key).cloned();
        assert_eq!(field(0, "boundary").as_deref(), Some("gateway.answer"));
        assert_eq!(field(0, "elapsed_ns").as_deref(), Some("500001"));
        assert_eq!(field(0, "excluded_ns").as_deref(), Some("1"));
        assert_eq!(field(1, "elapsed_ns").as_deref(), Some("7"));
        assert_eq!(field(1, "excluded_ns"), None);
        let request = spans
            .iter()
            .find(|span| span.name == "request")
            .expect("the request span was exported");
        assert_eq!(timings[0].parent_span_id, request.span_id);
        guard.shutdown();
    }
}
