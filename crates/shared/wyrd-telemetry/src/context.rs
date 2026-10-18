//! W3C trace context at the server's request edges.
//!
//! An inbound request names its caller's span in a `traceparent` header (HTTP)
//! or metadata entry (gRPC, carried as an HTTP/2 header). [`inbound_context`]
//! parses it once for every transport, [`serve_request`] makes it the parent
//! of the server's request span so everything the request does descends from
//! the caller, and [`current_context`] reads back the context of whatever span
//! is current, for work that forwards it. [`continued_context`] reads it only
//! inside a request that continued a caller's trace, and [`caller_sampled`]
//! reports the caller's own sampling decision there. Deferred work never
//! adopts a cause as its parent; [`serve_linked_work`] links it to that cause
//! instead, carrying the decision, and marks it continued the same way.

use std::sync::Arc;

use http::HeaderMap;
use opentelemetry::trace::{
    SpanContext, SpanId, TraceContextExt as _, TraceFlags, TraceId, TraceState,
};
use tracing_opentelemetry::OpenTelemetrySpanExt as _;
use tracing_subscriber::registry::{LookupSpan as _, Registry};
use wyrd_spec::trace::TraceContext;

use crate::export::{RequestSink, TenantSpanSink};

/// W3C header carrying the caller's trace and parent span.
const TRACEPARENT: &str = "traceparent";
/// W3C header carrying vendor trace state.
const TRACESTATE: &str = "tracestate";

/// Parse the caller's trace context from request `headers`.
///
/// W3C trace context treats a repeated `traceparent` as invalid, so only a
/// single parseable value is accepted; a missing, malformed, or repeated one
/// yields `None`, which a request treats as "start a new trace", never as an
/// error. Repeated `tracestate` headers are combined into one list in order;
/// [`trace_state`] validates it, and a list that fails is dropped while the
/// valid parent is kept with empty state, as W3C requires a vendor list that
/// fails to parse to be discarded independently.
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
        .and_then(|values| trace_state(&values))
        .unwrap_or_default();
    TraceContext::parse_traceparent(parent, state).ok()
}

/// The most list members a W3C `tracestate` may carry.
const TRACESTATE_MEMBERS: usize = 32;

/// Validate the combined W3C `tracestate` list in `values` and return it in
/// canonical form.
///
/// Members are comma-separated with optional spaces or tabs around each, and
/// empty members are skipped, as the W3C grammar allows. Each member's key
/// and value must be valid by OpenTelemetry's [`TraceState`] rules, keys must
/// be unique, and there may be at most 32 members. `None` when any rule
/// fails; an empty list is valid and yields an empty state.
fn trace_state(values: &[&str]) -> Option<String> {
    let members = values
        .iter()
        .flat_map(|value| value.split(','))
        .map(|member| member.trim_matches([' ', '\t']))
        .filter(|member| !member.is_empty())
        .map(|member| member.split_once('='))
        .collect::<Option<Vec<_>>>()?;
    let mut keys = std::collections::HashSet::new();
    if members.len() > TRACESTATE_MEMBERS || !members.iter().all(|(key, _)| keys.insert(*key)) {
        return None;
    }
    TraceState::from_key_value(members)
        .ok()
        .map(|state| state.header())
}

/// Extension on a request span that continued a caller's trace.
///
/// It lives in the `tracing` span tree, so every span opened inside the
/// request finds it among its ancestors, including spans on tasks the
/// request spawned inside its span. It holds the caller's sampled flag,
/// which the request span's own flags do not keep: those carry the operator's
/// decision.
struct Continued {
    /// Whether the caller sampled the span the request continued.
    sampled: bool,
}

/// Make the caller's span `context` the OpenTelemetry parent of `span`.
///
/// `span` then joins the caller's trace as a child of its span, and every span
/// opened inside it descends from the caller. The parent is marked remote,
/// which the process sampler reads to keep the operator's decision independent
/// of the caller's flags, and `span` is marked as continued for
/// [`continued_context`]. This must run before `span` is first entered; on a
/// started or disabled span, or without an OpenTelemetry layer, it does
/// nothing.
///
/// The serving server's `tenant_spans` sink rides in the same parent context
/// to every span the request opens, so the request's tenant copies stage on
/// the server that served it. Without a caller, `span` stays the root of a
/// new trace.
pub fn serve_request(
    span: &tracing::Span,
    caller: Option<&TraceContext>,
    tenant_spans: Option<&Arc<dyn TenantSpanSink>>,
) {
    let remote = caller.and_then(remote_span_context);
    let mut parent = opentelemetry::Context::new();
    if let Some(remote) = remote.clone() {
        parent = parent.with_remote_span_context(remote);
    }
    if let Some(sink) = tenant_spans {
        parent = parent.with_value(RequestSink(Arc::clone(sink)));
    }
    // A span already started keeps its own trace; continuing is best-effort
    // observability and never fails the request.
    if span.set_parent(parent).is_ok()
        && let Some(remote) = remote
    {
        mark_continued(span, remote.is_sampled());
    }
}

/// Start deferred work `span` as its own trace caused by the caller's span
/// `span_id` of trace `trace_id`.
///
/// `span` stays the root of a new trace, linked to its cause through
/// [`crate::link_remote_span`] with the caller's `sampled` decision, and
/// carries the serving server's `tenant_spans` sink like a request does. It
/// is marked continued with that decision, so [`continued_context`] places
/// work inside it (a captured gateway attempt) in `span`'s trace, and
/// [`caller_sampled`] reports the original caller's decision. This must run
/// before `span` is first entered; on a started or disabled span, or without
/// an OpenTelemetry layer, it does nothing.
pub fn serve_linked_work(
    span: &tracing::Span,
    trace_id: &wyrd_spec::vala::ids::TraceId,
    span_id: &wyrd_spec::vala::ids::SpanId,
    sampled: bool,
    tenant_spans: Option<&Arc<dyn TenantSpanSink>>,
) {
    let mut parent = opentelemetry::Context::new();
    if let Some(sink) = tenant_spans {
        parent = parent.with_value(RequestSink(Arc::clone(sink)));
    }
    if span.set_parent(parent).is_ok() {
        crate::link_remote_span(span, trace_id, span_id, sampled);
        mark_continued(span, sampled);
    }
}

/// Make `parent` the parent of `child`, which timed work before `parent`
/// could start.
///
/// `child` keeps the start time it was created with and joins `parent`'s
/// trace, so a phase that must run before its parent's trace is fixed (a
/// queued attempt's claim statement) still reads back inside it. `child`
/// must not have been entered, which would have fixed its own trace; on an
/// entered or disabled span, or without an OpenTelemetry layer, this does
/// nothing. Reading `parent`'s context starts it, so its links and parent
/// must already be installed.
pub fn adopt_span(child: &tracing::Span, parent: &tracing::Span) {
    // An entered child keeps its trace; adoption is best-effort observability.
    let _ = child.set_parent(parent.context());
}

/// Mark `span` as continuing a caller whose own decision was `sampled`.
///
/// The marker lives in the `tracing` registry, so it is skipped without a
/// registry subscriber or on a disabled span.
fn mark_continued(span: &tracing::Span, sampled: bool) {
    span.with_subscriber(|(id, dispatch)| {
        if let Some(span) = dispatch
            .downcast_ref::<Registry>()
            .and_then(|registry| registry.span(id))
        {
            span.extensions_mut().insert(Continued { sampled });
        }
    });
}

/// The caller's sampled flag when the current span lies inside a request
/// that continued a caller's trace; `None` elsewhere.
///
/// Reads the nearest continued ancestor, so a span on a task the request
/// spawned inside its span reads it too.
fn continued() -> Option<bool> {
    tracing::Span::current()
        .with_subscriber(|(id, dispatch)| {
            dispatch
                .downcast_ref::<Registry>()
                .and_then(|registry| registry.span(id))
                .and_then(|span| {
                    span.scope().find_map(|ancestor| {
                        ancestor.extensions().get::<Continued>().map(|c| c.sampled)
                    })
                })
        })
        .flatten()
}

/// The current span's trace context, only inside a request that continued a
/// caller's trace.
///
/// A span continues the caller's trace when it or an ancestor was passed to
/// [`serve_request`] with a caller, whatever the caller's sampled flag. Work that names
/// the caller's position only for a continued trace (a captured gateway
/// attempt's parent) reads it here; elsewhere it is `None`.
#[must_use]
pub fn continued_context() -> Option<TraceContext> {
    continued().and_then(|_| current_context())
}

/// Whether the current span lies inside a request that continued a caller
/// span the caller sampled.
///
/// This is the caller's decision, not the operator's: work the request
/// defers records it with its cause so the later work's link asks for the
/// same tenant copy the request received. `false` outside a continued
/// request.
#[must_use]
pub fn caller_sampled() -> bool {
    continued().unwrap_or(false)
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

    use super::{
        caller_sampled, continued_context, inbound_context, serve_linked_work, serve_request,
    };

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

    /// Spaces and tabs around members and empty members are W3C-valid and
    /// normalize away.
    #[test]
    fn tracestate_whitespace_and_empty_members_normalize() {
        let context = inbound_context(&headers(&[
            ("traceparent", PARENT),
            ("tracestate", "a=1 ,\t b=2,,"),
            ("tracestate", " c=3"),
        ]))
        .expect("the parent parses");
        assert_eq!(context.trace_state, "a=1,b=2,c=3");
    }

    /// A malformed `tracestate`, whether in one header or one of several, is
    /// dropped while the valid parent is kept.
    #[test]
    fn malformed_tracestate_is_dropped_keeping_the_parent() {
        let crowded = (0..=32)
            .map(|i| format!("k{i}=v"))
            .collect::<Vec<_>>()
            .join(",");
        for state in [
            vec!["not a list member"],
            vec!["a=1", "B=2"],
            vec!["a=1,b"],
            vec!["a=1", "a=2"],
            vec!["a==1"],
            vec![crowded.as_str()],
        ] {
            let mut pairs = vec![("traceparent", PARENT)];
            pairs.extend(state.iter().map(|value| ("tracestate", *value)));
            let context = inbound_context(&headers(&pairs)).expect("the parent survives");
            assert_eq!(context.span_id, "b7ad6b7169203331", "{pairs:?}");
            assert_eq!(context.trace_state, "", "{pairs:?}");
        }
    }

    /// A later-version `traceparent` continues the trace from its first four
    /// fields, extra fields included.
    #[test]
    fn future_version_traceparent_is_read_forward_compatibly() {
        for parent in [
            "01-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01",
            "cc-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01-what-the-future-holds",
        ] {
            let context =
                inbound_context(&headers(&[("traceparent", parent)])).expect("future version");
            assert_eq!(
                (
                    context.trace_id.as_str(),
                    context.span_id.as_str(),
                    context.trace_flags
                ),
                ("0af7651916cd43dd8448eb211c80319c", "b7ad6b7169203331", 1),
                "{parent}"
            );
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
            vec![(
                "traceparent",
                "ff-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01",
            )],
            vec![(
                "traceparent",
                "00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01-extra",
            )],
            vec![(
                "traceparent",
                "01-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01x",
            )],
            vec![("tracestate", "a=1")],
        ] {
            assert!(inbound_context(&headers(&pairs)).is_none(), "{pairs:?}");
        }
    }

    /// Spans inside a continued request, at any depth and whatever the
    /// caller's sampled flag, read the current context and the caller's own
    /// sampled flag; spans of a request that did not continue a caller's
    /// trace read none and report the caller unsampled.
    ///
    /// # Panics
    /// Panics when a fixed traceparent fails to parse or a span reads the
    /// wrong continued context or sampled flag.
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
                serve_request(&request, Some(&caller), None);
                let work = tracing::info_span!(parent: &request, "work");
                let (continued, sampled) = work.in_scope(|| {
                    tracing::info_span!("call").in_scope(|| (continued_context(), caller_sampled()))
                });
                assert_eq!(
                    continued.map(|context| context.trace_id),
                    Some(caller.trace_id),
                    "flags {flags}"
                );
                assert_eq!(sampled, flags == "01", "flags {flags}");
            }
            let untraced = tracing::info_span!("request");
            assert!(
                untraced.in_scope(continued_context).is_none(),
                "an uncontinued request reads no continued context"
            );
            assert!(
                !untraced.in_scope(caller_sampled),
                "an uncontinued request has no sampled caller"
            );
        });
    }

    /// Linked deferred work keeps a trace of its own but reads as continued
    /// inside it: its spans read that new trace's context and the original
    /// caller's sampled flag.
    ///
    /// # Panics
    /// Panics when a fixed trace or span id fails to parse, linked work reads
    /// no continued context, reuses the cause's trace id, or loses the
    /// caller's sampled flag.
    #[test]
    fn linked_work_continues_its_own_trace_with_the_callers_decision() {
        let provider = opentelemetry_sdk::trace::SdkTracerProvider::builder().build();
        let subscriber = tracing_subscriber::registry()
            .with(tracing_opentelemetry::layer().with_tracer(provider.tracer("context")));
        let cause = wyrd_spec::vala::ids::TraceId::from_hex("0af7651916cd43dd8448eb211c80319c")
            .expect("trace id");
        let span_id = wyrd_spec::vala::ids::SpanId::from_hex("b7ad6b7169203331").expect("span id");
        tracing::subscriber::with_default(subscriber, || {
            for sampled in [false, true] {
                let attempt = tracing::info_span!("attempt");
                serve_linked_work(&attempt, &cause, &span_id, sampled, None);
                let (continued, caller) = attempt.in_scope(|| {
                    tracing::info_span!("call").in_scope(|| (continued_context(), caller_sampled()))
                });
                let continued = continued.expect("linked work reads as continued");
                assert_ne!(continued.trace_id, cause.to_hex(), "sampled {sampled}");
                assert_eq!(caller, sampled);
            }
        });
    }
}
