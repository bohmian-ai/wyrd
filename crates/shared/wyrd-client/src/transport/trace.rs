//! Outbound W3C trace context for Wyrd requests.
//!
//! [`HttpTransport`](super::HttpTransport) and the Bifrost gRPC transport send
//! the caller's active trace context as `traceparent`/`tracestate` on every
//! Wyrd request through [`headers`], so server work
//! done for the request — a gateway call, or a judge call a verification makes
//! — joins the caller's trace. [`outbound`] picks that context: one a foreign
//! runtime handed over with [`with_caller_trace`] wins, because Python's and
//! Node's OpenTelemetry contexts never become Rust's current `tracing` span;
//! otherwise the current `tracing` span's OpenTelemetry context, when it is
//! valid. Without either, no trace header is sent.

use std::future::Future;

use opentelemetry::trace::TraceContextExt;
use tracing_opentelemetry::OpenTelemetrySpanExt;
use wyrd_spec::trace::TraceContext;

tokio::task_local! {
    /// The trace context a foreign runtime's caller had active.
    static CALLER: TraceContext;
}

/// W3C header carrying the caller's trace and parent span.
pub(crate) const TRACEPARENT: &str = "traceparent";
/// W3C header carrying vendor trace state.
pub(crate) const TRACESTATE: &str = "tracestate";

/// Run `future` with `context` as the caller's trace context of every Wyrd
/// request it sends on this task.
///
/// Foreign-runtime SDKs read their own active OpenTelemetry span and hand it
/// over here; `None`, or an unparseable `traceparent` the SDK dropped, runs
/// `future` unchanged so the current `tracing` span still applies. Work the
/// future spawns onto another task does not inherit it.
pub async fn with_caller_trace<F: Future>(context: Option<TraceContext>, future: F) -> F::Output {
    match context {
        Some(context) => CALLER.scope(context, future).await,
        None => future.await,
    }
}

/// The trace context the current request is sent under, if any.
///
/// A context scoped by [`with_caller_trace`] wins; otherwise the current
/// `tracing` span's OpenTelemetry span context supplies it when valid.
pub(crate) fn outbound() -> Option<TraceContext> {
    CALLER
        .try_with(Clone::clone)
        .ok()
        .or_else(current_span_context)
}

/// Record the PRODUCER span of one queued write to `table` and return the
/// context its rows carry to the server's ingest span.
///
/// A context a foreign runtime handed over with [`with_caller_trace`] is
/// that caller's own span, which stands as the producer. Otherwise a
/// `bifrost.enqueue` PRODUCER span opens and closes under the current span,
/// and its context is returned; without an OpenTelemetry layer there is none.
pub(crate) fn producer(table: &str) -> Option<TraceContext> {
    CALLER.try_with(Clone::clone).ok().or_else(|| {
        tracing::info_span!(
            "bifrost.enqueue",
            otel.kind = "producer",
            messaging.system = "bifrost",
            messaging.operation.type = "create",
            messaging.destination.name = table,
        )
        .in_scope(current_span_context)
    })
}

/// The W3C headers the current request carries: `traceparent`, plus
/// `tracestate` when non-empty; none without an [`outbound`] context.
pub(crate) fn headers() -> Vec<(&'static str, String)> {
    outbound().map_or_else(Vec::new, |context| {
        let mut headers = vec![(TRACEPARENT, context.to_traceparent())];
        if !context.trace_state.is_empty() {
            headers.push((TRACESTATE, context.trace_state));
        }
        headers
    })
}

/// The current `tracing` span's valid OpenTelemetry span context as W3C
/// trace context; `None` without an OpenTelemetry layer or an active span.
fn current_span_context() -> Option<TraceContext> {
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

/// Unit tests for picking the outbound trace context.
#[cfg(test)]
mod tests {
    use wyrd_spec::trace::TraceContext;

    use super::{outbound, producer, with_caller_trace};

    /// A valid caller `traceparent`.
    const PARENT: &str = "00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01";

    /// Outside every span and scope no trace context is sent.
    #[test]
    fn no_active_context_sends_none() {
        assert_eq!(outbound(), None);
    }

    /// A scoped foreign caller context is the outbound context, and `None`
    /// scopes nothing.
    #[tokio::test]
    async fn scoped_caller_context_is_outbound() {
        let context = TraceContext::parse_traceparent(PARENT, "wyrd=journey".to_owned())
            .expect("valid traceparent");
        let sent = with_caller_trace(Some(context.clone()), async { outbound() }).await;
        assert_eq!(sent, Some(context));
        assert_eq!(with_caller_trace(None, async { outbound() }).await, None);
    }

    /// A foreign caller's handed-over context stands as the producer of a
    /// queued write, since that caller's own span did the enqueue.
    #[tokio::test]
    async fn scoped_caller_context_is_the_producer() {
        let context =
            TraceContext::parse_traceparent(PARENT, String::new()).expect("valid traceparent");
        let produced = with_caller_trace(Some(context.clone()), async { producer("t") }).await;
        assert_eq!(produced, Some(context));
    }

    /// Without a foreign caller, a queued write records its own PRODUCER span
    /// as a child of the current span, in the current span's trace.
    #[test]
    fn producer_span_continues_the_current_trace() {
        use opentelemetry::trace::TracerProvider as _;
        use tracing_subscriber::layer::SubscriberExt as _;

        let traced = tracing_subscriber::registry().with(
            tracing_opentelemetry::layer().with_tracer(
                opentelemetry_sdk::trace::SdkTracerProvider::builder()
                    .build()
                    .tracer("producer"),
            ),
        );
        tracing::subscriber::with_default(traced, || {
            tracing::info_span!("caller").in_scope(|| {
                let caller = outbound().expect("the caller span has a context");
                let produced = producer("t").expect("the PRODUCER span has a context");
                assert_eq!(produced.trace_id, caller.trace_id);
                assert_ne!(produced.span_id, caller.span_id);
            });
        });
    }
}
