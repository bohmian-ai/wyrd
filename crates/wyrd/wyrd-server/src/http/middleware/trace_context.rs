//! Inbound W3C trace context.
//!
//! The request span (see `http::router::request_span`) already continues a
//! valid inbound `traceparent`. [`attach_trace_context`] additionally makes the
//! parsed context active for the rest of that request's task, so work done on
//! the request's behalf — a gateway call, or a judge call a direct
//! verification makes in process — can name the caller's span through
//! [`active`], and it stores the request span in the request's extensions so
//! a transport that serves the request on a task of its own — the `/mcp`
//! service spawns one per request — can run that task inside it again. A
//! missing or malformed `traceparent` leaves no active context; it is never an
//! error.

use axum::extract::Request;
use axum::middleware::Next;
use axum::response::Response;
use wyrd_spec::trace::TraceContext;

tokio::task_local! {
    /// The caller's trace context for the request this task serves.
    static INBOUND: TraceContext;
}

/// The SERVER span of the request that carried an extension holding it.
///
/// A transport that hands the request to another task re-enters this span
/// there, so the work it does descends from the request.
#[derive(Clone, Debug)]
pub(crate) struct RequestSpan(pub(crate) tracing::Span);

/// Run the rest of the request with its valid inbound trace context active
/// and its request span recorded in its extensions.
///
/// A request without exactly one valid `traceparent` runs with no active
/// context.
pub async fn attach_trace_context(mut request: Request, next: Next) -> Response {
    request
        .extensions_mut()
        .insert(RequestSpan(tracing::Span::current()));
    let inbound = wyrd_telemetry::inbound_context(request.headers());
    within(inbound, next.run(request)).await
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
