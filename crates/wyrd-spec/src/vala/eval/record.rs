//! `EvalRecordObservation` — the typed observation an instrumented agent emits
//! at evaluation points via `run.observe.eval(context, ...)`.
//!
//! The emitting principal comes from the verified credential; the Card the
//! record anchors to is the scoped run's subject `card_ref` — client-asserted
//! and server-authorized against that principal's signed Card scope. Neither
//! the invocation id nor the subject is a field here: both travel as Bifrost
//! row correlation, so the record carries no identity the server would have to
//! re-derive or reconcile. See `wyrd-design.md`, "Observation identity —
//! Card → Run → Observation".
//!
//! `session_id` is explicit at emit because run-to-session is many-to-many. A
//! batch worker may be one run serving many sessions, so session is a fact about
//! the observed interaction, not about the run.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::error::WyrdError;

use super::ids::{RecordId, SessionId, SpanId, TraceId};
use super::media::MediaRef;

/// The eval observation an instrumented agent emits at evaluation points.
///
/// The record names neither the invocation nor a Verifier. The emitting run
/// and the observed subject Card travel beside it as Bifrost row correlation;
/// Scribe authorizes that asserted subject against the publisher's signed Card
/// scope and stamps the resolved Card UID. Once the input is committed, the
/// server activates every matching active `observations_ready` binding from
/// that authorized identity, so one raw record is stored once rather than
/// copied per binding.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EvalRecordObservation {
    /// Client-generated UUIDv7-compatible record identity.
    ///
    /// Stable across retries; the server deduplicates on this value.
    pub record_id: RecordId,

    /// Optional session identifier supplied explicitly at emit.
    ///
    /// Run-to-session is many-to-many. The SDK surface is
    /// `observe.eval(context, session_id=...)`. When the agent has no session
    /// concept, pass `None`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<SessionId>,

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

    /// Reference descriptors for media associated with this eval record.
    ///
    /// URIs pointing to object storage — not inline blobs. Callers fetch content
    /// separately. Null for records with no associated media.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub media: Option<Vec<MediaRef>>,
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
