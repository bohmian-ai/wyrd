//! Audit envelope construction shell and the audit staging seam.

use wyrd_spec::DataTenantId;
use wyrd_spec::vala::api::AuditEvent;
use wyrd_spec::{
    actor::Actor, redaction::RedactionPolicy, request_id::RequestId, trace::TraceContext,
};

use crate::outbox::{Outbox, OutboxSink};

/// Non-blocking handoff every audited surface stages its decisions through.
///
/// Permissions block; audits do not. A surface evaluates its permission,
/// enforces the verdict, and stages the decision here, which only queues it:
/// no request waits for, or is refused by, the audit write. The server
/// implements it with its one Scribe outbox; lower crates hold it as
/// `dyn AuditStage` so they never name the server's sink.
pub trait AuditStage: Send + Sync {
    /// Queues `event` for `tenant` and returns at once; never fails.
    fn stage(&self, tenant: DataTenantId, event: AuditEvent);
}

impl<S> AuditStage for Outbox<S>
where
    S: OutboxSink,
    S::Item: From<AuditEvent>,
{
    /// Stages `event` on this outbox through its ordinary non-blocking
    /// [`Outbox::stage`].
    fn stage(&self, tenant: DataTenantId, event: AuditEvent) {
        Self::stage(self, tenant, event);
    }
}

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
