//! `AgentTraceRecord` — one high-fidelity coding-harness trace row (the fine-tuning
//! corpus). Carries the code axis (reusing the landed dev-capture `CorrelationContext`)
//! plus the full un-truncated agent payload. Tenant is a stamped system column, not a
//! field (C-02).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::error::WyrdError;
use crate::vala::CorrelationContext;
use crate::vala::ids::{RunId, SpanId, TraceId};

/// Cap on the serialized `messages` + `tool_io` payload; `validate()` rejects overflow so
/// no unbounded blob enters a queryable table (M-10).
pub const MAX_AGENT_TRACE_PAYLOAD_BYTES: usize = 1 << 20; // 1 MiB

/// One coding-harness turn row for the `agent_traces` Bifrost table (code-axis,
/// `CorrelationPolicy::CodeAxis`). Carries full un-truncated payload; sensitive
/// columns are redacted at write by the Bifrost sink (task 02 `PayloadClass::Sensitive`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AgentTraceRecord {
    /// The code axis — `dev_session_id`, `repo`, `commit_sha` (advisory base commit),
    /// `branch`. Reused verbatim from the landed dev-capture sub-struct; these are part
    /// of this table's fingerprint (F-03). `CorrelationPolicy::CodeAxis` does NOT append
    /// a second universal `run_id` — this record's own `run_id` below is the one
    /// physical column (C-01).
    pub correlation: CorrelationContext,
    /// The code-axis run identifier. This is this table's own user field; the
    /// `CorrelationPolicy::CodeAxis` mechanism does NOT append a universal `run_id`.
    pub run_id: RunId,
    /// Correlation to the emitting trace/span, when captured under a trace.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trace_id: Option<TraceId>,
    /// Correlated span within the trace, when present.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_id: Option<SpanId>,
    /// Harness turn role (`user` | `assistant` | `tool`).
    pub role: String,
    /// Model + provider that produced the turn.
    pub model: String,
    /// Inference provider (e.g. `anthropic`, `openai`).
    pub provider: String,
    /// Full un-truncated message content for the turn. Sensitive-default
    /// (`SENSITIVE_PAYLOAD_COLUMNS`, task 02) — redacted on write, payload-gated on read.
    pub messages: serde_json::Value,
    /// Full un-truncated tool-call inputs/outputs. Sensitive-default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_io: Option<serde_json::Value>,
    /// Turn start/end wall-clock.
    pub started_at: DateTime<Utc>,
    /// Wall-clock turn end; must be `>= started_at` (validated by `validate()`).
    pub ended_at: DateTime<Utc>,
}

impl AgentTraceRecord {
    /// # Errors
    /// `Validation` when the serialized `messages` + `tool_io` exceed
    /// `MAX_AGENT_TRACE_PAYLOAD_BYTES`, or `ended_at < started_at`.
    pub fn validate(&self) -> Result<(), WyrdError> {
        let err = |msg: &str| {
            Err(WyrdError::Validation {
                message: msg.to_string(),
                details: serde_json::Value::Null,
            })
        };
        if self.ended_at < self.started_at {
            return err("ended_at must be >= started_at");
        }
        let msg_bytes = serde_json::to_vec(&self.messages).unwrap_or_default();
        let tool_bytes = self
            .tool_io
            .as_ref()
            .map(|v| serde_json::to_vec(v).unwrap_or_default().len())
            .unwrap_or(0);
        if msg_bytes.len() + tool_bytes > MAX_AGENT_TRACE_PAYLOAD_BYTES {
            return err("agent trace payload exceeds MAX_AGENT_TRACE_PAYLOAD_BYTES");
        }
        Ok(())
    }
}
