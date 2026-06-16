//! HTTP response mapping for Wyrd errors.

use axum::body::Body;
use axum::http::{StatusCode, header::CONTENT_TYPE};
use axum::response::{IntoResponse, Response};
use wyrd_spec::error::WyrdError;

/// Server-owned response wrapper for public Wyrd errors.
///
/// Axum's `IntoResponse` trait and `WyrdError` are both owned outside this
/// crate, so Rust's orphan rules require the local wrapper. All server Wyrd
/// error responses still flow through this one mapper.
#[derive(Debug, Clone)]
pub struct WyrdErrorResponse(pub WyrdError);

impl From<WyrdError> for WyrdErrorResponse {
    fn from(error: WyrdError) -> Self {
        Self(error)
    }
}

impl From<WyrdErrorResponse> for WyrdError {
    fn from(response: WyrdErrorResponse) -> Self {
        response.0
    }
}

impl IntoResponse for WyrdErrorResponse {
    fn into_response(self) -> Response {
        wyrd_error_response(self.0)
    }
}

/// Render a Wyrd error as an RFC 9457 problem+json response.
#[must_use]
pub fn wyrd_error_response(error: WyrdError) -> Response {
    let status = StatusCode::from_u16(error.status()).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    match serde_json::to_vec(&error.as_problem_json()) {
        Ok(body) => response_with_body(status, body),
        Err(_) => response_with_body(
            StatusCode::INTERNAL_SERVER_ERROR,
            br#"{"type":"https://wyrd.dev/problems/WYRD_SPEC_500_INTERNAL","title":"Internal error","status":500,"detail":"failed to serialize Wyrd error response","code":"WYRD_SPEC_500_INTERNAL","details":{},"remediation":"Retry later or inspect server logs using the request ID."}"#.to_vec(),
        ),
    }
}

fn response_with_body(status: StatusCode, body: Vec<u8>) -> Response {
    let mut response = Response::new(Body::from(body));
    *response.status_mut() = status;
    response.headers_mut().insert(
        CONTENT_TYPE,
        axum::http::HeaderValue::from_static("application/problem+json"),
    );
    response
}
