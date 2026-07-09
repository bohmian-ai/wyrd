//! OTel-compliant span record.
//!
//! (Proto: `opentelemetry.proto.trace.v1.Span`.)
//!
//! `SpanRecord` is the centerpiece of the trace surface. Every consumer reads
//! this shape. Field order and naming follow the OTel proto so the OTLP
//! receiver conversion is a near-mechanical mapping. `Resource` and
//! `InstrumentationScope` are nested, not flattened, so the OTel canonical
//! structure is preserved end-to-end.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::error::WyrdError;
use crate::vala::ids::{SpanId, TraceId};

use super::{InstrumentationScope, Resource, SpanEvent, SpanLink};

/// One OTel-compliant span.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SpanRecord {
    /// 16-byte trace id. (Proto: `Span.trace_id`.)
    pub trace_id: TraceId,
    /// 8-byte span id. (Proto: `Span.span_id`.)
    pub span_id: SpanId,
    /// Parent span id. `None` for root spans.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_span_id: Option<SpanId>,
    /// OTel proto `Span.flags` fixed32 bitfield.
    #[serde(default)]
    pub flags: u32,
    /// W3C tracestate header. (Proto: `Span.trace_state`.)
    #[serde(default)]
    pub trace_state: String,
    /// Span name. Non-empty and no longer than 256 bytes.
    pub name: String,
    /// Span kind. (Proto: `Span.kind`.)
    pub kind: SpanKind,
    /// Span start time. (Proto: `Span.start_time_unix_nano`.)
    pub start_time: DateTime<Utc>,
    /// Span end time. (Proto: `Span.end_time_unix_nano`.)
    pub end_time: DateTime<Utc>,
    /// Query convenience field equal to `(end_time - start_time)` in
    /// milliseconds.
    pub duration_ms: u64,
    /// Span status. (Proto: `Span.status`.)
    pub status: SpanStatus,
    /// Span attribute bag. (Proto: `Span.attributes`.)
    #[serde(default)]
    pub attributes: serde_json::Map<String, serde_json::Value>,
    /// Count of span-level attributes the SDK dropped before sending.
    #[serde(default)]
    pub dropped_attributes_count: u32,
    /// Span events. (Proto: `Span.events`.)
    #[serde(default)]
    pub events: Vec<SpanEvent>,
    /// Count of span events the SDK dropped before sending.
    #[serde(default)]
    pub dropped_events_count: u32,
    /// Span links. (Proto: `Span.links`.)
    #[serde(default)]
    pub links: Vec<SpanLink>,
    /// Count of span links the SDK dropped before sending.
    #[serde(default)]
    pub dropped_links_count: u32,
    /// Instrumentation scope of the producer.
    pub scope: InstrumentationScope,
    /// Resource of the producer.
    pub resource: Resource,
}

impl SpanRecord {
    /// Returns the producer service name from the nested resource.
    #[must_use]
    pub fn service_name(&self) -> &str {
        &self.resource.service_name
    }

    /// Full OTel-compliance check for the span record.
    ///
    /// Enforces non-empty bounded names, timestamp ordering, derived duration
    /// consistency, the W3C 512-byte tracestate cap, event timestamps inside
    /// the span window, child event/link validation, and nested
    /// resource/scope validation.
    ///
    /// # Errors
    /// Returns [`WyrdError::Validation`] when any span invariant or child
    /// validator fails.
    pub fn validate(&self) -> Result<(), WyrdError> {
        if self.name.is_empty() {
            return Err(WyrdError::Validation {
                message: "span.name must be non-empty".to_string(),
                details: serde_json::Value::Null,
            });
        }
        if self.name.len() > 256 {
            return Err(WyrdError::Validation {
                message: format!("span.name length must be <= 256, got {}", self.name.len()),
                details: serde_json::Value::Null,
            });
        }
        if self.attributes.len() > 256 {
            return Err(WyrdError::Validation {
                message: format!(
                    "span.attributes count must be <= 256, got {}",
                    self.attributes.len()
                ),
                details: serde_json::Value::Null,
            });
        }
        if self.end_time < self.start_time {
            return Err(WyrdError::Validation {
                message: format!(
                    "span.end_time {:?} < start_time {:?}",
                    self.end_time, self.start_time
                ),
                details: serde_json::Value::Null,
            });
        }

        let derived = super::duration_ms_from_timestamps(self.start_time, self.end_time);
        if self.duration_ms != derived {
            return Err(WyrdError::Validation {
                message: format!(
                    "span.duration_ms {} != end_time - start_time = {}",
                    self.duration_ms, derived
                ),
                details: serde_json::Value::Null,
            });
        }
        if self.trace_state.len() > 512 {
            return Err(WyrdError::Validation {
                message: format!(
                    "span.trace_state length must be <= 512, got {}",
                    self.trace_state.len()
                ),
                details: serde_json::Value::Null,
            });
        }

        for (index, event) in self.events.iter().enumerate() {
            event.validate().map_err(|error| match error {
                WyrdError::Validation { message, .. } => WyrdError::Validation {
                    message: format!("events[{index}]: {message}"),
                    details: serde_json::Value::Null,
                },
                other => other,
            })?;
            if event.timestamp < self.start_time || event.timestamp > self.end_time {
                return Err(WyrdError::Validation {
                    message: format!(
                        "events[{index}].timestamp {:?} outside span window [{:?}, {:?}]",
                        event.timestamp, self.start_time, self.end_time
                    ),
                    details: serde_json::Value::Null,
                });
            }
        }

        for (index, link) in self.links.iter().enumerate() {
            link.validate().map_err(|error| match error {
                WyrdError::Validation { message, .. } => WyrdError::Validation {
                    message: format!("links[{index}]: {message}"),
                    details: serde_json::Value::Null,
                },
                other => other,
            })?;
        }

        self.resource.validate()?;
        self.scope.validate()?;
        Ok(())
    }
}

/// OTel canonical span kind. (Proto: `Span.SpanKind`.)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SpanKind {
    /// Default internal operation inside the service.
    Internal,
    /// Inbound RPC or HTTP handler.
    Server,
    /// Outbound RPC or HTTP call.
    Client,
    /// Asynchronous message producer.
    Producer,
    /// Asynchronous message consumer.
    Consumer,
}

/// OTel canonical span status. (Proto: `Span.Status`.)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "code", rename_all = "snake_case", deny_unknown_fields)]
pub enum SpanStatus {
    /// No status set; OTel default.
    Unset,
    /// Operation completed successfully.
    Ok,
    /// Operation failed with an optional human-readable description.
    Error {
        /// Optional status description.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        description: Option<String>,
    },
}
