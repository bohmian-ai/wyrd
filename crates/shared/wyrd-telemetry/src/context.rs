//! W3C trace context at the server's request edges.
//!
//! An inbound request names its caller's span in a `traceparent` header (HTTP)
//! or metadata entry (gRPC, carried as an HTTP/2 header). [`inbound_context`]
//! parses it once for every transport, [`continue_remote`] makes it the parent
//! of the server's request span so everything the request does descends from
//! the caller, and [`current_context`] reads back the context of whatever span
//! is current, for work that forwards it. [`continued_context`] reads it only
//! inside a request that continued a caller's trace. Deferred work never adopts a cause as
//! its parent; [`crate::link_remote_span`] records it as a span link instead.

use http::HeaderMap;
use opentelemetry::trace::{
    SpanContext, SpanId, TraceContextExt as _, TraceFlags, TraceId, TraceState,
};
use tracing_opentelemetry::OpenTelemetrySpanExt as _;
use tracing_subscriber::registry::{LookupSpan as _, Registry};
use wyrd_spec::trace::TraceContext;

/// W3C header carrying the caller's trace and parent span.
const TRACEPARENT: &str = "traceparent";
/// W3C header carrying vendor trace state.
const TRACESTATE: &str = "tracestate";

/// Parse the caller's trace context from request `headers`.
///
/// W3C trace context treats a repeated `traceparent` as invalid, so only a
/// single parseable value is accepted; a missing, malformed, or repeated one
/// yields `None`, which a request treats as "start a new trace", never as an
/// error. Repeated `tracestate` headers are combined into one list in order.
/// The combined list must parse as W3C `tracestate` (OpenTelemetry's
/// [`TraceState`] parser); otherwise the state is dropped and the valid parent
/// is kept with empty state, as W3C requires a vendor list that fails to parse
/// to be discarded independently.
#[must_use]
pub fn inbound_context(headers: &HeaderMap) -> Option<TraceContext> {
    let mut parents = headers.get_all(TRACEPARENT).iter();
    let parent = parents.next()?.to_str().ok()?;
    if parents.next().is_some() {
        return None;
    }
    let state = headers
        .get_all(TRACESTATE)
        .iter()
        .map(|value| value.to_str().ok())
        .collect::<Option<Vec<_>>>()
        .map(|values| values.join(","))
        .filter(|state| state.parse::<TraceState>().is_ok())
        .unwrap_or_default();
    TraceContext::parse_traceparent(parent, state).ok()
}

/// Extension on a request span that continued a caller's trace.
///
/// It lives in the `tracing` span tree, so every span opened inside the
/// request finds it among its ancestors, including spans on tasks the
/// request spawned inside its span.
struct Continued;

/// Make the caller's span `context` the OpenTelemetry parent of `span`.
///
/// `span` then joins the caller's trace as a child of its span, and every span
/// opened inside it descends from the caller. The parent is marked remote,
/// which the process sampler reads to keep the operator's decision independent
/// of the caller's flags, and `span` is marked as continued for
/// [`continued_context`]. This must run before `span` is first entered; on a
/// started or disabled span, or without an OpenTelemetry layer, it does
/// nothing.
pub fn continue_remote(span: &tracing::Span, context: &TraceContext) {
    let Some(remote) = remote_span_context(context) else {
        return;
    };
    // A span already started keeps its own trace; continuing is best-effort
    // observability and never fails the request.
    if span
        .set_parent(opentelemetry::Context::new().with_remote_span_context(remote))
        .is_ok()
    {
        span.with_subscriber(|(id, dispatch)| {
            if let Some(span) = dispatch
                .downcast_ref::<Registry>()
                .and_then(|registry| registry.span(id))
            {
                span.extensions_mut().insert(Continued);
            }
        });
    }
}

/// The current span's trace context, only inside a request that continued a
/// caller's trace.
///
/// A span continues the caller's trace when it or an ancestor was passed to
/// [`continue_remote`], whatever the caller's sampled flag. Work that names
/// the caller's position only for a continued trace (a captured gateway
/// attempt's parent) reads it here; elsewhere it is `None`.
#[must_use]
pub fn continued_context() -> Option<TraceContext> {
    let continued = tracing::Span::current()
        .with_subscriber(|(id, dispatch)| {
            dispatch
                .downcast_ref::<Registry>()
                .and_then(|registry| registry.span(id))
                .is_some_and(|span| {
                    span.scope()
                        .any(|ancestor| ancestor.extensions().get::<Continued>().is_some())
                })
        })
        .unwrap_or(false);
    continued.then(current_context).flatten()
}

/// The trace context of the current `tracing` span, when it has a valid one.
///
/// Work that forwards the caller's position in the trace (a gateway call, a
/// capture parent) reads it here rather than threading the inbound header
/// through every call. `None` without an OpenTelemetry layer or outside any
/// recorded span.
#[must_use]
pub fn current_context() -> Option<TraceContext> {
    let context = tracing::Span::current().context();
    let span = context.span();
    let span_context = span.span_context();
    span_context.is_valid().then(|| TraceContext {
        trace_id: span_context.trace_id().to_string(),
        span_id: span_context.span_id().to_string(),
        trace_flags: span_context.trace_flags().to_u8(),
        trace_state: span_context.trace_state().header(),
    })
}

/// Convert a caller's wire trace context into a remote OpenTelemetry span
/// context.
///
/// `None` when an id is not valid hex of the right width or is all zeros. A
/// state that does not parse is dropped, keeping the ids.
fn remote_span_context(context: &TraceContext) -> Option<SpanContext> {
    let trace_id = TraceId::from_hex(&context.trace_id).ok()?;
    let span_id = SpanId::from_hex(&context.span_id).ok()?;
    let span_context = SpanContext::new(
        trace_id,
        span_id,
        TraceFlags::new(context.trace_flags),
        true,
        context.trace_state.parse().unwrap_or_default(),
    );
    span_context.is_valid().then_some(span_context)
}

#[cfg(test)]
mod tests {
    use http::{HeaderMap, HeaderValue};

    use opentelemetry::trace::TracerProvider as _;
    use tracing_subscriber::layer::SubscriberExt as _;
    use wyrd_spec::trace::TraceContext;

    use super::{continue_remote, continued_context, inbound_context};

    /// A valid caller `traceparent`.
    const PARENT: &str = "00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01";

    /// Headers holding each `(name, value)` pair in order.
    fn headers(pairs: &[(&'static str, &str)]) -> HeaderMap {
        let mut headers = HeaderMap::new();
        for (name, value) in pairs {
            headers.append(*name, HeaderValue::from_str(value).expect("header value"));
        }
        headers
    }

    /// A valid `traceparent` with repeated `tracestate` headers parses into
    /// one context carrying the combined state.
    #[test]
    fn valid_traceparent_is_the_inbound_context() {
        let context = inbound_context(&headers(&[
            ("traceparent", PARENT),
            ("tracestate", "a=1"),
            ("tracestate", "b=2"),
        ]))
        .expect("context parses");
        assert_eq!(
            (
                context.trace_id.as_str(),
                context.span_id.as_str(),
                context.trace_state.as_str()
            ),
            (
                "0af7651916cd43dd8448eb211c80319c",
                "b7ad6b7169203331",
                "a=1,b=2"
            )
        );
    }

    /// A malformed `tracestate`, whether in one header or one of several, is
    /// dropped while the valid parent is kept.
    #[test]
    fn malformed_tracestate_is_dropped_keeping_the_parent() {
        for state in [vec!["not a list member"], vec!["a=1", "B=2"], vec!["a=1,b"]] {
            let mut pairs = vec![("traceparent", PARENT)];
            pairs.extend(state.iter().map(|value| ("tracestate", *value)));
            let context = inbound_context(&headers(&pairs)).expect("the parent survives");
            assert_eq!(context.span_id, "b7ad6b7169203331", "{pairs:?}");
            assert_eq!(context.trace_state, "", "{pairs:?}");
        }
    }

    /// A malformed, zero, or repeated `traceparent` yields no context.
    #[test]
    fn malformed_traceparent_is_ignored() {
        for pairs in [
            vec![("traceparent", "not-a-traceparent")],
            vec![(
                "traceparent",
                "00-00000000000000000000000000000000-b7ad6b7169203331-01",
            )],
            vec![("traceparent", PARENT), ("traceparent", PARENT)],
            vec![("tracestate", "a=1")],
        ] {
            assert!(inbound_context(&headers(&pairs)).is_none(), "{pairs:?}");
        }
    }

    /// Spans inside a continued request, at any depth and whatever the
    /// caller's sampled flag, read the current context; spans of a request
    /// that did not continue a caller's trace read none.
    #[test]
    fn only_spans_of_a_continued_request_read_a_continued_context() {
        let provider = opentelemetry_sdk::trace::SdkTracerProvider::builder().build();
        let subscriber = tracing_subscriber::registry()
            .with(tracing_opentelemetry::layer().with_tracer(provider.tracer("context")));
        tracing::subscriber::with_default(subscriber, || {
            for flags in ["00", "01"] {
                let caller = TraceContext::parse_traceparent(
                    &format!("00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-{flags}"),
                    String::new(),
                )
                .expect("valid traceparent");
                let request = tracing::info_span!("request");
                continue_remote(&request, &caller);
                let work = tracing::info_span!(parent: &request, "work");
                let continued =
                    work.in_scope(|| tracing::info_span!("call").in_scope(continued_context));
                assert_eq!(
                    continued.map(|context| context.trace_id),
                    Some(caller.trace_id),
                    "flags {flags}"
                );
            }
            let untraced = tracing::info_span!("request");
            assert!(
                untraced.in_scope(continued_context).is_none(),
                "an uncontinued request reads no continued context"
            );
        });
    }
}
