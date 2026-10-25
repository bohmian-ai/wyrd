//! Telemetry setup shell.

#![deny(missing_docs)]

mod context;
mod export;

pub use context::{
    caller_sampled, continued_context, current_context, inbound_context, serve_linked_work,
    serve_request,
};
pub use export::{
    TRACE_EXPORT_FAILED_BATCHES_TOTAL, TenantSpanRoute, TenantSpanSink, bind_operation_route,
    bind_tenant_route,
};

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
    /// Log output filter directive.
    pub filter: String,
    /// Operator trace verbosity: the filter directive selecting which spans
    /// and events reach the operator's trace backend. It never removes a
    /// tenant copy.
    #[serde(default = "default_trace_filter")]
    pub trace_filter: String,
    /// Optional OTLP endpoint URL.
    pub endpoint: Option<String>,
    /// Logical service name reported by exporters.
    pub service_name: Option<String>,
    /// OTLP transport protocol.
    pub protocol: OtlpProtocol,
    /// Operator sampling ratio in the `0.0..=1.0` range; unset selects
    /// [`TelemetryConfig::operator_sample_ratio`]'s default.
    pub sample_ratio: Option<f64>,
    /// Optional exporter timeout in milliseconds.
    pub export_timeout_ms: Option<u64>,
}

impl Default for TelemetryConfig {
    fn default() -> Self {
        Self {
            filter: "info".to_string(),
            trace_filter: default_trace_filter(),
            endpoint: None,
            service_name: None,
            protocol: OtlpProtocol::Grpc,
            sample_ratio: None,
            export_timeout_ms: None,
        }
    }
}

impl TelemetryConfig {
    /// The operator sampling ratio: `sample_ratio`, or 0.01 when unset.
    ///
    /// This is the one default and validation of the ratio. `0.0` exports no
    /// trace and `1.0` every trace; the ratio applies only to the operator,
    /// never to tenant copies.
    ///
    /// # Errors
    ///
    /// Returns the reason when `sample_ratio` is outside `0.0..=1.0` or NaN.
    pub fn operator_sample_ratio(&self) -> Result<f64, String> {
        match self.sample_ratio {
            None => Ok(0.01),
            Some(ratio) if (0.0..=1.0).contains(&ratio) => Ok(ratio),
            Some(ratio) => Err(format!(
                "telemetry.sample_ratio must be in [0.0, 1.0], got {ratio}"
            )),
        }
    }
}

/// Default operator trace verbosity.
fn default_trace_filter() -> String {
    "info".to_owned()
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
/// Drop it (or call [`TelemetryGuard::shutdown`]) after serving stops to
/// flush and shut down the tracer provider, even though the global
/// subscriber still holds a tracer.
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

    /// Shut down telemetry exports now rather than when the guard drops.
    ///
    /// Equivalent to dropping the guard; see its [`Drop`] implementation.
    pub fn shutdown(self) {
        drop(self);
    }
}

impl Drop for TelemetryGuard {
    /// Flush and shut down the tracer provider exactly once.
    ///
    /// The global subscriber's tracer keeps the provider alive after the
    /// guard is gone, so the provider's own final-reference cleanup never
    /// runs; the guard shuts it down explicitly. Shutdown exports the queued
    /// spans, shuts down every exporter within the SDK's default timeout, and
    /// logs the processor's lifetime dropped-span total. A shutdown error is
    /// logged, never raised. Without a provider this does nothing.
    fn drop(&mut self) {
        if let Some(provider) = self.tracer_provider.take()
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

/// Resolve the effective log filter from multiple sources in priority order.
///
/// Priority: `WYRD_LOG` env var → `RUST_LOG` env var → `config.filter` field → `"info"`.
pub fn resolve_filter(config: &TelemetryConfig) -> EnvFilter {
    env_filter(&["WYRD_LOG", "RUST_LOG"], &config.filter)
}

/// Resolve the operator trace filter: the `WYRD_TRACE_FILTER` env var, else
/// `config.trace_filter`.
fn resolve_trace_filter(config: &TelemetryConfig) -> EnvFilter {
    env_filter(&["WYRD_TRACE_FILTER"], &config.trace_filter)
}

/// The filter named by the first non-empty variable of `vars`, else
/// `fallback`.
fn env_filter(vars: &[&str], fallback: &str) -> EnvFilter {
    vars.iter()
        .find_map(|var| std::env::var(var).ok().filter(|value| !value.is_empty()))
        .map_or_else(|| EnvFilter::new(fallback), EnvFilter::new)
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
/// Installs one global subscriber: formatted log output filtered by
/// [`resolve_filter`], and an OpenTelemetry layer over the process tracer
/// provider filtered by the operator trace filter (`WYRD_TRACE_FILTER`, else
/// `config.trace_filter`) plus the tenant catalog. The provider is always
/// built, so tenant span export works with or without an operator
/// destination. When `config.endpoint` is set, the spans the server sampler
/// samples are also batched to that OTLP endpoint.
///
/// # Errors
/// Returns an error if the sampling ratio is invalid, another Rustls
/// provider already owns the process, a global subscriber was already
/// installed, or the OTLP exporter fails to build.
pub fn init(config: TelemetryConfig) -> Result<TelemetryGuard, WyrdError> {
    use opentelemetry::trace::TracerProvider as _;
    use tracing_subscriber::util::SubscriberInitExt as _;

    let operator = config
        .endpoint
        .as_deref()
        .map(|endpoint| build_otlp_exporter(&config, endpoint))
        .transpose()?
        .map(|exporter| {
            export::OperatorExport(
                opentelemetry_sdk::trace::BatchSpanProcessor::builder(export::FailedBatches(
                    exporter,
                ))
                .build(),
            )
        });
    let provider = export::tracer_provider(
        telemetry_sampler(&config)?,
        telemetry_resource(&config),
        operator,
    );
    subscriber(
        resolve_filter(&config),
        resolve_trace_filter(&config),
        provider.tracer("wyrd"),
        false,
        std::io::stdout,
    )
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
/// export, subscriber composition, and global subscriber path as [`init`].
/// The operator's exporter destination differs, allowing journeys to inspect
/// completed spans without a second telemetry implementation, and every
/// captured span also carries its `tracing` level as the `level` attribute.
///
/// An unset `sample_ratio` captures every trace rather than the production
/// 1% default; an explicit ratio applies as in production. The operator
/// trace filter is fixed at `trace`, so capture is unfiltered:
/// every span and event the code emits reaches the exporter, and what a test
/// can assert on never depends on `WYRD_LOG` or `WYRD_TRACE_FILTER`. Only the
/// formatted log output is filtered, by [`resolve_filter`].
///
/// # Errors
///
/// Returns [`WyrdError::Validation`] if the sampling ratio is invalid, or
/// [`WyrdError::Conflict`] if another global subscriber is installed.
#[cfg(feature = "test-support")]
pub fn init_test_capture(
    config: TelemetryConfig,
) -> Result<(TelemetryGuard, TestTraceCapture), WyrdError> {
    use opentelemetry::trace::TracerProvider as _;
    use tracing_subscriber::util::SubscriberInitExt as _;

    let capture = TestTraceCapture::default();
    let exporter = CaptureExporter {
        spans: Arc::clone(&capture.spans),
    };
    let sampler = match config.sample_ratio {
        None => opentelemetry_sdk::trace::Sampler::TraceIdRatioBased(1.0),
        Some(_) => telemetry_sampler(&config)?,
    };
    let provider = export::tracer_provider(
        sampler,
        telemetry_resource(&config),
        Some(export::OperatorExport(
            opentelemetry_sdk::trace::SimpleSpanProcessor::new(exporter),
        )),
    );
    subscriber(
        resolve_filter(&config),
        EnvFilter::new("trace"),
        provider.tracer("wyrd"),
        true,
        std::io::stdout,
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

/// Compose the process subscriber shared by [`init`] and the test capture.
///
/// Each destination has its own filter. Formatted log output passes `logs`.
/// The OpenTelemetry layer over `tracer` passes `traces`, the operator's
/// diagnostic verbosity, and, whatever either filter says, every span and
/// event the tenant copy needs: catalog spans and events at `INFO` or above,
/// the verbosity the tenant catalog is defined at. Neither filter can
/// therefore remove a tenant copy. With `level` set, each exported span also
/// carries its `tracing` level.
fn subscriber<W>(
    logs: EnvFilter,
    traces: EnvFilter,
    tracer: opentelemetry_sdk::trace::Tracer,
    level: bool,
    writer: W,
) -> impl tracing::Subscriber + Send + Sync
where
    W: for<'writer> tracing_subscriber::fmt::MakeWriter<'writer> + Send + Sync + 'static,
{
    use tracing_subscriber::Layer as _;
    use tracing_subscriber::filter::{FilterExt as _, filter_fn};
    use tracing_subscriber::layer::SubscriberExt as _;

    let tenant = filter_fn(|metadata| {
        *metadata.level() <= tracing::Level::INFO
            && (metadata.is_event() || export::in_catalog(metadata.name()))
    });
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::fmt::layer()
                .with_writer(writer)
                .with_filter(logs),
        )
        .with(
            tracing_opentelemetry::layer()
                .with_tracer(tracer)
                .with_level(level)
                .with_filter(traces.or(tenant)),
        )
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
///
/// # Errors
///
/// Returns [`WyrdError::Validation`] when the configured ratio is invalid.
fn telemetry_sampler(
    config: &TelemetryConfig,
) -> Result<opentelemetry_sdk::trace::Sampler, WyrdError> {
    config
        .operator_sample_ratio()
        .map(opentelemetry_sdk::trace::Sampler::TraceIdRatioBased)
        .map_err(|message| WyrdError::Validation {
            message,
            details: serde_json::json!({ "component": "telemetry" }),
        })
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
    use std::sync::{Arc, Mutex, PoisonError};

    use opentelemetry::trace::TracerProvider as _;
    use opentelemetry_proto::tonic::trace::v1::ResourceSpans;
    use tracing_subscriber::EnvFilter;
    use wyrd_spec::DataTenantId;
    use wyrd_spec::auth::{Attribution, PrincipalId};
    use wyrd_spec::trace::TraceContext;

    use crate::{TenantSpanRoute, TenantSpanSink};

    /// Span names of every tenant copy staged, in end order.
    #[derive(Default)]
    struct Staged(
        /// The staged span names.
        Mutex<Vec<String>>,
    );

    impl TenantSpanSink for Staged {
        /// Record the staged copy's span names.
        fn stage(&self, _route: TenantSpanRoute, span: ResourceSpans) {
            self.0
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .extend(
                    span.scope_spans
                        .iter()
                        .flat_map(|scope| &scope.spans)
                        .map(|span| span.name.clone()),
                );
        }
    }

    /// Serve one sampled client's request under the production subscriber
    /// composed with log filter `logs`, operator filter `traces`, and
    /// operator sampling `ratio`, and return the tenant copies and the
    /// operator's spans by name, and the log output.
    ///
    /// The request runs a catalog child, a debug diagnostic span, an info
    /// event, and an error event.
    ///
    /// # Panics
    ///
    /// Panics when the fixed caller traceparent or principal id fails to
    /// parse.
    fn serve(logs: &str, traces: &str, ratio: f64) -> (Vec<String>, Vec<String>, String) {
        let operator = super::CaptureExporter {
            spans: Arc::default(),
        };
        let exported = Arc::clone(&operator.spans);
        let provider = super::export::tracer_provider(
            opentelemetry_sdk::trace::Sampler::TraceIdRatioBased(ratio),
            opentelemetry_sdk::Resource::builder_empty().build(),
            Some(super::export::OperatorExport(
                opentelemetry_sdk::trace::SimpleSpanProcessor::new(operator),
            )),
        );
        let staged = Arc::new(Staged::default());
        let sink: Arc<dyn TenantSpanSink> = Arc::clone(&staged) as _;
        let caller = TraceContext::parse_traceparent(
            "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01",
            String::new(),
        )
        .expect("a valid traceparent");
        let output = Logs::default();
        let writer = output.clone();
        let subscriber = super::subscriber(
            EnvFilter::new(logs),
            EnvFilter::new(traces),
            provider.tracer("wyrd"),
            false,
            move || writer.clone(),
        );
        tracing::subscriber::with_default(subscriber, || {
            let request = tracing::info_span!("request");
            crate::serve_request(&request, Some(&caller), Some(&sink));
            request.in_scope(|| {
                crate::bind_tenant_route(TenantSpanRoute {
                    tenant: DataTenantId::new_v7(),
                    attribution: Attribution::direct(
                        "01900000-0000-7000-8000-000000000007"
                            .parse::<PrincipalId>()
                            .expect("a principal id"),
                    ),
                });
                tracing::info_span!("verification.run").in_scope(|| {
                    tracing::debug_span!("verification.detail").in_scope(|| {});
                    tracing::info!("verification step");
                    tracing::error!(error.type = "timeout", "verification failed");
                });
            });
        });
        let operator = exported
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .map(|span| span.name.to_string())
            .collect();
        let tenant = std::mem::take(&mut *staged.0.lock().unwrap_or_else(PoisonError::into_inner));
        (tenant, operator, output.text())
    }

    /// Log verbosity and operator trace verbosity leave the tenant copy of
    /// a sampled client's request unchanged; only the operator's diagnostic
    /// spans follow the operator filter, and an error in a request the
    /// operator did not sample is still logged inside its request span.
    ///
    /// # Panics
    ///
    /// Panics when a tenant copy differs between filter settings, the
    /// operator receives the wrong spans, or the unsampled error is missing
    /// from the log or lacks its request's span context.
    #[test]
    fn production_filters_preserve_tenant_traces() {
        let (tenant, operator, logs) = serve("info", "info", 0.0);
        assert_eq!(tenant, ["verification.run", "request"]);
        assert!(
            operator.is_empty(),
            "ratio zero exports nothing: {operator:?}"
        );
        let failure = logs
            .lines()
            .find(|line| line.contains("verification failed"))
            .unwrap_or_else(|| panic!("the unsampled error is logged: {logs}"));
        assert!(
            failure.contains("request") && failure.contains("verification.run"),
            "the error carries its request's span context: {failure}"
        );
        assert!(!serve("off", "info", 0.0).2.contains("verification failed"));
        for (logs, traces) in [("off", "info"), ("error", "error"), ("info", "off")] {
            assert_eq!(
                serve(logs, traces, 0.0).0,
                tenant,
                "logs {logs:?}, traces {traces:?}"
            );
        }
        assert_eq!(serve("off", "info", 1.0).1, ["verification.run", "request"]);
        assert_eq!(
            serve("off", "debug", 1.0).1,
            ["verification.detail", "verification.run", "request"]
        );
    }

    /// Log output shared between a test and the subscriber writing it.
    #[derive(Clone, Default)]
    struct Logs(
        /// The bytes written so far.
        Arc<Mutex<Vec<u8>>>,
    );

    impl Logs {
        /// Everything written so far.
        fn text(&self) -> String {
            String::from_utf8_lossy(&self.0.lock().unwrap_or_else(PoisonError::into_inner))
                .into_owned()
        }
    }

    impl std::io::Write for Logs {
        /// Append `bytes`.
        ///
        /// # Errors
        ///
        /// Never fails.
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .extend_from_slice(bytes);
            Ok(bytes.len())
        }

        /// Nothing is buffered.
        ///
        /// # Errors
        ///
        /// Never fails.
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    /// An operator exporter whose every export call blocks until the test
    /// releases it with the call's result.
    #[derive(Debug)]
    struct Gated {
        /// Reports each call's batch size as the call starts.
        started: Mutex<std::sync::mpsc::Sender<usize>>,
        /// Supplies each call's result.
        results: Mutex<std::sync::mpsc::Receiver<opentelemetry_sdk::error::OTelSdkResult>>,
        /// Reports shutdown.
        shut: Mutex<std::sync::mpsc::Sender<()>>,
    }

    impl opentelemetry_sdk::trace::SpanExporter for Gated {
        /// Report the call, then wait for the test to release its result.
        ///
        /// # Errors
        ///
        /// Returns the error the test released for this call.
        fn export(
            &self,
            batch: Vec<opentelemetry_sdk::trace::SpanData>,
        ) -> impl std::future::Future<Output = opentelemetry_sdk::error::OTelSdkResult> + Send
        {
            let _ = self
                .started
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .send(batch.len());
            let result = self
                .results
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .recv()
                .unwrap_or(Ok(()));
            std::future::ready(result)
        }

        /// Report shutdown.
        ///
        /// # Errors
        ///
        /// Never fails.
        fn shutdown_with_timeout(
            &mut self,
            _timeout: std::time::Duration,
        ) -> opentelemetry_sdk::error::OTelSdkResult {
            let _ = self
                .shut
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .send(());
            Ok(())
        }
    }

    /// Counts increments of the failed-batch counter and nothing else.
    struct FailedBatchCount(
        /// The failed-batch counter's cell.
        Arc<std::sync::atomic::AtomicU64>,
    );

    impl metrics::Recorder for FailedBatchCount {
        /// Descriptions are not recorded.
        fn describe_counter(
            &self,
            _: metrics::KeyName,
            _: Option<metrics::Unit>,
            _: metrics::SharedString,
        ) {
        }

        /// Descriptions are not recorded.
        fn describe_gauge(
            &self,
            _: metrics::KeyName,
            _: Option<metrics::Unit>,
            _: metrics::SharedString,
        ) {
        }

        /// Descriptions are not recorded.
        fn describe_histogram(
            &self,
            _: metrics::KeyName,
            _: Option<metrics::Unit>,
            _: metrics::SharedString,
        ) {
        }

        /// The failed-batch counter shares one cell; any other is a no-op.
        fn register_counter(
            &self,
            key: &metrics::Key,
            _: &metrics::Metadata<'_>,
        ) -> metrics::Counter {
            if key.name() == crate::TRACE_EXPORT_FAILED_BATCHES_TOTAL {
                metrics::Counter::from_arc(Arc::clone(&self.0))
            } else {
                metrics::Counter::noop()
            }
        }

        /// Gauges are not recorded.
        fn register_gauge(&self, _: &metrics::Key, _: &metrics::Metadata<'_>) -> metrics::Gauge {
            metrics::Gauge::noop()
        }

        /// Histograms are not recorded.
        fn register_histogram(
            &self,
            _: &metrics::Key,
            _: &metrics::Metadata<'_>,
        ) -> metrics::Histogram {
            metrics::Histogram::noop()
        }
    }

    /// A failed export call counts one failed batch and returns its error
    /// unchanged; a successful one counts nothing. Through the stock batch
    /// processor: a stalled export never blocks a
    /// span's end, a full queue drops spans with the SDK's first-drop warning
    /// and its exact lifetime total at shutdown, each failed export call
    /// counts one failed batch while its error reaches the processor, and a
    /// stalled shutdown returns within its timeout before the exporter is
    /// shut down.
    ///
    /// # Panics
    ///
    /// Panics when another metrics recorder is installed, an export call
    /// does not start or complete as released, a failed call is miscounted
    /// or its error altered, a drop signal is missing, or the stalled
    /// shutdown does not time out before reaching the exporter.
    #[test]
    fn operator_export_loss_is_observable() {
        use opentelemetry::trace::{Span as _, Tracer as _};
        use opentelemetry_sdk::trace::SpanExporter as _;
        use std::future::Future as _;
        use std::sync::atomic::Ordering;
        use std::sync::mpsc::channel;

        let failed = Arc::new(std::sync::atomic::AtomicU64::new(0));
        metrics::set_global_recorder(FailedBatchCount(Arc::clone(&failed)))
            .expect("the only recorder in this test process");
        let (started, calls) = channel();
        let (release, results) = channel();
        let (shut, shutdowns) = channel();
        let direct = super::export::FailedBatches(Gated {
            started: Mutex::new(channel().0),
            results: Mutex::new(results),
            shut: Mutex::new(channel().0),
        });
        let failure = || {
            opentelemetry_sdk::error::OTelSdkError::InternalFailure("collector unavailable".into())
        };
        let export = |result| {
            release.send(result).expect("the exporter waits");
            let mut call = std::pin::pin!(direct.export(Vec::new()));
            match call
                .as_mut()
                .poll(&mut std::task::Context::from_waker(std::task::Waker::noop()))
            {
                std::task::Poll::Ready(result) => result,
                std::task::Poll::Pending => panic!("the export completed"),
            }
        };
        assert!(matches!(
            export(Err(failure())),
            Err(opentelemetry_sdk::error::OTelSdkError::InternalFailure(cause))
                if cause == "collector unavailable"
        ));
        assert!(export(Ok(())).is_ok());
        assert_eq!(failed.load(Ordering::Acquire), 1, "only the failed call");
        let (release, results) = channel();
        let processor = opentelemetry_sdk::trace::BatchSpanProcessor::builder(
            super::export::FailedBatches(Gated {
                started: Mutex::new(started),
                results: Mutex::new(results),
                shut: Mutex::new(shut),
            }),
        )
        .with_batch_config(
            opentelemetry_sdk::trace::BatchConfigBuilder::default()
                .with_max_queue_size(1)
                .with_max_export_batch_size(1)
                .with_scheduled_delay(std::time::Duration::from_secs(3600))
                .build(),
        )
        .build();
        let provider = super::export::tracer_provider(
            opentelemetry_sdk::trace::Sampler::AlwaysOn,
            opentelemetry_sdk::Resource::builder_empty().build(),
            Some(super::export::OperatorExport(processor)),
        );
        let tracer = provider.tracer("wyrd");
        let logs = Logs::default();
        let writer = logs.clone();
        let subscriber = tracing_subscriber::fmt()
            .with_writer(move || writer.clone())
            .with_ansi(false)
            .finish();
        tracing::subscriber::with_default(subscriber, || {
            let end = |name: &'static str| tracer.start(name).end();
            end("exported");
            assert_eq!(calls.recv(), Ok(1), "the first export call is stalled");
            end("queued");
            end("dropped");
            end("dropped too");
            assert!(
                logs.text()
                    .contains("BatchSpanProcessor.SpanDroppingStarted")
            );
            release
                .send(Err(
                    opentelemetry_sdk::error::OTelSdkError::InternalFailure(
                        "collector unavailable".to_owned(),
                    ),
                ))
                .expect("the exporter waits");
            assert_eq!(calls.recv(), Ok(1), "the queued span exports next");
            release.send(Ok(())).expect("the exporter waits");
            assert!(provider.force_flush().is_ok());
            assert_eq!(failed.load(Ordering::Acquire), 2, "one more failed call");
            end("stalled at shutdown");
            assert_eq!(calls.recv(), Ok(1));
            let bound = std::time::Duration::from_millis(100);
            assert!(matches!(
                provider.shutdown_with_timeout(bound),
                Err(opentelemetry_sdk::error::OTelSdkError::InternalFailure(errors))
                    if errors.contains("Timeout(100ms)")
            ));
            assert!(
                logs.text().contains("dropped_span_count=2"),
                "{}",
                logs.text()
            );
            release.send(Ok(())).expect("the exporter waits");
            assert_eq!(shutdowns.recv(), Ok(()), "shutdown reaches the exporter");
        });
        assert_eq!(failed.load(Ordering::Acquire), 2);
    }

    /// Dropping the guard shuts down the provider even while a tracer still
    /// holds it, as the global subscriber does in production: the queued span
    /// is exported, the exporter is shut down once, and the SDK logs its
    /// exact lifetime dropped total. An explicit shutdown also shuts the
    /// exporter down once, and a guard without a provider drops harmlessly.
    ///
    /// # Panics
    ///
    /// Panics when the stalled export never starts, a drop is not warned,
    /// the queued span is not drained, the dropped total is missing, or an
    /// exporter is shut down other than exactly once.
    #[test]
    fn guard_drop_shuts_down_a_retained_provider() {
        use opentelemetry::trace::{Span as _, Tracer as _};
        use std::sync::mpsc::{TryRecvError, channel};

        let guarded = |started, results, shut| {
            let processor = opentelemetry_sdk::trace::BatchSpanProcessor::builder(Gated {
                started: Mutex::new(started),
                results: Mutex::new(results),
                shut: Mutex::new(shut),
            })
            .with_batch_config(
                opentelemetry_sdk::trace::BatchConfigBuilder::default()
                    .with_max_queue_size(1)
                    .with_max_export_batch_size(1)
                    .with_scheduled_delay(std::time::Duration::from_secs(3600))
                    .build(),
            )
            .build();
            let provider = super::export::tracer_provider(
                opentelemetry_sdk::trace::Sampler::AlwaysOn,
                opentelemetry_sdk::Resource::builder_empty().build(),
                Some(super::export::OperatorExport(processor)),
            );
            let tracer = provider.tracer("wyrd");
            let guard = super::TelemetryGuard {
                config: super::TelemetryConfig::default(),
                tracer_provider: Some(provider),
            };
            (guard, tracer)
        };
        let (started, calls) = channel();
        let (release, results) = channel();
        let (shut, shutdowns) = channel();
        let (guard, tracer) = guarded(started, results, shut);
        let logs = Logs::default();
        let writer = logs.clone();
        let subscriber = tracing_subscriber::fmt()
            .with_writer(move || writer.clone())
            .with_ansi(false)
            .finish();
        tracing::subscriber::with_default(subscriber, || {
            let end = |name: &'static str| tracer.start(name).end();
            end("exported");
            assert_eq!(calls.recv(), Ok(1), "the first export call is stalled");
            end("queued");
            end("dropped");
            assert!(
                logs.text()
                    .contains("BatchSpanProcessor.SpanDroppingStarted")
            );
            for _ in 0..2 {
                release.send(Ok(())).expect("the exporter waits");
            }
            drop(guard);
            assert_eq!(calls.try_recv(), Ok(1), "the queued span is drained");
            assert_eq!(shutdowns.try_recv(), Ok(()), "the exporter is shut down");
            assert!(
                logs.text().contains("dropped_span_count=1"),
                "{}",
                logs.text()
            );
        });
        drop(tracer);
        assert_eq!(shutdowns.try_recv(), Err(TryRecvError::Disconnected));

        let (shut, shutdowns) = channel();
        let (guard, _tracer) = guarded(channel().0, channel().1, shut);
        guard.shutdown();
        assert_eq!(shutdowns.try_recv(), Ok(()));
        assert_eq!(shutdowns.try_recv(), Err(TryRecvError::Disconnected));
        drop(super::init_test_only_no_global(
            super::TelemetryConfig::default(),
        ));
    }

    /// An unset operator ratio samples 1% of traces, an explicit ratio in
    /// `0.0..=1.0` is honored exactly, and anything else is refused.
    ///
    /// # Panics
    ///
    /// Panics when the default is not 1%, an explicit ratio is altered, or
    /// an invalid ratio is accepted.
    #[test]
    fn operator_sampling_defaults_are_explicit() {
        let sampler = |sample_ratio| {
            super::telemetry_sampler(&super::TelemetryConfig {
                sample_ratio,
                ..super::TelemetryConfig::default()
            })
            .map(|sampler| format!("{sampler:?}"))
            .ok()
        };
        assert_eq!(sampler(None).as_deref(), Some("TraceIdRatioBased(0.01)"));
        for ratio in [0.0, 0.25, 1.0] {
            assert_eq!(
                sampler(Some(ratio)),
                Some(format!("TraceIdRatioBased({ratio:?})"))
            );
        }
        for ratio in [-0.1, 1.1, f64::NAN, f64::INFINITY] {
            assert_eq!(sampler(Some(ratio)), None, "{ratio} is refused");
        }
    }

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
    ///
    /// # Panics
    ///
    /// Panics when the test pipeline does not initialize or an exported
    /// timing span or component is missing or wrong.
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
