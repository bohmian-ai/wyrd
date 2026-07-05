//! Eval run audit seam.
//!
//! Run open/complete emits an audit event (principal, tenant, `eval_ref`,
//! `run_id`) even though run state is ephemeral. Mirrors the swappable
//! `AuthzAuditWriter` seam on [`crate::state::AppState`]: a `Send + Sync` trait
//! object with a tracing-backed default, overridable in tests.

use wyrd_runtime::PrincipalId;
use wyrd_spec::DataTenantId;
use wyrd_spec::reference::CardRef;
use wyrd_spec::vala::ids::RunId;

/// Lifecycle point being audited.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvalAuditKind {
    /// A run was opened.
    RunOpen,
    /// A run reached `RunComplete`.
    RunComplete,
}

impl EvalAuditKind {
    /// Stable event tag for structured logs.
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::RunOpen => "eval.run.open",
            Self::RunComplete => "eval.run.complete",
        }
    }
}

/// One eval audit fact.
#[derive(Debug, Clone)]
pub struct EvalAuditEvent {
    /// Lifecycle point.
    pub kind: EvalAuditKind,
    /// Principal that drove the operation.
    pub principal: PrincipalId,
    /// Tenant the run belongs to.
    pub tenant: DataTenantId,
    /// Eval card the run was opened against.
    pub eval_ref: CardRef,
    /// Ephemeral session-run id.
    pub run_id: RunId,
}

/// Audit sink for eval run lifecycle events.
pub trait EvalAuditWriter: Send + Sync {
    /// Record one eval audit fact.
    fn record(&self, event: &EvalAuditEvent);
}

/// Default writer: emits a structured tracing event on the `wyrd_audit` target.
#[derive(Debug, Default, Clone, Copy)]
pub struct TracingEvalAuditWriter;

impl EvalAuditWriter for TracingEvalAuditWriter {
    fn record(&self, event: &EvalAuditEvent) {
        let kind = event.kind.as_str();
        let eval_ref = event.eval_ref.name.as_str();
        tracing::info!(
            target: "wyrd_audit",
            audit_event = kind,
            audit_principal = %event.principal,
            audit_tenant = %event.tenant,
            audit_eval_ref = eval_ref,
            audit_run_id = %event.run_id,
            "eval run audit event",
        );
    }
}
