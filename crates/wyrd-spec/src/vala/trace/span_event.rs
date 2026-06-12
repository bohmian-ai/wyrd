//! OTel canonical `Span.Event`.
//!
//! (Proto: `opentelemetry.proto.trace.v1.Span.Event`.)
//!
//! A point-in-time annotation inside a span. Used for log-line-equivalent
//! signals (`exception`, `log`, custom event names). The
//! `gen_ai.evaluation.result` event (commit 08) is a SpanEvent whose
//! attributes carry the evaluation score / label / explanation.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::error::WyrdError;

/// OTel-canonical span event.
///
/// Fields:
///
/// - `timestamp` — wall-clock time the event occurred. Must lie within
///   the span's `[start_time, end_time]` window; that check is enforced
///   by [`crate::vala::trace::SpanRecord::validate`], not here, because
///   the event itself has no knowledge of its parent span.
/// - `name` — event name. Free-form but non-empty; OTel conventions
///   suggest namespaced names (`exception`, `log`,
///   `gen_ai.evaluation.result`).
/// - `attributes` — typed key/value bag stored as flat JSON. Use the
///   constants in [`crate::vala::trace::attributes`] for known keys.
/// - `dropped_attributes_count` — count of attributes the SDK dropped
///   before sending (OTel default 0 means "none dropped"). Carried for
///   parity with the OTLP proto.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SpanEvent {
    /// Wall-clock time the event occurred.
    pub timestamp: DateTime<Utc>,
    /// Event name; non-empty.
    pub name: String,
    /// Event attribute bag.
    #[serde(default)]
    pub attributes: serde_json::Map<String, serde_json::Value>,
    /// Count of attributes the SDK dropped before sending.
    #[serde(default)]
    pub dropped_attributes_count: u32,
}

impl SpanEvent {
    /// Validate the event-level invariants:
    ///
    /// - `name` non-empty and ≤ 256 characters
    /// - `attributes` size ≤ 128 entries (OTel SDK default limit; raise
    ///   if a future consumer needs more)
    ///
    /// The timestamp-vs-span-window check lives in
    /// [`crate::vala::trace::SpanRecord::validate`].
    ///
    /// # Errors
    /// Returns [`WyrdError::Validation`] when the event violates these
    /// invariants.
    pub fn validate(&self) -> Result<(), WyrdError> {
        if self.name.is_empty() {
            return Err(WyrdError::Validation {
                message: "span_event.name must be non-empty".to_string(),
                details: serde_json::Value::Null,
            });
        }
        if self.name.len() > 256 {
            return Err(WyrdError::Validation {
                message: format!(
                    "span_event.name length must be <= 256, got {}",
                    self.name.len()
                ),
                details: serde_json::Value::Null,
            });
        }
        if self.attributes.len() > 128 {
            return Err(WyrdError::Validation {
                message: format!(
                    "span_event.attributes count must be <= 128, got {}",
                    self.attributes.len()
                ),
                details: serde_json::Value::Null,
            });
        }
        Ok(())
    }
}
