//! Request ID middleware shell.

use wyrd_spec::request_id::RequestId;

/// Header name used for Wyrd request IDs.
pub const REQUEST_ID_HEADER: &str = "x-request-id";

/// Mint a fresh UUID v7 request ID.
#[must_use]
pub fn mint() -> RequestId {
    let raw = uuid::Uuid::now_v7().to_string();
    RequestId::parse(&raw)
        .unwrap_or_else(|error| panic!("generated UUIDv7 did not validate as RequestId: {error}"))
}

/// Request ID propagation decision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequestIdPropagation {
    /// Request ID value.
    pub request_id: Option<RequestId>,
    /// Whether the server should mint a fresh ID.
    pub should_generate: bool,
}

/// Inspect an optional request-id header.
#[must_use]
pub fn inspect_header(value: Option<&str>) -> RequestIdPropagation {
    match value.and_then(|value| RequestId::parse(value).ok()) {
        Some(request_id) => RequestIdPropagation {
            request_id: Some(request_id),
            should_generate: false,
        },
        None => RequestIdPropagation {
            request_id: None,
            should_generate: true,
        },
    }
}
