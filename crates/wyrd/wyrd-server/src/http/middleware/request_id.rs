//! Request ID middleware.

use std::net::{IpAddr, SocketAddr};

use axum::body::Body;
use axum::extract::{ConnectInfo, State};
use axum::http::{HeaderValue, Request, header::HeaderName};
use axum::middleware::Next;
use axum::response::Response;
use ipnetwork::IpNetwork;
use wyrd_runtime::request_id::{inspect_header, mint};

use crate::state::AppState;

/// Header name used for Wyrd request IDs.
pub const REQUEST_ID_HEADER: &str = "wyrd-request-id";

/// Attach a Wyrd request ID extension to every inbound HTTP request.
///
/// Trusts and propagates an inbound `wyrd-request-id` header only when
/// `state.trusted_request_id_propagation` is enabled and the peer IP is in
/// `state.trusted_upstreams_parsed`. All other requests mint a fresh ID.
///
/// After the handler runs, the middleware:
/// - Writes the request ID into the `wyrd-request-id` response header.
/// - Injects an `instance` field into problem+json error responses that lack
///   one, linking the response back to the request ID.
pub async fn attach_request_id(
    State(state): State<AppState>,
    mut request: Request<Body>,
    next: Next,
) -> Response {
    // Read ConnectInfo from extensions rather than using it as an extractor so
    // the function signature stays a simple 3-tuple and avoids the axum
    // OptionalFromRequestParts constraint on ConnectInfo's non-Infallible rejection.
    let peer_trusted = request
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|ci| ci.0.ip())
        .map(|ip| is_trusted(&ip, &state.trusted_upstreams_parsed))
        .unwrap_or(false);

    let propagation = if state.trusted_request_id_propagation && peer_trusted {
        let inbound = request
            .headers()
            .get(REQUEST_ID_HEADER)
            .and_then(|v| v.to_str().ok());
        inspect_header(inbound)
    } else {
        inspect_header(None)
    };

    let request_id = propagation.request_id.unwrap_or_else(mint);
    tracing::Span::current().record("request_id", request_id.as_str());
    request.extensions_mut().insert(request_id.clone());

    let mut response = next.run(request).await;

    if let Ok(value) = HeaderValue::from_str(request_id.as_str()) {
        response
            .headers_mut()
            .insert(HeaderName::from_static(REQUEST_ID_HEADER), value);
    }

    if should_inject_instance(&response) {
        response = inject_instance(response, &request_id).await;
    }

    response
}

fn is_trusted(ip: &IpAddr, networks: &[IpNetwork]) -> bool {
    networks.iter().any(|net| net.contains(*ip))
}

fn should_inject_instance(response: &Response) -> bool {
    let is_error = response.status().is_client_error() || response.status().is_server_error();
    if !is_error {
        return false;
    }
    response
        .headers()
        .get(axum::http::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .map(|ct| ct.starts_with("application/problem+json"))
        .unwrap_or(false)
}

async fn inject_instance(
    response: Response,
    request_id: &wyrd_spec::request_id::RequestId,
) -> Response {
    let (mut parts, body) = response.into_parts();
    let bytes = match axum::body::to_bytes(body, 1024 * 1024).await {
        Ok(b) => b,
        Err(_) => return Response::from_parts(parts, Body::empty()),
    };
    let mut json: serde_json::Value = match serde_json::from_slice(&bytes) {
        Ok(v) => v,
        Err(_) => {
            return Response::from_parts(parts, Body::from(bytes));
        }
    };
    if let serde_json::Value::Object(ref mut map) = json
        && !map.contains_key("instance")
    {
        map.insert(
            "instance".to_owned(),
            serde_json::Value::String(format!("urn:wyrd:request:{}", request_id.as_str())),
        );
    }
    let updated = serde_json::to_vec(&json).unwrap_or_else(|_| bytes.to_vec());
    parts.headers.insert(
        axum::http::header::CONTENT_LENGTH,
        axum::http::HeaderValue::from(updated.len()),
    );
    Response::from_parts(parts, Body::from(updated))
}

#[cfg(test)]
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
    use std::net::{IpAddr, Ipv4Addr};

    use axum::body::Body;
    use axum::http::Request;
    use wyrd_spec::request_id::RequestId;

    use super::{REQUEST_ID_HEADER, inspect_inbound_request_id, is_trusted};

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

    #[test]
    fn peer_outside_allowlist_mints_fresh() {
        let trusted: Vec<ipnetwork::IpNetwork> = vec!["10.0.0.0/8".parse().expect("valid cidr")];
        let outsider = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1));
        assert!(!is_trusted(&outsider, &trusted));
        let insider = IpAddr::V4(Ipv4Addr::new(10, 1, 2, 3));
        assert!(is_trusted(&insider, &trusted));
    }

    fn request_with_header(value: &str) -> Request<Body> {
        Request::builder()
            .uri("/v1/cards/upload/init")
            .header(REQUEST_ID_HEADER, value)
            .body(Body::empty())
            .expect("request builds")
    }
}
