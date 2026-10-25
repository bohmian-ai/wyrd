//! Dual export of server spans: the operator's trace backend and the
//! requesting tenant's `vala.traces.spans`.
//!
//! The process has one tracer provider, so each span gets one recording
//! decision. [`ServerSampler`] makes that one decision carry two independent
//! answers. The operator's answer comes from the server's configured policy
//! alone. The tenant's answer is whether the request continued a sampled
//! client trace or links a sampled client span. A span is sampled (the operator exports it), recorded but not
//! sampled (only a tenant may receive it), or dropped. [`OperatorExport`]
//! limits the operator's processor to sampled spans, so what the operator
//! receives never depends on client input.
//!
//! [`TenantExport`] stages the tenant copy:
//! - The serving server's request edge puts its own [`TenantSpanSink`] in the
//!   request span's parent context with [`crate::serve_request`], so a
//!   process hosting several servers stages each request's copies on the
//!   server that served it.
//! - A span that starts with a sampled remote cause, its parent or one of its
//!   links, opens a route on that sink, and each descendant the
//!   behavior-span catalog names joins its parent's route. A span outside the
//!   catalog has no route, so neither it nor any span below it is copied. A
//!   queued write reaches the server with its producers as links and no
//!   `traceparent`, so its CONSUMER span opens the route and the tenant reads
//!   it as a trace linked to the producers.
//! - Once authentication succeeds, the server binds the tenant and principal
//!   to that route with [`bind_tenant_route`].
//! - When a span on a bound route ends, it is filtered to its safe attributes
//!   and staged on that route's sink.
//!
//! The route is never a span attribute and is never exported.

use std::collections::HashMap;
use std::sync::{Arc, LazyLock, Mutex, OnceLock, PoisonError};
use std::time::Duration;

use opentelemetry::trace::{
    Link, SamplingDecision, SamplingResult, Span as _, SpanContext, SpanId, SpanKind, Status,
    TraceContextExt, TraceId, TraceState,
};
use opentelemetry::{Context, Key, KeyValue};
use opentelemetry_proto::tonic::trace::v1::ResourceSpans;
use opentelemetry_proto::transform::common::tonic::ResourceAttributesWithSchema;
use opentelemetry_sdk::Resource;
use opentelemetry_sdk::error::OTelSdkResult;
use opentelemetry_sdk::trace::{
    IdGenerator as _, RandomIdGenerator, Sampler, SdkTracerProvider, ShouldSample, Span, SpanData,
    SpanExporter, SpanProcessor,
};
use tracing_opentelemetry::OpenTelemetrySpanExt as _;
use wyrd_spec::DataTenantId;
use wyrd_spec::auth::Attribution;

/// Resource attribute naming the service, the only resource attribute a
/// tenant copy keeps.
const SERVICE_NAME: &str = "service.name";

/// Event attribute naming an exception's type, the only event attribute a
/// tenant copy keeps.
const EXCEPTION_TYPE: &str = "exception.type";

/// Build the process tracer provider.
///
/// The provider has the [`ServerSampler`] over the configured operator
/// `policy`, the [`TenantExport`], and the `operator` processor when an
/// operator destination is configured. Without one, spans still record for
/// tenants and nothing reaches an operator.
pub(crate) fn tracer_provider(
    policy: Sampler,
    resource: Resource,
    operator: Option<impl SpanProcessor + 'static>,
) -> SdkTracerProvider {
    let builder = SdkTracerProvider::builder()
        .with_sampler(ServerSampler::new(policy))
        .with_resource(resource)
        .with_span_processor(TenantExport::default());
    match operator {
        Some(operator) => builder.with_span_processor(operator),
        None => builder,
    }
    .build()
}

/// The process sampler. Its operator decision comes from server policy
/// alone, and it also records spans for a tenant that continued a sampled
/// client trace.
///
/// For a span with no parent, `policy` decides on its server-generated trace
/// id. For a span whose parent is remote, `policy` decides on a trace id drawn
/// here, so no field the client sends changes the operator decision. If the
/// operator rejects it and the client's parent is sampled, the span is still
/// recorded for the tenant. A span with a local parent follows that parent:
/// sampled, recorded only, or dropped. Any span the operator rejects is
/// still recorded for the tenant when it links a sampled remote span. Every
/// span starts with the parent's trace state, and the operator's copy drops
/// it, so a client's `tracestate` never reaches the operator.
#[derive(Clone, Debug)]
pub(crate) struct ServerSampler {
    /// The configured operator policy: always on, or a trace id ratio.
    policy: Sampler,
    /// Source of the server-drawn trace id a remote parent is judged on.
    ids: RandomIdGenerator,
}

impl ServerSampler {
    /// Wrap the configured operator `policy`.
    pub(crate) fn new(policy: Sampler) -> Self {
        Self {
            policy,
            ids: RandomIdGenerator::default(),
        }
    }

    /// Whether the operator policy samples a parentless span of trace
    /// `trace_id` with this name, kind, attributes, and links.
    fn operator_samples(
        &self,
        trace_id: TraceId,
        name: &str,
        kind: &SpanKind,
        attributes: &[KeyValue],
        links: &[Link],
    ) -> bool {
        self.policy
            .should_sample(None, trace_id, name, kind, attributes, links)
            .decision
            == SamplingDecision::RecordAndSample
    }
}

impl ShouldSample for ServerSampler {
    /// Decide one span from its parent, as the type documentation describes.
    fn should_sample(
        &self,
        parent_context: Option<&Context>,
        trace_id: TraceId,
        name: &str,
        span_kind: &SpanKind,
        attributes: &[KeyValue],
        links: &[Link],
    ) -> SamplingResult {
        let parent = parent_context
            .filter(|context| context.has_active_span())
            .map(TraceContextExt::span);
        let trace_state = parent
            .as_ref()
            .map(|parent| parent.span_context().trace_state().clone())
            .unwrap_or_default();
        let operator = match &parent {
            None => self.operator_samples(trace_id, name, span_kind, attributes, links),
            Some(parent) if parent.span_context().is_remote() => {
                self.operator_samples(self.ids.new_trace_id(), name, span_kind, attributes, links)
            }
            Some(parent) => parent.span_context().is_sampled(),
        };
        let tenant = links_sampled(links)
            || parent.as_ref().is_some_and(|parent| {
                if parent.span_context().is_remote() {
                    parent.span_context().is_sampled()
                } else {
                    parent.is_recording()
                }
            });
        let decision = if operator {
            SamplingDecision::RecordAndSample
        } else if tenant {
            SamplingDecision::RecordOnly
        } else {
            SamplingDecision::Drop
        };
        SamplingResult {
            decision,
            attributes: Vec::new(),
            trace_state,
        }
    }
}

/// Whether any of `links` is a sampled remote span.
fn links_sampled(links: &[Link]) -> bool {
    links
        .iter()
        .any(|link| link.span_context.is_remote() && link.span_context.is_sampled())
}

/// Wraps the operator's span processor and passes on only the spans the
/// operator sampled, without their trace state.
///
/// Spans recorded only for a tenant therefore never enter the operator's
/// queue, so its content and volume are what the server sampler alone
/// decides. The live context keeps a caller's `tracestate` for propagation;
/// the operator's copy drops it, so client state never reaches the
/// operator's backend.
#[derive(Debug)]
pub(crate) struct OperatorExport<P>(pub(crate) P);

impl<P: SpanProcessor> SpanProcessor for OperatorExport<P> {
    /// Pass every start on unchanged.
    fn on_start(&self, span: &mut Span, cx: &Context) {
        self.0.on_start(span, cx);
    }

    /// Pass on an ended span, its trace state cleared, only when the
    /// operator sampled it.
    fn on_end(&self, mut span: SpanData) {
        if span.span_context.is_sampled() {
            let context = &span.span_context;
            span.span_context = SpanContext::new(
                context.trace_id(),
                context.span_id(),
                context.trace_flags(),
                context.is_remote(),
                TraceState::default(),
            );
            self.0.on_end(span);
        }
    }

    /// Flush the operator's processor.
    ///
    /// # Errors
    ///
    /// Returns the operator processor's flush failure.
    fn force_flush(&self) -> OTelSdkResult {
        self.0.force_flush()
    }

    /// Shut down the operator's processor within `timeout`.
    ///
    /// # Errors
    ///
    /// Returns the operator processor's shutdown failure.
    fn shutdown_with_timeout(&self, timeout: Duration) -> OTelSdkResult {
        self.0.shutdown_with_timeout(timeout)
    }

    /// Give the operator's processor the process resource.
    fn set_resource(&mut self, resource: &Resource) {
        self.0.set_resource(resource);
    }
}

/// Counter of operator export calls that returned an error.
///
/// One failed call is one failed batch, not a count of lost spans: the
/// exporter may have retried it, and spans the processor's full queue drops
/// never reach an export call. Tenant copies are counted by their own sink.
pub const TRACE_EXPORT_FAILED_BATCHES_TOTAL: &str = "wyrd_trace_export_failed_batches_total";

/// Wraps the operator's exporter and counts each export call that fails in
/// [`TRACE_EXPORT_FAILED_BATCHES_TOTAL`].
///
/// The result is returned unchanged, and flush, shutdown, and resource
/// calls go straight to the wrapped exporter.
#[derive(Debug)]
pub(crate) struct FailedBatches<E>(
    /// The operator's exporter, which performs every call.
    pub(crate) E,
);

impl<E: SpanExporter> SpanExporter for FailedBatches<E> {
    /// Export `batch`, counting the call once if it fails.
    ///
    /// # Errors
    ///
    /// Returns the wrapped exporter's export failure unchanged.
    fn export(
        &self,
        batch: Vec<SpanData>,
    ) -> impl std::future::Future<Output = OTelSdkResult> + Send {
        let export = self.0.export(batch);
        async move {
            let result = export.await;
            if result.is_err() {
                metrics::counter!(TRACE_EXPORT_FAILED_BATCHES_TOTAL).increment(1);
            }
            result
        }
    }

    /// Shut down the wrapped exporter.
    ///
    /// # Errors
    ///
    /// Returns the wrapped exporter's shutdown failure.
    fn shutdown(&mut self) -> OTelSdkResult {
        self.0.shutdown()
    }

    /// Shut down the wrapped exporter within `timeout`.
    ///
    /// # Errors
    ///
    /// Returns the wrapped exporter's shutdown failure.
    fn shutdown_with_timeout(&mut self, timeout: Duration) -> OTelSdkResult {
        self.0.shutdown_with_timeout(timeout)
    }

    /// Flush the wrapped exporter.
    ///
    /// # Errors
    ///
    /// Returns the wrapped exporter's flush failure.
    fn force_flush(&mut self) -> OTelSdkResult {
        self.0.force_flush()
    }

    /// Give the wrapped exporter the process resource.
    fn set_resource(&mut self, resource: &Resource) {
        self.0.set_resource(resource);
    }
}

/// The authenticated identity a request's tenant span copies are written
/// under.
///
/// Rows are stamped with `attribution` and written to `tenant`. The tenant is
/// never a span attribute.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TenantSpanRoute {
    /// Tenant whose `vala.traces.spans` receives the copies.
    pub tenant: DataTenantId,
    /// Subject and actor of the emitting operation, stamped on each row.
    pub attribution: Attribution,
}

/// Where tenant span copies are staged.
///
/// Each server builds one over its Scribe outbox, and its request edges carry
/// it to the routes their requests open with [`crate::serve_request`].
pub trait TenantSpanSink: Send + Sync {
    /// Stage one tenant copy for `route` as a single-span OTLP resource span.
    ///
    /// This must return without waiting. The sink counts a copy it cannot
    /// take and drops it.
    fn stage(&self, route: TenantSpanRoute, span: ResourceSpans);
}

/// One request's route, shared by every span of the request.
struct Route {
    /// The authenticated identity, unset until the server binds it.
    identity: OnceLock<TenantSpanRoute>,
    /// The serving server's sink the copies are staged on.
    sink: Arc<dyn TenantSpanSink>,
}

impl Route {
    /// An unbound route staging on `sink`.
    fn new(sink: Arc<dyn TenantSpanSink>) -> RouteSlot {
        Arc::new(Self {
            identity: OnceLock::new(),
            sink,
        })
    }
}

/// The serving server's tenant span sink, carried in a request span's
/// OpenTelemetry context to every span the request opens.
pub(crate) struct RequestSink(pub(crate) Arc<dyn TenantSpanSink>);

/// A request's route as every live span of the request holds it.
type RouteSlot = Arc<Route>;

/// The process routing registry of live request routes.
struct TenantRoutes {
    /// The route of every live catalog span whose ancestors up to a sampled
    /// remote parent are all catalog spans, keyed by the span's own ids,
    /// which the server generated.
    // ponytail: one process lock per routed span start and end; shard by span id if it contends.
    live: Mutex<HashMap<(TraceId, SpanId), RouteSlot>>,
}

impl TenantRoutes {
    /// Record the route `span` joins as it starts under `parent`.
    ///
    /// A span with a routed local parent joins the parent's route. A span
    /// with a sampled remote cause, its parent or one of its links, opens a
    /// new route on the serving server's `sink`, when the request edge
    /// carried one. Either way the span joins only if the catalog names it,
    /// so a span outside the catalog takes its whole subtree out of the
    /// tenant copy. Any other span has no route. `data` reads the span's name
    /// and links, only when it could join.
    fn start(
        &self,
        parent: &SpanContext,
        span: &SpanContext,
        sink: Option<&RequestSink>,
        data: impl FnOnce() -> Option<SpanData>,
    ) {
        let mut live = self.live.lock().unwrap_or_else(PoisonError::into_inner);
        let joined = (!parent.is_remote())
            .then(|| {
                live.get(&(parent.trace_id(), parent.span_id()))
                    .map(Arc::clone)
            })
            .flatten();
        if joined.is_none() && sink.is_none() {
            return;
        }
        // ponytail: reads every unrouted request span's data to see its links; give the
        // sampler's verdict to the processor if span starts show up in profiles.
        let Some(data) = data().filter(|data| in_catalog(&data.name)) else {
            return;
        };
        let route = match (joined, sink) {
            (Some(route), _) => route,
            (None, Some(RequestSink(sink)))
                if (parent.is_remote() && parent.is_sampled()) || links_sampled(&data.links) =>
            {
                Route::new(Arc::clone(sink))
            }
            _ => return,
        };
        live.insert((span.trace_id(), span.span_id()), route);
    }

    /// Forget the ended `span` and return its route and bound identity when
    /// the route is bound.
    fn end(&self, span: &SpanContext) -> Option<(TenantSpanRoute, RouteSlot)> {
        let route = self
            .live
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(&(span.trace_id(), span.span_id()))?;
        Some((*route.identity.get()?, route))
    }

    /// Bind `route` to the request `span` belongs to.
    ///
    /// The first binding of a request wins. A span with no route ignores the
    /// call: an untraced or unsampled request, deferred work, or a span
    /// that started after its parent ended.
    fn bind(&self, span: &SpanContext, route: TenantSpanRoute) {
        if let Some(slot) = self.slot(span) {
            let _ = slot.identity.set(route);
        }
    }

    /// Give the live `span` and the spans it opens afterwards their own
    /// route to `route` on the same sink.
    ///
    /// The request and its other spans keep their binding. A span with no
    /// route ignores the call, as [`Self::bind`] does.
    fn rebind(&self, span: &SpanContext, route: TenantSpanRoute) {
        let mut live = self.live.lock().unwrap_or_else(PoisonError::into_inner);
        let key = (span.trace_id(), span.span_id());
        let Some(slot) = live.get(&key) else {
            return;
        };
        let own = Arc::new(Route {
            identity: OnceLock::from(route),
            sink: Arc::clone(&slot.sink),
        });
        live.insert(key, own);
    }

    /// The route of the live `span`, if it has one.
    fn slot(&self, span: &SpanContext) -> Option<RouteSlot> {
        self.live
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(&(span.trace_id(), span.span_id()))
            .map(Arc::clone)
    }
}

/// The one process routing registry.
static ROUTES: LazyLock<TenantRoutes> = LazyLock::new(|| TenantRoutes {
    live: Mutex::new(HashMap::new()),
});

/// Route the tenant copies of the current request's spans to `route`.
///
/// The server calls this once a request's credential verifies, inside the
/// request. It binds every span of the request, including spans already
/// running and spans started later. A request that neither continued a
/// sampled client trace nor linked a sampled producer, deferred work, and the
/// reserved system tenant are not routed. Their spans go only to the
/// operator.
pub fn bind_tenant_route(route: TenantSpanRoute) {
    if route.tenant == DataTenantId::SYSTEM_OWNER {
        return;
    }
    let context = tracing::Span::current().context();
    ROUTES.bind(context.span().span_context(), route);
}

/// Route the tenant copies of the current span and the spans it opens to
/// `route`, apart from the rest of its request.
///
/// An operation performed by a different actor than its request binds its
/// own pair here, such as an agent-check call the tenant SYSTEM principal makes for
/// the request's subject. The request and its other spans keep the pair
/// [`bind_tenant_route`] bound. An unrouted span and the reserved system
/// tenant are not routed.
pub fn bind_operation_route(route: TenantSpanRoute) {
    if route.tenant == DataTenantId::SYSTEM_OWNER {
        return;
    }
    let context = tracing::Span::current().context();
    ROUTES.rebind(context.span().span_context(), route);
}

/// Stages each ended span on a bound route as its tenant copy.
#[derive(Debug, Default)]
pub(crate) struct TenantExport {
    /// The tenant copy's resource: the service name alone.
    resource: ResourceAttributesWithSchema,
}

impl SpanProcessor for TenantExport {
    /// Record the route the starting span joins, as
    /// [`TenantRoutes::start`] describes.
    fn on_start(&self, span: &mut Span, cx: &Context) {
        ROUTES.start(
            cx.span().span_context(),
            span.span_context(),
            cx.get::<RequestSink>(),
            || span.exported_data(),
        );
    }

    /// Stage the tenant copy of an ended span on its route's sink if the
    /// route's identity is bound.
    ///
    /// The span leaves the registry first, so the registry holds only live
    /// spans.
    fn on_end(&self, span: SpanData) {
        if let Some((identity, route)) = ROUTES.end(&span.span_context) {
            route.sink.stage(
                identity,
                ResourceSpans::new(tenant_copy(span), &self.resource),
            );
        }
    }

    /// Nothing is buffered here; the sink owns delivery.
    ///
    /// # Errors
    ///
    /// Never fails.
    fn force_flush(&self) -> OTelSdkResult {
        Ok(())
    }

    /// Nothing is buffered here; the sink owns delivery.
    ///
    /// # Errors
    ///
    /// Never fails.
    fn shutdown_with_timeout(&self, _timeout: Duration) -> OTelSdkResult {
        Ok(())
    }

    /// Keep the process resource's service name for tenant copies, and
    /// nothing else from it.
    fn set_resource(&mut self, resource: &Resource) {
        let service = resource
            .get(&Key::from_static_str(SERVICE_NAME))
            .map(|name| KeyValue::new(SERVICE_NAME, name));
        self.resource = ResourceAttributesWithSchema::from(
            &Resource::builder_empty().with_attributes(service).build(),
        );
    }
}

/// Whether the behavior-span catalog names span `name`, which makes it
/// eligible for a tenant copy.
///
/// The catalog is the request span, `verification.*`, the Agent loop's
/// GenAI spans (`invoke_agent`, `chat`, `execute_tool`) that sit between a
/// Verifier's Agent action and its gateway call, `dispatch`,
/// `agent.gateway.call`, `gateway.*` except `gateway.attempt` (the gateway
/// captures attempts as their own rows, so nothing below an attempt is
/// copied either), `bifrost.gate.*`,
/// `bifrost.oracle.query`, and the OTLP ingest spans.
pub(crate) fn in_catalog(name: &str) -> bool {
    matches!(
        name,
        "request"
            | "dispatch"
            | "agent.gateway.call"
            | "bifrost.oracle.query"
            | "invoke_agent"
            | "chat"
            | "execute_tool"
    ) || name.starts_with("verification.")
        || name.starts_with("bifrost.gate.")
        || name.starts_with("otlp.")
        || (name.starts_with("gateway.") && name != "gateway.attempt")
}

/// Whether a tenant copy keeps span attribute `key`.
///
/// It keeps `wyrd.*` correlation, `gen_ai.*`, and the HTTP, RPC, and error
/// semantic-convention fields of the safe-attribute allowlist.
fn safe_attribute(key: &str) -> bool {
    key.starts_with("wyrd.")
        || key.starts_with("gen_ai.")
        || matches!(
            key,
            "http.request.method"
                | "http.route"
                | "http.response.status_code"
                | "rpc.system"
                | "rpc.service"
                | "rpc.method"
                | "rpc.grpc.status_code"
                | "error.type"
        )
}

/// Reduce `span` to what its tenant may see.
///
/// Only safe attributes are kept. Only exception events are kept, each named
/// `exception` and carrying only its `exception.type`. An error status keeps
/// its code and loses its description. Ids, parent, kind, times, and links
/// are unchanged, so the copy joins the same tree as the operator's.
fn tenant_copy(mut span: SpanData) -> SpanData {
    span.attributes
        .retain(|attribute| safe_attribute(attribute.key.as_str()));
    span.events.events = std::mem::take(&mut span.events.events)
        .into_iter()
        .filter_map(|mut event| {
            event
                .attributes
                .retain(|attribute| attribute.key.as_str() == EXCEPTION_TYPE);
            event.name = "exception".into();
            (!event.attributes.is_empty()).then_some(event)
        })
        .collect();
    if matches!(span.status, Status::Error { .. }) {
        span.status = Status::error("");
    }
    span
}

#[cfg(test)]
mod tests {
    //! The sampler, the operator filter, routing, and the tenant filter
    //! against a real SDK provider with recording processors in place of
    //! exporters.

    use std::sync::{Arc, Mutex, PoisonError};
    use std::time::Duration;

    use opentelemetry::trace::{
        SamplingDecision, Span as _, SpanContext, SpanId, SpanKind, Status, TraceContextExt as _,
        TraceFlags, TraceId, TraceState, Tracer as _, TracerProvider as _,
    };
    use opentelemetry::{Context, KeyValue};
    use opentelemetry_proto::tonic::trace::v1::ResourceSpans;
    use opentelemetry_sdk::Resource;
    use opentelemetry_sdk::error::OTelSdkResult;
    use opentelemetry_sdk::trace::{Sampler, ShouldSample as _, Span, SpanData, SpanProcessor};
    use wyrd_spec::DataTenantId;
    use wyrd_spec::auth::{Attribution, PrincipalId};

    use super::{
        OperatorExport, ROUTES, RequestSink, ServerSampler, TenantSpanRoute, TenantSpanSink,
        tracer_provider,
    };

    /// Records every span it is handed, standing in for an operator exporter.
    #[derive(Debug, Clone, Default)]
    struct Recorder(Arc<Mutex<Vec<SpanData>>>);

    impl Recorder {
        /// Names of the recorded spans of trace `trace_id`, in end order.
        fn names(&self, trace_id: TraceId) -> Vec<String> {
            self.0
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .iter()
                .filter(|span| span.span_context.trace_id() == trace_id)
                .map(|span| span.name.to_string())
                .collect()
        }
    }

    impl SpanProcessor for Recorder {
        /// Starts are not recorded.
        fn on_start(&self, _span: &mut Span, _cx: &Context) {}

        /// Record the ended span.
        fn on_end(&self, span: SpanData) {
            self.0
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push(span);
        }

        /// Nothing to flush.
        ///
        /// # Errors
        ///
        /// Never fails.
        fn force_flush(&self) -> OTelSdkResult {
            Ok(())
        }

        /// Nothing to shut down.
        ///
        /// # Errors
        ///
        /// Never fails.
        fn shutdown_with_timeout(&self, _timeout: Duration) -> OTelSdkResult {
            Ok(())
        }
    }

    /// One staged copy as `(route, name, span id, parent span id, attribute
    /// keys)`.
    type StagedCopy = (TenantSpanRoute, String, Vec<u8>, Vec<u8>, Vec<String>);

    /// Records every staged tenant copy with its route.
    #[derive(Default)]
    struct Staged(Mutex<Vec<(TenantSpanRoute, ResourceSpans)>>);

    impl Staged {
        /// The staged copies of trace `trace_id`, in end order.
        fn of(&self, trace_id: TraceId) -> Vec<StagedCopy> {
            self.0
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .iter()
                .flat_map(|(route, resource)| {
                    resource
                        .scope_spans
                        .iter()
                        .flat_map(|scope| &scope.spans)
                        .map(move |span| (route, span))
                })
                .filter(|(_, span)| span.trace_id == trace_id.to_bytes())
                .map(|(route, span)| {
                    (
                        *route,
                        span.name.clone(),
                        span.span_id.clone(),
                        span.parent_span_id.clone(),
                        span.attributes.iter().map(|kv| kv.key.clone()).collect(),
                    )
                })
                .collect()
        }
    }

    impl TenantSpanSink for Staged {
        /// Record the copy.
        fn stage(&self, route: TenantSpanRoute, span: ResourceSpans) {
            self.0
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push((route, span));
        }
    }

    /// The sink every test's request edge binds; tests tell their copies
    /// apart by trace id.
    fn staged() -> Arc<Staged> {
        static STAGED: std::sync::OnceLock<Arc<Staged>> = std::sync::OnceLock::new();
        Arc::clone(STAGED.get_or_init(Arc::default))
    }

    /// A client's remote span context in a fresh trace, sampled or not.
    fn remote(sampled: bool) -> SpanContext {
        SpanContext::new(
            TraceId::from(rand_u128()),
            SpanId::from(0x00f0_67aa_0ba9_02b7),
            if sampled {
                TraceFlags::SAMPLED
            } else {
                TraceFlags::default()
            },
            true,
            TraceState::from_key_value([("vendor", "client")]).expect("valid trace state"),
        )
    }

    /// A process-unique nonzero value, so each test's trace is its own.
    fn rand_u128() -> u128 {
        use opentelemetry_sdk::trace::IdGenerator as _;
        u128::from_be_bytes(
            opentelemetry_sdk::trace::RandomIdGenerator::default()
                .new_trace_id()
                .to_bytes(),
        )
    }

    /// The sampling decision for a span under `parent`, with operator
    /// `policy`.
    fn decide(policy: Sampler, parent: Option<&Context>) -> SamplingDecision {
        ServerSampler::new(policy)
            .should_sample(
                parent,
                TraceId::from(1_u128),
                "request",
                &SpanKind::Server,
                &[],
                &[],
            )
            .decision
    }

    /// Under a remote parent, the operator decision and the client's sampled
    /// flag combine into the four matrix rows.
    #[test]
    fn remote_parent_rows_combine_operator_and_client_decisions() {
        for (policy, sampled, expected) in [
            (Sampler::AlwaysOn, true, SamplingDecision::RecordAndSample),
            (Sampler::AlwaysOn, false, SamplingDecision::RecordAndSample),
            (Sampler::AlwaysOff, true, SamplingDecision::RecordOnly),
            (Sampler::AlwaysOff, false, SamplingDecision::Drop),
        ] {
            let parent = Context::new().with_remote_span_context(remote(sampled));
            assert_eq!(
                decide(policy.clone(), Some(&parent)),
                expected,
                "{policy:?} sampled={sampled}"
            );
        }
    }

    /// Every decision under a parent keeps the parent's trace state, so the
    /// live context propagates it; a root starts with none.
    #[test]
    fn sampling_keeps_the_parents_trace_state() {
        for policy in [Sampler::AlwaysOn, Sampler::AlwaysOff] {
            let parent = Context::new().with_remote_span_context(remote(true));
            let result = ServerSampler::new(policy.clone()).should_sample(
                Some(&parent),
                TraceId::from(1_u128),
                "request",
                &SpanKind::Server,
                &[],
                &[],
            );
            assert_eq!(result.trace_state.header(), "vendor=client", "{policy:?}");
        }
        let root = ServerSampler::new(Sampler::AlwaysOn).should_sample(
            None,
            TraceId::from(1_u128),
            "request",
            &SpanKind::Server,
            &[],
            &[],
        );
        assert_eq!(root.trace_state.header(), "");
    }

    /// A remote parent whose trace id the ratio would sample does not make
    /// the operator sample it; the decision uses a server-drawn id.
    #[test]
    fn client_trace_id_does_not_change_the_operator_decision() {
        let policy = Sampler::TraceIdRatioBased(1e-12);
        let low = TraceId::from(1_u128);
        assert_eq!(
            ServerSampler::new(policy.clone())
                .should_sample(None, low, "request", &SpanKind::Server, &[], &[])
                .decision,
            SamplingDecision::RecordAndSample,
            "the policy samples the low trace id when the server chose it"
        );
        let parent = Context::new().with_remote_span_context(SpanContext::new(
            low,
            SpanId::from(7_u64),
            TraceFlags::SAMPLED,
            true,
            TraceState::default(),
        ));
        for _ in 0..64 {
            assert_eq!(
                decide(policy.clone(), Some(&parent)),
                SamplingDecision::RecordOnly
            );
        }
    }

    /// A span without a parent is decided by the operator policy alone.
    #[test]
    fn root_follows_the_operator_policy() {
        assert_eq!(
            decide(Sampler::AlwaysOn, None),
            SamplingDecision::RecordAndSample
        );
        assert_eq!(decide(Sampler::AlwaysOff, None), SamplingDecision::Drop);
        assert_eq!(
            decide(Sampler::AlwaysOn, Some(&Context::new())),
            SamplingDecision::RecordAndSample
        );
    }

    /// The operator filter passes on sampled spans without their trace state
    /// and withholds recorded-only ones.
    #[test]
    fn operator_filter_admits_only_sampled_spans() {
        let recorder = Recorder::default();
        let filter = OperatorExport(recorder.clone());
        let trace_id = TraceId::from(rand_u128());
        for (name, flags) in [
            ("sampled", TraceFlags::SAMPLED),
            ("recorded", TraceFlags::default()),
        ] {
            filter.on_end(SpanData {
                span_context: SpanContext::new(
                    trace_id,
                    SpanId::from(1_u64),
                    flags,
                    false,
                    TraceState::from_key_value([("vendor", "client")]).expect("valid trace state"),
                ),
                parent_span_id: SpanId::INVALID,
                parent_span_is_remote: false,
                span_kind: SpanKind::Internal,
                name: name.into(),
                start_time: std::time::SystemTime::UNIX_EPOCH,
                end_time: std::time::SystemTime::UNIX_EPOCH,
                attributes: Vec::new(),
                dropped_attributes_count: 0,
                events: opentelemetry_sdk::trace::SpanEvents::default(),
                links: opentelemetry_sdk::trace::SpanLinks::default(),
                status: Status::Unset,
                instrumentation_scope: opentelemetry::InstrumentationScope::default(),
            });
        }
        assert_eq!(recorder.names(trace_id), ["sampled"]);
        assert!(
            recorder
                .0
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .iter()
                .all(|span| span.span_context.trace_state().header().is_empty()),
            "the operator copy carries no client trace state"
        );
    }

    /// One continued request: a bound root with a catalog child and a
    /// non-catalog `gateway.attempt` child whose own child the catalog names,
    /// under `policy`, with or without an operator.
    ///
    /// Returns the trace id, the operator's span names, and the staged copies.
    fn continued_request(
        policy: Sampler,
        sampled: bool,
        operator: bool,
        bind: bool,
    ) -> (TraceId, Vec<String>, Vec<StagedCopy>) {
        let staged = staged();
        let recorder = Recorder::default();
        let provider = tracer_provider(
            policy,
            Resource::builder_empty()
                .with_attributes([
                    KeyValue::new("service.name", "wyrd"),
                    KeyValue::new("service.instance.id", "internal"),
                ])
                .build(),
            operator.then(|| OperatorExport(recorder.clone())),
        );
        let tracer = provider.tracer("wyrd");
        let parent = remote(sampled);
        let trace_id = parent.trace_id();
        let caller = Context::new()
            .with_remote_span_context(parent)
            .with_value(RequestSink(Arc::clone(&staged) as Arc<dyn TenantSpanSink>));
        let root = tracer
            .span_builder("request")
            .with_kind(SpanKind::Server)
            .with_attributes([
                KeyValue::new("rpc.system", "grpc"),
                KeyValue::new("path", "/v1/secret"),
            ])
            .start_with_context(&tracer, &caller);
        if bind {
            ROUTES.bind(
                root.span_context(),
                TenantSpanRoute {
                    tenant: DataTenantId::new_v7(),
                    attribution: Attribution::direct(
                        "01900000-0000-7000-8000-000000000007"
                            .parse::<PrincipalId>()
                            .expect("a principal id"),
                    ),
                },
            );
        }
        let request = caller.with_span(root);
        let mut child = tracer
            .span_builder("verification.execute")
            .with_attributes([
                KeyValue::new("wyrd.run_id", "run"),
                KeyValue::new("db.statement", "SELECT 1"),
            ])
            .start_with_context(&tracer, &request);
        child.add_event(
            "the engine failed: secret detail",
            vec![
                KeyValue::new("exception.type", "EngineError"),
                KeyValue::new("exception.message", "secret detail"),
            ],
        );
        child.set_status(Status::error("secret detail"));
        child.end();
        let attempt = request.with_span(tracer.start_with_context("gateway.attempt", &request));
        tracer
            .start_with_context("gateway.translate", &attempt)
            .end();
        attempt.span().end();
        request.span().end();
        (trace_id, recorder.names(trace_id), staged.of(trace_id))
    }

    /// The four sampling rows, with and without an operator: the operator
    /// receives every span exactly when it accepts, and the tenant receives
    /// the catalog spans exactly when the client sampled, never one below a
    /// span outside the catalog.
    #[test]
    fn sampling_matrix_holds_with_and_without_an_operator() {
        for operator in [true, false] {
            for (policy, sampled, operator_sees, tenant_sees) in [
                (Sampler::AlwaysOn, true, true, true),
                (Sampler::AlwaysOn, false, true, false),
                (Sampler::AlwaysOff, true, false, true),
                (Sampler::AlwaysOff, false, false, false),
            ] {
                let row = format!("{policy:?} sampled={sampled} operator={operator}");
                let (_, exported, staged) = continued_request(policy, sampled, operator, true);
                let expected: &[&str] = if operator && operator_sees {
                    &[
                        "verification.execute",
                        "gateway.translate",
                        "gateway.attempt",
                        "request",
                    ]
                } else {
                    &[]
                };
                assert_eq!(exported, expected, "{row}");
                let names = staged
                    .iter()
                    .map(|(_, name, ..)| name.as_str())
                    .collect::<Vec<_>>();
                let expected: &[&str] = if tenant_sees {
                    &["verification.execute", "request"]
                } else {
                    &[]
                };
                assert_eq!(names, expected, "{row}");
            }
        }
    }

    /// A tenant copy keeps the operator's ids and parent, carries only safe
    /// attributes and the exception type, and is staged under the bound
    /// route.
    #[test]
    fn tenant_copy_keeps_ids_and_only_safe_fields() {
        let (_, _, staged) = continued_request(Sampler::AlwaysOn, true, true, true);
        let [
            (child_route, _, child_id, child_parent, child_keys),
            (root_route, _, root_id, _, root_keys),
        ] = staged.as_slice()
        else {
            panic!("expected the child and the root, got {staged:?}");
        };
        assert_eq!(child_parent, root_id);
        assert_ne!(child_id, root_id);
        assert_eq!(child_route, root_route);
        assert_eq!(child_keys, &["wyrd.run_id"]);
        assert_eq!(root_keys, &["rpc.system"]);
    }

    /// Spans of a request that was never bound stay operator-only.
    #[test]
    fn unbound_request_is_operator_only() {
        let (_, exported, staged) = continued_request(Sampler::AlwaysOn, true, true, false);
        assert_eq!(exported.len(), 4);
        assert!(staged.is_empty(), "{staged:?}");
    }

    /// A sampled remote link records a span the operator rejects; an
    /// unsampled one does not.
    #[test]
    fn a_sampled_link_records_for_the_tenant() {
        for (sampled, expected) in [
            (true, SamplingDecision::RecordOnly),
            (false, SamplingDecision::Drop),
        ] {
            let link = opentelemetry::trace::Link::with_context(remote(sampled));
            let decision = ServerSampler::new(Sampler::AlwaysOff)
                .should_sample(
                    None,
                    TraceId::from(1_u128),
                    "bifrost.gate.write",
                    &SpanKind::Consumer,
                    &[],
                    &[link],
                )
                .decision;
            assert_eq!(decision, expected, "sampled={sampled}");
        }
    }

    /// A queued write's consumer under an untraced request opens its own
    /// route from a sampled producer link: the tenant receives the consumer
    /// and its catalog child, not the request span, with the operator's ids
    /// and parents; without a sampled link nothing is staged.
    #[test]
    fn a_sampled_producer_link_routes_the_consumer() {
        let provider = tracer_provider(
            Sampler::AlwaysOff,
            Resource::builder_empty().build(),
            None::<OperatorExport<Recorder>>,
        );
        let tracer = provider.tracer("wyrd");
        for sampled in [true, false] {
            let staged = Arc::new(Staged::default());
            let edge = Context::new()
                .with_value(RequestSink(Arc::clone(&staged) as Arc<dyn TenantSpanSink>));
            let request = edge.with_span(tracer.start_with_context("request", &edge));
            let consumer = tracer
                .span_builder("bifrost.gate.write")
                .with_kind(SpanKind::Consumer)
                .with_links(vec![opentelemetry::trace::Link::with_context(remote(
                    sampled,
                ))])
                .start_with_context(&tracer, &request);
            let request_id = request.span().span_context().span_id();
            let write = request.with_span(consumer);
            ROUTES.bind(
                write.span().span_context(),
                TenantSpanRoute {
                    tenant: DataTenantId::new_v7(),
                    attribution: Attribution::direct(
                        "01900000-0000-7000-8000-000000000007"
                            .parse::<PrincipalId>()
                            .expect("a principal id"),
                    ),
                },
            );
            let trace_id = write.span().span_context().trace_id();
            let write_id = write.span().span_context().span_id();
            tracer.start_with_context("dispatch", &write).end();
            write.span().end();
            request.span().end();
            let copies = staged
                .of(trace_id)
                .into_iter()
                .map(|(_, name, _, parent, _)| (name, parent))
                .collect::<Vec<_>>();
            let expected = if sampled {
                vec![
                    ("dispatch".to_owned(), write_id.to_bytes().to_vec()),
                    (
                        "bifrost.gate.write".to_owned(),
                        request_id.to_bytes().to_vec(),
                    ),
                ]
            } else {
                Vec::new()
            };
            assert_eq!(copies, expected, "sampled={sampled}");
        }
    }

    /// Each request's copies stage on the sink its own server bound, so two
    /// servers in one process never receive each other's spans.
    #[test]
    fn each_request_stages_on_the_sink_it_bound() {
        let provider = tracer_provider(
            Sampler::AlwaysOn,
            Resource::builder_empty().build(),
            None::<OperatorExport<Recorder>>,
        );
        let tracer = provider.tracer("wyrd");
        let servers = [Arc::new(Staged::default()), Arc::new(Staged::default())];
        let traces = servers.each_ref().map(|server| {
            let parent = remote(true);
            let trace_id = parent.trace_id();
            let mut root = tracer.span_builder("request").start_with_context(
                &tracer,
                &Context::new()
                    .with_remote_span_context(parent)
                    .with_value(RequestSink(Arc::clone(server) as Arc<dyn TenantSpanSink>)),
            );
            ROUTES.bind(
                root.span_context(),
                TenantSpanRoute {
                    tenant: DataTenantId::new_v7(),
                    attribution: Attribution::direct(
                        "01900000-0000-7000-8000-000000000007"
                            .parse::<PrincipalId>()
                            .expect("a principal id"),
                    ),
                },
            );
            root.end();
            trace_id
        });
        for (server, own) in servers.iter().zip(traces) {
            assert_eq!(
                server
                    .0
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .len(),
                1
            );
            assert_eq!(server.of(own).len(), 1);
        }
    }

    /// An operation that binds its own pair stages itself and the spans it
    /// opens under that pair on the request's sink; the request and its
    /// other spans keep the request's pair.
    ///
    /// # Panics
    ///
    /// Panics when a copy is staged under the other span's pair.
    #[test]
    fn an_operation_route_rebinds_only_its_subtree() {
        let provider = tracer_provider(
            Sampler::AlwaysOn,
            Resource::builder_empty().build(),
            None::<OperatorExport<Recorder>>,
        );
        let tracer = provider.tracer("wyrd");
        let staged = Arc::new(Staged::default());
        let parent = remote(true);
        let trace_id = parent.trace_id();
        let principal = |n: u8| {
            format!("01900000-0000-7000-8000-00000000000{n}")
                .parse::<PrincipalId>()
                .expect("a principal id")
        };
        let tenant = DataTenantId::new_v7();
        let request_pair = Attribution::new(principal(1), principal(2));
        let judge_pair = Attribution::new(principal(1), principal(3));
        let edge = Context::new()
            .with_remote_span_context(parent)
            .with_value(RequestSink(Arc::clone(&staged) as Arc<dyn TenantSpanSink>));
        let request = edge.with_span(tracer.start_with_context("request", &edge));
        ROUTES.bind(
            request.span().span_context(),
            TenantSpanRoute {
                tenant,
                attribution: request_pair,
            },
        );
        let judge = request.with_span(tracer.start_with_context("agent.gateway.call", &request));
        ROUTES.rebind(
            judge.span().span_context(),
            TenantSpanRoute {
                tenant,
                attribution: judge_pair,
            },
        );
        tracer.start_with_context("gateway.invoke", &judge).end();
        judge.span().end();
        tracer
            .start_with_context("verification.result", &request)
            .end();
        request.span().end();

        let copies = staged
            .of(trace_id)
            .into_iter()
            .map(|(route, name, ..)| (name, route.attribution))
            .collect::<Vec<_>>();
        assert_eq!(
            copies,
            vec![
                ("gateway.invoke".to_owned(), judge_pair),
                ("agent.gateway.call".to_owned(), judge_pair),
                ("verification.result".to_owned(), request_pair),
                ("request".to_owned(), request_pair),
            ]
        );
    }

    /// An ended span leaves the registry, so routes live only as long as
    /// their spans.
    #[test]
    fn ended_spans_leave_the_registry() {
        let (trace_id, ..) = continued_request(Sampler::AlwaysOn, true, true, true);
        let live = ROUTES.live.lock().unwrap_or_else(PoisonError::into_inner);
        assert!(!live.keys().any(|(trace, _)| *trace == trace_id));
    }

    /// The tenant filter drops event detail, error descriptions, and unsafe
    /// attributes.
    #[test]
    fn tenant_filter_drops_detail() {
        let mut span = SpanData {
            span_context: SpanContext::empty_context(),
            parent_span_id: SpanId::INVALID,
            parent_span_is_remote: false,
            span_kind: SpanKind::Internal,
            name: "gateway.call".into(),
            start_time: std::time::SystemTime::UNIX_EPOCH,
            end_time: std::time::SystemTime::UNIX_EPOCH,
            attributes: vec![
                KeyValue::new("gen_ai.request.model", "gpt"),
                KeyValue::new("server.address", "10.0.0.1"),
                KeyValue::new("tenant", "other"),
            ],
            dropped_attributes_count: 0,
            events: opentelemetry_sdk::trace::SpanEvents::default(),
            links: opentelemetry_sdk::trace::SpanLinks::default(),
            status: Status::error("key sk-123 rejected"),
            instrumentation_scope: opentelemetry::InstrumentationScope::default(),
        };
        span.events.events.push(opentelemetry::trace::Event::new(
            "plain log line",
            std::time::SystemTime::UNIX_EPOCH,
            vec![KeyValue::new("message", "detail")],
            0,
        ));
        let copy = super::tenant_copy(span);
        assert_eq!(
            copy.attributes,
            [KeyValue::new("gen_ai.request.model", "gpt")]
        );
        assert!(copy.events.events.is_empty());
        assert_eq!(copy.status, Status::error(""));
    }

    /// The catalog admits behavior spans and excludes attempts and internals.
    ///
    /// # Panics
    /// Panics when a span name is misclassified by the catalog.
    #[test]
    fn catalog_names_only_behavior_spans() {
        for name in [
            "request",
            "dispatch",
            "agent.gateway.call",
            "verification.judge",
            "invoke_agent",
            "chat",
            "execute_tool",
            "gateway.call",
            "bifrost.gate.write",
            "bifrost.oracle.query",
            "otlp.http.trace.export",
        ] {
            assert!(super::in_catalog(name), "{name}");
        }
        for name in [
            "gateway.attempt",
            "bifrost.oracle.admission",
            "bifrost.scribe.wal.append",
            "storage.get",
            "sqlx.query",
        ] {
            assert!(!super::in_catalog(name), "{name}");
        }
    }
}
