//! OTel canonical `Span.Link`.
//!
//! (Proto: `opentelemetry.proto.trace.v1.Span.Link`.)
//!
//! A causal reference from this span to another span — typically another
//! trace (asynchronous fan-out, batched work, distributed correlation).
//! Carries the linked `(trace_id, span_id)` plus optional `trace_state`
//! and attributes.

use serde::{Deserialize, Serialize};

use crate::error::WyrdError;
use crate::vala::ids::{SpanId, TraceId};

/// OTel-canonical span link.
///
/// Fields:
///
/// - `trace_id` / `span_id` — linked span identity. Both are typed
///   (commit 02); the wire shape is hex strings.
/// - `trace_state` — W3C TraceContext `tracestate` of the linked span
///   (vendor list, comma-separated). Defaults to empty.
/// - `flags` — full OTel proto `Link.flags` fixed32 bitfield. The low
///   byte carries W3C trace flags (bit 0 = `sampled`); the upper bits
///   are reserved by the OTel spec for SDK-internal signaling. Stored
///   as `u32` so we don't truncate non-W3C bits at the contract layer.
/// - `attributes` — typed key/value bag, same shape as
///   [`super::SpanEvent::attributes`].
/// - `dropped_attributes_count` — count of attributes the SDK dropped
///   before sending.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SpanLink {
    /// Linked trace id.
    pub trace_id: TraceId,
    /// Linked span id.
    pub span_id: SpanId,
    /// W3C tracestate header value for the linked span.
    #[serde(default)]
    pub trace_state: String,
    /// OTel proto `Link.flags` fixed32 bitfield. Bit 0 = `sampled`;
    /// upper bits reserved.
    #[serde(default)]
    pub flags: u32,
    /// Link attribute bag.
    #[serde(default)]
    pub attributes: serde_json::Map<String, serde_json::Value>,
    /// Count of attributes the SDK dropped before sending.
    #[serde(default)]
    pub dropped_attributes_count: u32,
}

impl SpanLink {
    /// Validate the link-level invariants:
    ///
    /// - `trace_state` ≤ 512 bytes (W3C TraceContext recommended cap)
    /// - `attributes` size ≤ 128 entries (matches `SpanEvent`)
    ///
    /// The typed `TraceId` / `SpanId` newtypes (commit 02) already reject
    /// the all-zero sentinel at construction, so no in-line id check.
    ///
    /// # Errors
    /// Returns [`WyrdError::Validation`] when the link violates these
    /// invariants.
    pub fn validate(&self) -> Result<(), WyrdError> {
        if self.trace_state.len() > 512 {
            return Err(WyrdError::Validation {
                message: format!(
                    "span_link.trace_state length must be <= 512, got {}",
                    self.trace_state.len()
                ),
                details: serde_json::Value::Null,
            });
        }
        if self.attributes.len() > 128 {
            return Err(WyrdError::Validation {
                message: format!(
                    "span_link.attributes count must be <= 128, got {}",
                    self.attributes.len()
                ),
                details: serde_json::Value::Null,
            });
        }
        Ok(())
    }
}
