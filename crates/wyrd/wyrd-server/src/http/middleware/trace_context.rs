//! Inbound W3C trace context.
//!
//! [`attach_trace_context`] reads a request's `traceparent` and `tracestate`
//! headers and makes the parsed context the active one for the rest of that
//! request's task, so work done on the request's behalf — a gateway call, or
//! a judge call a direct verification makes in process — joins the caller's
//! trace through [`active`]. A missing or malformed `traceparent` leaves no
//! active context; it is never an error. A malformed `tracestate` is dropped
//! on its own, keeping a valid parent. A transport that serves the request
//! on a task of its own — the `/mcp` service spawns one per request — scopes
//! that task again with [`within`].

use axum::extract::Request;
use axum::http::HeaderMap;
use axum::middleware::Next;
use axum::response::Response;
use opentelemetry::trace::TraceState;
use wyrd_spec::trace::TraceContext;

tokio::task_local! {
    /// The caller's trace context for the request this task serves.
    static INBOUND: TraceContext;
}

/// W3C header carrying the caller's trace and parent span.
const TRACEPARENT: &str = "traceparent";
/// W3C header carrying vendor trace state.
const TRACESTATE: &str = "tracestate";

/// Run the rest of the request with its valid inbound trace context active.
///
/// A request without exactly one valid `traceparent` runs unchanged.
pub async fn attach_trace_context(request: Request, next: Next) -> Response {
    within(inbound(request.headers()), next.run(request)).await
}

/// Run `future` with `context` as the active inbound trace context.
///
/// `None` runs `future` unchanged, so an untraced request stays untraced.
pub(crate) async fn within<F: Future>(context: Option<TraceContext>, future: F) -> F::Output {
    match context {
        Some(context) => INBOUND.scope(context, future).await,
        None => future.await,
    }
}

/// The trace context active on the current task, if its request carried one.
///
/// Work spawned onto another task does not inherit it.
pub(crate) fn active() -> Option<TraceContext> {
    INBOUND.try_with(Clone::clone).ok()
}

/// Parse the inbound context from `headers`.
///
/// W3C trace context treats a repeated `traceparent` as invalid, so only a
/// single parseable value is accepted. Repeated `tracestate` headers are
/// combined into one list in order. The combined list must parse as W3C
/// `tracestate` (OpenTelemetry's [`TraceState`] parser); otherwise the state
/// is dropped and the valid parent is kept with empty state, as W3C requires
/// a vendor list that fails to parse to be discarded independently.
pub(crate) fn inbound(headers: &HeaderMap) -> Option<TraceContext> {
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

#[cfg(test)]
mod tests {
    use axum::http::{HeaderMap, HeaderValue};

    use super::inbound;

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
        let context = inbound(&headers(&[
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
            let context = inbound(&headers(&pairs)).expect("the parent survives");
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
            assert!(inbound(&headers(&pairs)).is_none(), "{pairs:?}");
        }
    }
}
