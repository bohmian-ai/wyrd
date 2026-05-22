//! Request ID middleware shell.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use wyrd_spec::request_id::RequestId;

/// Header name used for Wyrd request IDs.
pub const REQUEST_ID_HEADER: &str = "x-request-id";

static REQUEST_COUNTER: AtomicU64 = AtomicU64::new(1);

/// Mint a fresh ULID-shaped request ID.
#[must_use]
pub fn mint() -> RequestId {
    let raw = mint_ulid_string();
    RequestId::parse(&raw)
        .unwrap_or_else(|error| panic!("generated ULID did not validate as RequestId: {error}"))
}

fn mint_ulid_string() -> String {
    let timestamp_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_else(|_| Duration::from_secs(0))
        .as_millis()
        .min(u128::from(u64::MAX)) as u64;
    let counter = REQUEST_COUNTER.fetch_add(1, Ordering::Relaxed);
    encode_ulid(timestamp_ms, counter)
}

fn encode_ulid(timestamp_ms: u64, counter: u64) -> String {
    const ALPHABET: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";

    let mut bytes = [0_u8; 16];
    let timestamp = timestamp_ms & 0xFFFF_FFFF_FFFF;
    bytes[0] = (timestamp >> 40) as u8;
    bytes[1] = (timestamp >> 32) as u8;
    bytes[2] = (timestamp >> 24) as u8;
    bytes[3] = (timestamp >> 16) as u8;
    bytes[4] = (timestamp >> 8) as u8;
    bytes[5] = timestamp as u8;
    bytes[8..].copy_from_slice(&counter.to_be_bytes());

    let mut value = u128::from_be_bytes(bytes);
    let mut output = [0_u8; 26];
    for index in (0..output.len()).rev() {
        output[index] = ALPHABET[(value & 0b1_1111) as usize];
        value >>= 5;
    }
    output.iter().map(|byte| char::from(*byte)).collect()
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

/// Axum/Tower middleware shell for request ID propagation.
///
/// Phase 1 exposes an identity layer so server crates can type-check future
/// wiring without locking request behavior before the HTTP phase.
#[cfg(feature = "behavior")]
#[must_use]
pub fn middleware() -> tower::layer::util::Identity {
    tower::layer::util::Identity::new()
}
