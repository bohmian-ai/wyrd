use std::sync::Arc;

use vala_bifrost_redux::gate::{AuthContext, GateAudit};
use wyrd_spec::vala::api::AuditOutcome;

use crate::oracle::OracleQueryAudit;

/// Gate's `bifrost_record:write` decision sink over the process audit outbox.
///
/// Gate evaluates the permission but owns no database, so the composition root
/// supplies this sink. Each decision is staged on the one non-blocking outbox
/// every request-path decision of the process shares; ingest never waits for
/// its commit, and a decision the outbox cannot commit is logged and counted
/// by the outbox writer.
#[derive(Clone)]
pub struct OutboxGateAudit {
    /// The process audit outbox the decisions are staged on.
    outbox: Arc<OracleQueryAudit>,
}

impl OutboxGateAudit {
    /// Binds the sink to the process audit outbox.
    #[must_use]
    pub const fn new(outbox: Arc<OracleQueryAudit>) -> Self {
        Self { outbox }
    }
}

impl GateAudit for OutboxGateAudit {
    /// Stages one write decision for the caller's tenant audit chain.
    ///
    /// The row is built by [`crate::audit::attributed_event`], the builder
    /// HTTP decisions share, so it is attributed to the verified principal —
    /// the subject a delegated token acts for — and names a delegation chain's
    /// actors exactly as an HTTP decision does.
    fn stage_write_decision(&self, auth: &AuthContext, resource: &str, outcome: AuditOutcome) {
        self.outbox.stage(
            auth.tenant,
            crate::audit::attributed_event(
                &auth.request_id,
                &auth.principal,
                &auth.delegation_chain,
                "bifrost.record.write",
                resource,
                "bifrost:record:write",
                outcome,
            ),
        );
    }
}
