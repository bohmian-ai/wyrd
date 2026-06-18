//! Request ID middleware.

use axum::body::Body;
use axum::extract::State;
use axum::http::{HeaderValue, Request, header::HeaderName};
use axum::middleware::Next;
use axum::response::Response;
use wyrd_runtime::request_id::{inspect_header, mint};

use crate::state::AppState;

/// Header name used for Wyrd request IDs.
pub const REQUEST_ID_HEADER: &str = "wyrd-request-id";

/// Attach a Wyrd request ID extension to every inbound HTTP request.
pub async fn attach_request_id(
    State(state): State<AppState>,
    mut request: Request<Body>,
    next: Next,
) -> Response {
    let propagation = inspect_inbound_request_id(&request, state.trusted_request_id_propagation);
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

fn inspect_inbound_request_id(
    request: &Request<Body>,
    trusted_request_id_propagation: bool,
) -> wyrd_runtime::request_id::RequestIdPropagation {
    if !trusted_request_id_propagation {
        return inspect_header(None);
    }

    let inbound = request
        .headers()
        .get(REQUEST_ID_HEADER)
        .and_then(|value| value.to_str().ok());
    inspect_header(inbound)
}

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use axum::http::Request;
    use wyrd_spec::request_id::RequestId;

    use super::{REQUEST_ID_HEADER, inspect_inbound_request_id};

    #[test]
    fn untrusted_edge_mints_fresh_ulid() {
        let inbound = uuid::Uuid::now_v7().to_string();
        let request = request_with_header(&inbound);
        let propagation = inspect_inbound_request_id(&request, false);

        assert!(propagation.should_generate);
        assert_eq!(propagation.request_id, None);
    }

    #[test]
    fn trusted_upstream_honors_parseable_inbound() {
        let inbound = uuid::Uuid::now_v7().to_string();
        let request = request_with_header(&inbound);
        let propagation = inspect_inbound_request_id(&request, true);

        assert!(!propagation.should_generate);
        assert_eq!(
            propagation.request_id,
            Some(RequestId::parse(&inbound).expect("generated UUIDv7 is valid"))
        );
    }

    #[test]
    fn trusted_upstream_falls_back_to_fresh_when_unparseable() {
        let request = request_with_header("not-a-request-id");
        let propagation = inspect_inbound_request_id(&request, true);

        assert!(propagation.should_generate);
        assert_eq!(propagation.request_id, None);
    }

    fn request_with_header(value: &str) -> Request<Body> {
        Request::builder()
            .uri("/v1/cards/upload/init")
            .header(REQUEST_ID_HEADER, value)
            .body(Body::empty())
            .expect("request builds")
    }
}
