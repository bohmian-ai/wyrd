//! Request span hand-off for transports that serve a request on a task of
//! their own.
//!
//! The request span (see `http::router::request_span`) already continues a
//! valid inbound `traceparent`, so work done on the request's behalf reads
//! the caller's position through `wyrd_telemetry::continued_context`.
//! [`attach_trace_context`] stores that span in the request's extensions so a
//! transport that serves the request on a task of its own — the `/mcp`
//! service spawns one per request — can run that task inside it again.

use axum::extract::Request;
use axum::middleware::Next;
use axum::response::Response;

/// The SERVER span of the request that carried an extension holding it.
///
/// A transport that hands the request to another task re-enters this span
/// there, so the work it does descends from the request.
#[derive(Clone, Debug)]
pub(crate) struct RequestSpan(pub(crate) tracing::Span);

/// Run the rest of the request with its request span recorded in its
/// extensions.
pub async fn attach_trace_context(mut request: Request, next: Next) -> Response {
    request
        .extensions_mut()
        .insert(RequestSpan(tracing::Span::current()));
    next.run(request).await
}
