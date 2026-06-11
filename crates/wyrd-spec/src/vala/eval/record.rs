//! `EvalRecordObservation` — the typed observation an instrumented agent emits
//! at evaluation points via `run.observe.eval(record, session_id=...)`.
//!
//! Identity comes from the chain `run_id -> run -> card`. The emit-plane token
//! carries the eval card reference, so the record does not carry an `agent_id`
//! field; duplicating identity invites drift.
//!
//! `session_id` is explicit at emit because run-to-session is many-to-many. A
//! batch worker may be one run serving many sessions, so session is a fact about
//! the observed interaction, not about the run.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::error::WyrdError;
use crate::reference::CardRef;

use super::ids::{RecordId, RunId, SessionId, SpanId, TraceId};

/// The eval observation an instrumented agent emits at evaluation points.
///
/// Rides the same wire envelope as every other Vala observation
/// (`vala_client::ObservationEnvelope`). The server resolves the eval card from
/// `eval_ref`, validates `subject_ref` presence against the emit-plane card
/// identity, and scores the record through the runtime engine.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct EvalRecordObservation {
    /// Client-generated UUIDv7-compatible record identity.
    ///
    /// Stable across retries; the server deduplicates on this value.
    pub record_id: RecordId,

    /// Run identifier of the agent invocation that emitted the record.
    ///
    /// The server resolves the agent / eval card identity via this id; the
    /// record does not carry a separate `agent_id`.
    pub run_id: RunId,

    /// Optional session identifier supplied explicitly at emit.
    ///
    /// Run-to-session is many-to-many. The SDK surface is
    /// `run.observe.eval(record, session_id=...)`. When the agent has no
    /// session concept, pass `None`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<SessionId>,

    /// Reference to the Eval card this record feeds.
    ///
    /// Per the D8 presence rule, the registry rejects online observations
    /// targeting an Eval card whose `subject_ref` is unset, so this field is
    /// meaningful only against cards that declared a subject.
    pub eval_ref: CardRef,

    /// JSON payload the eval tasks assert against.
    ///
    /// The shape is task-driven (`JsonPath` extraction). The eval runtime
    /// applies the spec's `context_capture` policy when storing extracted
    /// values into `AssertionResult.actual`.
    pub context: serde_json::Value,

    /// Trace identifier of the active span at emit time.
    ///
    /// Populated by the SDK from the active OTel span only when the Eval card
    /// has trace tasks. Otherwise `None`; the eval pipeline never awaits a
    /// trace it does not need.
    ///
    /// Carried as a record field, not a propagation mechanism. Span tagging
    /// uses span attributes (`wyrd.run_id`, `wyrd.eval.scenario_id` in local
    /// mode). Cross-service ancestry uses the `Wyrd-Request-Id` HTTP header
    /// carried as a label. The source of truth for span attributes and label
    /// propagation is `architecture/v1/00-foundations/tracing.md`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trace_id: Option<TraceId>,

    /// Span identifier within `trace_id`. Required when `trace_id` is set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_id: Option<SpanId>,

    /// Wall-clock emission time.
    pub created_at: DateTime<Utc>,
}

impl EvalRecordObservation {
    /// Validate cross-field invariants.
    ///
    /// # Errors
    /// Returns [`WyrdError::Validation`] when `span_id` is set without
    /// `trace_id`, because a span id is only meaningful inside its trace.
    pub fn validate(&self) -> Result<(), WyrdError> {
        if self.span_id.is_some() && self.trace_id.is_none() {
            return Err(WyrdError::Validation {
                message: "eval_record_observation.span_id requires trace_id".to_string(),
                details: serde_json::Value::Null,
            });
        }
        Ok(())
    }
}
