//! Oracle read-decision and tenant-tripwire audit written to the audit outbox.
//!
//! Oracle records its authorization decisions the one way every boundary does:
//! a committed `append_audit` into `vala.audit_staging`, which the
//! [`crate::audit::publication::AuditPublisher`] later moves into retained
//! history. The commit runs as a tracked background task so a query is never
//! held behind it. A commit that fails is logged and counted; that decision has
//! no audit row.
//!
//! Commits share the Vala pool with reader-epoch renewal and readiness. An
//! append waits on the tenant's `audit_chain_head` row lock, so while that lock
//! is held (for example by a publication settling behind a locked staging row)
//! every read would otherwise park one pooled connection until renewal starved
//! and the Oracle fenced itself. Commits therefore hold one of a fixed share of
//! the pool's connections; the rest queue in memory without a connection.

use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use tokio::sync::Semaphore;
use tokio_util::task::TaskTracker;
use vala_bifrost_redux::oracle::{
    AuthorizedQueryContext, BifrostQueryReadDecision, BifrostSecurityViolation, OracleAudit,
    VerifiedSecurityContext,
};
use vala_sql::ValaPostgres;
use wyrd_spec::DataTenantId;
use wyrd_spec::vala::api::{AuditDetail, AuditEvent, AuditOutcome};
use wyrd_spec::vala::error::BifrostError;

use crate::audit;

/// Owns the non-blocking outbox writes for one server's Oracle decisions.
pub struct OracleQueryAudit {
    /// Tenant-scoped Vala SQL owner each background commit acquires from.
    vala: ValaPostgres,
    /// Tracks in-flight commits so shutdown can wait for them.
    tasks: TaskTracker,
    /// Caps commits holding a pooled connection at once, leaving the rest of
    /// the pool to lease renewal, readiness, and query pins.
    connections: Arc<Semaphore>,
}

/// Fraction of the Vala pool that concurrent audit commits may occupy.
const POOL_SHARE_DIVISOR: u32 = 4;

impl OracleQueryAudit {
    /// Creates the writer around the server's Vala Postgres owner.
    ///
    /// Concurrent commits are limited to a quarter of the pool's configured
    /// maximum connections, and never fewer than one.
    #[must_use]
    pub(crate) fn new(vala: ValaPostgres) -> Arc<Self> {
        let permits = (vala.pool().options().get_max_connections() / POOL_SHARE_DIVISOR).max(1);
        Arc::new(Self {
            vala,
            tasks: TaskTracker::new(),
            connections: Arc::new(Semaphore::new(permits as usize)),
        })
    }

    /// Spawns one tracked commit of `event` into `tenant`'s audit outbox.
    ///
    /// Returns immediately. The task waits for a connection permit before it
    /// acquires a connection and holds it through commit. A failed acquire,
    /// append, or commit increments `oracle_audit_commit_failures_total` and
    /// logs the request identity.
    fn stage(&self, tenant: DataTenantId, event: AuditEvent) {
        let vala = self.vala.clone();
        let connections = Arc::clone(&self.connections);
        self.tasks.spawn(async move {
            let committed = async {
                let _permit = connections
                    .acquire_owned()
                    .await
                    .map_err(|e| e.to_string())?;
                let mut conn = vala.tenant_conn(tenant).await.map_err(|e| e.to_string())?;
                audit::append_on(&mut conn, &event)
                    .await
                    .map_err(|e| e.to_string())?;
                conn.commit().await.map_err(|e| e.to_string())
            }
            .await;
            if let Err(error) = committed {
                metrics::counter!("oracle_audit_commit_failures_total").increment(1);
                tracing::error!(
                    %error,
                    operation = %event.operation,
                    request_id = %event.request_id,
                    "Oracle audit decision did not commit to the audit outbox"
                );
            }
        });
    }

    /// Waits until `deadline` for in-flight commits and returns how many remain.
    ///
    /// Commits still running at the deadline keep running in the background;
    /// a nonzero return lets role shutdown report them.
    pub async fn shutdown(&self, deadline: Instant) -> usize {
        self.tasks.close();
        let _ = tokio::time::timeout_at(deadline.into(), self.tasks.wait()).await;
        self.tasks.len()
    }

    /// Returns the number of commits still in flight.
    #[cfg(feature = "test-support")]
    #[must_use]
    pub fn pending(&self) -> usize {
        self.tasks.len()
    }
}

#[async_trait]
impl OracleAudit for OracleQueryAudit {
    /// Stages the immutable read decision in the tenant audit outbox.
    ///
    /// # Errors
    /// Never returns an error; commit failures are logged and counted.
    async fn append_read_decision(
        &self,
        context: &AuthorizedQueryContext,
        decision: BifrostQueryReadDecision,
    ) -> Result<(), BifrostError> {
        let event = build_event(
            context,
            "bifrost.query.read_decision",
            AuditOutcome::Allowed,
            decision.into_detail(),
        );
        self.stage(context.data_tenant_id, event);
        Ok(())
    }

    /// Stages a verified security violation in the tenant audit outbox.
    ///
    /// # Errors
    /// Never returns an error; commit failures are logged and counted.
    async fn append_security_violation(
        &self,
        context: VerifiedSecurityContext,
        violation: BifrostSecurityViolation,
    ) -> Result<(), BifrostError> {
        let event = build_event(
            &context.query,
            "bifrost.query.security_violation",
            AuditOutcome::Denied,
            AuditDetail::BifrostSecurityViolation {
                violation: violation.violation,
                phase: violation.phase,
                query_digest: context.query_digest.clone(),
                delegation_chain: wyrd_runtime::audit_delegation_chain(
                    &context.query.delegation_chain,
                ),
            },
        );
        self.stage(context.query.data_tenant_id, event);
        Ok(())
    }
}

/// Builds the scrubbed event shared by read decisions and security violations.
fn build_event(
    context: &AuthorizedQueryContext,
    operation: &str,
    outcome: AuditOutcome,
    detail: AuditDetail,
) -> AuditEvent {
    AuditEvent::new(
        context.request_id.clone(),
        context.trace_id.clone(),
        operation.to_owned(),
        "bifrost.query".to_owned(),
        context.principal.card_ref().cloned(),
        context.principal.id,
        context.principal.kind.tag(),
        context.permission.to_string(),
        outcome,
    )
    .with_detail(detail)
}
