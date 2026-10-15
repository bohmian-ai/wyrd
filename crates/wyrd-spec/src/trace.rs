//! W3C trace context carrier.

use serde::{Deserialize, Serialize};

/// Raw W3C trace context without SDK dependencies.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct TraceContext {
    /// 16-byte trace ID encoded as 32 lowercase hex characters.
    pub trace_id: String,
    /// 8-byte span ID encoded as 16 lowercase hex characters.
    pub span_id: String,
    /// W3C trace flags.
    pub trace_flags: u8,
    /// Raw `tracestate` header.
    pub trace_state: String,
}

impl TraceContext {
    /// Parse a W3C `traceparent` header.
    ///
    /// Version `00` is exactly `00-<32hex>-<16hex>-<2hex>`. A later version
    /// is read forward-compatibly, as W3C requires: its first four fields
    /// parse as version `00` does and any further `-`-separated fields are
    /// ignored.
    ///
    /// # Errors
    /// Returns an error when the version is not two lowercase hex digits or
    /// is the invalid `ff`, when a version `00` header has other than four
    /// fields, or when the ids or flags are malformed or the ids all zero.
    pub fn parse_traceparent(header: &str, trace_state: String) -> Result<Self, TraceError> {
        let parts: Vec<&str> = header.split('-').collect();
        let version = parts[0];
        if !is_lower_hex(version, 2) || version == "ff" || parts.len() < 4 {
            return Err(TraceError::BadHeader);
        }
        if version == "00" && parts.len() != 4 {
            return Err(TraceError::BadHeader);
        }
        if !is_lower_hex(parts[1], 32) || is_all_zero(parts[1]) {
            return Err(TraceError::BadHeader);
        }
        if !is_lower_hex(parts[2], 16) || is_all_zero(parts[2]) {
            return Err(TraceError::BadHeader);
        }
        if !is_lower_hex(parts[3], 2) {
            return Err(TraceError::BadHeader);
        }
        let trace_flags = u8::from_str_radix(parts[3], 16).map_err(|_| TraceError::BadHeader)?;
        Ok(Self {
            trace_id: parts[1].to_string(),
            span_id: parts[2].to_string(),
            trace_flags,
            trace_state,
        })
    }

    /// Format this context as a W3C `traceparent` header.
    #[must_use]
    pub fn to_traceparent(&self) -> String {
        format!(
            "00-{}-{}-{:02x}",
            self.trace_id, self.span_id, self.trace_flags
        )
    }
}

/// Trace context parse errors.
#[derive(Debug, thiserror::Error)]
pub enum TraceError {
    /// Header did not match W3C `traceparent` shape.
    #[error("bad traceparent header")]
    BadHeader,
}

fn is_lower_hex(value: &str, len: usize) -> bool {
    value.len() == len
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn is_all_zero(value: &str) -> bool {
    value.bytes().all(|byte| byte == b'0')
}
