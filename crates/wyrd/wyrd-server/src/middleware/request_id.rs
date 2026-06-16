//! Request ID middleware.

use axum::body::Body;
use axum::http::{HeaderValue, Request, header::HeaderName};
use axum::middleware::Next;
use axum::response::Response;
use wyrd_runtime::request_id::{REQUEST_ID_HEADER, inspect_header, mint};

/// Attach a Wyrd request ID extension to every inbound HTTP request.
pub async fn attach_request_id(mut request: Request<Body>, next: Next) -> Response {
    let inbound = request
        .headers()
        .get(REQUEST_ID_HEADER)
        .and_then(|value| value.to_str().ok());
    let propagation = inspect_header(inbound);
    let request_id = propagation.request_id.unwrap_or_else(mint);

    tracing::Span::current().record("request_id", request_id.as_str());
    request.extensions_mut().insert(request_id.clone());

    let mut response = next.run(request).await;
    if let Ok(value) = HeaderValue::from_str(request_id.as_str()) {
        response
            .headers_mut()
            .insert(HeaderName::from_static(REQUEST_ID_HEADER), value);
    }
    response
}
