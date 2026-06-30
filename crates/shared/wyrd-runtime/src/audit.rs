//! Audit envelope construction shell.

use wyrd_spec::{
    actor::Actor, redaction::RedactionPolicy, request_id::RequestId, trace::TraceContext,
};

/// Inputs required to prepare an audit envelope.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditEnvelopeSeed {
    /// Operation name, such as `card.register`.
    pub operation: String,
    /// Optional resource subject for the operation.
    pub subject: Option<String>,
    /// Request ID associated with the operation.
    pub request_id: RequestId,
    /// Optional W3C trace context for the operation.
    pub trace_context: Option<TraceContext>,
    /// Actor that initiated the operation.
    pub actor: Actor,
    /// Redaction policy applied before durable audit emission.
    pub redaction_policy: RedactionPolicy,
}

/// Prepared audit envelope draft.
///
/// This crate does not emit or persist audit events. The draft is the typed
/// handoff point that downstream server code consumes when assembling request,
/// trace, actor, and redaction context.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditEnvelopeDraft {
    /// Operation name, such as `card.register`.
    pub operation: String,
    /// Optional resource subject for the operation.
    pub subject: Option<String>,
    /// Request ID associated with the operation.
    pub request_id: RequestId,
    /// Optional W3C trace context for the operation.
    pub trace_context: Option<TraceContext>,
    /// Actor that initiated the operation.
    pub actor: Actor,
    /// Redaction policy applied before durable audit emission.
    pub redaction_policy: RedactionPolicy,
}

/// Build an audit envelope draft from a seed.
#[must_use]
pub fn prepare(seed: AuditEnvelopeSeed) -> AuditEnvelopeDraft {
    AuditEnvelopeDraft {
        operation: seed.operation,
        subject: seed.subject,
        request_id: seed.request_id,
        trace_context: seed.trace_context,
        actor: seed.actor,
        redaction_policy: seed.redaction_policy,
    }
}
