//! `LogRecord` — the canonical OTel LogRecord signal (a faithful OTLP log point).
//! Per the OTel data model every field except the server-observed time is optional.
//! Structured app/agent logs that correlate to spans via trace_id/span_id. Opaque
//! body/attributes. Tenant + event-time are stamped as Bifrost system columns, not here.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::error::WyrdError;
use crate::vala::ids::{SpanId, TraceId};
use crate::vala::trace::{InstrumentationScope, Resource};

/// Maximum serialized size of `LogRecord.body` in bytes.
pub const MAX_LOG_BODY_BYTES: usize = 256 * 1024; // 256 KiB
/// Maximum serialized size of `LogRecord.attributes` in bytes.
pub const MAX_LOG_ATTRIBUTES_BYTES: usize = 64 * 1024; // 64 KiB

/// One OTel log record for the `logs.records` Bifrost table. Every field except
/// `observed_time` is optional per the OTel data model. Tenant is a stamped system
/// column (C-02). Payload size is bounded by `validate()`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LogRecord {
    /// `time_unix_nano` — OTLP-optional (absent when upstream never set it).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub time: Option<DateTime<Utc>>,
    /// `observed_time_unix_nano` — server-set at ingest; always present, and the
    /// source of `wyrd_event_time` when `time` is absent.
    pub observed_time: DateTime<Utc>,
    /// OTel SeverityNumber 0..=24. `None` or `Some(0)` = UNSPECIFIED.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub severity_number: Option<u8>,
    /// OTel SeverityText (display only).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub severity_text: Option<String>,
    /// OTel EventName (top-level; replaces the deprecated `event.name` attribute).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub event_name: Option<String>,
    /// Opaque log body (string or structured JSON) — OTLP-optional.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<serde_json::Value>,
    /// Correlation to the emitting span, when present.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trace_id: Option<TraceId>,
    /// Correlated span; requires `trace_id` when set (validated by `validate()`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_id: Option<SpanId>,
    /// OTel `trace_flags` byte (W3C trace-context flags) — OTLP-optional.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trace_flags: Option<u8>,
    /// Producer resource — OTLP-optional.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resource: Option<Resource>,
    /// OTel InstrumentationScope of the emitting logger — OTLP-optional.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<InstrumentationScope>,
    /// Opaque log attributes.
    #[serde(default)]
    pub attributes: serde_json::Map<String, serde_json::Value>,
    /// OTel `dropped_attributes_count`.
    #[serde(default)]
    pub dropped_attributes_count: u32,
}

impl LogRecord {
    /// # Errors
    /// - `severity_number > 24` (0 is the valid UNSPECIFIED sentinel).
    /// - `span_id` set without `trace_id`.
    /// - Serialized `body` exceeds `MAX_LOG_BODY_BYTES`.
    /// - Serialized `attributes` exceeds `MAX_LOG_ATTRIBUTES_BYTES`.
    pub fn validate(&self) -> Result<(), WyrdError> {
        let err = |msg: &str| {
            Err(WyrdError::Validation {
                message: msg.to_string(),
                details: serde_json::Value::Null,
            })
        };
        if let Some(sev) = self.severity_number
            && sev > 24
        {
            return err("severity_number must be in [0, 24]");
        }
        if self.span_id.is_some() && self.trace_id.is_none() {
            return err("span_id requires trace_id");
        }
        if let Some(body) = &self.body {
            let bytes = serde_json::to_vec(body).unwrap_or_default();
            if bytes.len() > MAX_LOG_BODY_BYTES {
                return err("log body exceeds MAX_LOG_BODY_BYTES");
            }
        }
        let attr_bytes = serde_json::to_vec(&self.attributes).unwrap_or_default();
        if attr_bytes.len() > MAX_LOG_ATTRIBUTES_BYTES {
            return err("log attributes exceed MAX_LOG_ATTRIBUTES_BYTES");
        }
        Ok(())
    }
}
