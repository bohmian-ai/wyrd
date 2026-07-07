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
///
/// Propagates a valid inbound `wyrd-request-id` header from any caller;
/// mints a fresh UUID v7 when the header is absent or unparseable.
///
/// After the handler runs, the middleware:
/// - Writes the request ID into the `wyrd-request-id` response header.
/// - Injects an `instance` field into problem+json error responses that lack
///   one, linking the response back to the request ID.
pub async fn attach_request_id(
    State(_state): State<AppState>,
    mut request: Request<Body>,
    next: Next,
) -> Response {
    let inbound = request
        .headers()
        .get(REQUEST_ID_HEADER)
        .and_then(|v| v.to_str().ok());
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

    if should_inject_instance(&response) {
        response = inject_instance(response, &request_id).await;
    }

    response
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
mod tests {
    use axum::body::Body;
    use axum::http::Request;
    use wyrd_runtime::request_id::inspect_header;
    use wyrd_spec::request_id::RequestId;

    use super::REQUEST_ID_HEADER;

    #[test]
    fn valid_inbound_request_id_is_propagated() {
        let inbound = uuid::Uuid::now_v7().to_string();
        let request = request_with_header(&inbound);
        let header_val = request
            .headers()
            .get(REQUEST_ID_HEADER)
            .and_then(|v| v.to_str().ok());
        let propagation = inspect_header(header_val);

        assert!(!propagation.should_generate);
        assert_eq!(
            propagation.request_id,
            Some(RequestId::parse(&inbound).expect("generated UUIDv7 is valid"))
        );
    }

    #[test]
    fn unparseable_inbound_request_id_mints_fresh() {
        let request = request_with_header("not-a-request-id");
        let header_val = request
            .headers()
            .get(REQUEST_ID_HEADER)
            .and_then(|v| v.to_str().ok());
        let propagation = inspect_header(header_val);

        assert!(propagation.should_generate);
        assert_eq!(propagation.request_id, None);
    }

    #[test]
    fn absent_header_mints_fresh() {
        let propagation = inspect_header(None);
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
