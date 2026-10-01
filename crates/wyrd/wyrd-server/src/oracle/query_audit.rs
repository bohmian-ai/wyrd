//! Oracle read-decision and tenant-tripwire audit written to the audit outbox.
//!
//! Oracle records its authorization decisions the one way every boundary does:
//! a committed append into `vala.audit_staging`, which the
//! [`crate::audit::publication::AuditPublisher`] later moves into retained
//! history. A query is never held behind that commit: its decision joins a
//! bounded in-memory queue and the query continues.
//!
//! One background writer drains the queue. It takes everything waiting, groups
//! it by tenant, and commits each tenant's events in one transaction through
//! [`append_audit_batch`]. Every append for a tenant serializes on that
//! tenant's `audit_chain_head` row, so one row lock and one commit per batch,
//! rather than per decision, is what lets audit keep up with the query rate.
//! The writer holds one pooled connection at a time, leaving the rest of the
//! Vala pool to reader-epoch renewal, readiness, and query pins.
//!
//! A decision arriving while the queue is full, or one whose batch fails to
//! commit, is logged and counted in `oracle_audit_commit_failures_total`; that
//! decision has no audit row.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;
use vala_bifrost_redux::oracle::{
    AuthorizedQueryContext, BifrostQueryReadDecision, BifrostSecurityViolation, OracleAudit,
    VerifiedSecurityContext,
};
use vala_sql::ValaPostgres;
use vala_sql::queries::audit_staging::append_audit_batch;
use wyrd_spec::DataTenantId;
use wyrd_spec::vala::api::{AuditDetail, AuditEvent, AuditOutcome};

/// Decisions that may wait for the writer at once.
///
/// Bounds the queue's memory to a few megabytes of small events while leaving
/// room for several seconds of decisions at thousands of queries per second.
const QUEUE_EVENTS: usize = 16_384;

/// Most decisions the writer commits in one tenant transaction.
const BATCH_EVENTS: usize = 1_024;

/// Owns the non-blocking outbox writes for one server's Oracle decisions.
pub struct OracleQueryAudit {
    /// Bounded queue of decisions waiting for the writer.
    queue: mpsc::Sender<(DataTenantId, AuditEvent)>,
    /// Decisions queued or being written, not yet committed or counted lost.
    pending: Arc<AtomicUsize>,
    /// Asks the writer to stop taking new decisions and finish the queue.
    stop: CancellationToken,
    /// Tracks the writer so shutdown can wait for it.
    writer: TaskTracker,
}

impl OracleQueryAudit {
    /// Creates the writer around the server's Vala Postgres owner and starts
    /// its background task.
    ///
    /// # Panics
    ///
    /// Panics when called outside a Tokio runtime, because the writer task is
    /// spawned immediately.
    #[must_use]
    pub(crate) fn new(vala: ValaPostgres) -> Arc<Self> {
        let (queue, decisions) = mpsc::channel(QUEUE_EVENTS);
        let pending = Arc::new(AtomicUsize::new(0));
        let stop = CancellationToken::new();
        let writer = TaskTracker::new();
        writer.spawn(
            OracleAuditWriter {
                vala,
                decisions,
                pending: Arc::clone(&pending),
                stop: stop.clone(),
            }
            .run(),
        );
        writer.close();
        Arc::new(Self {
            queue,
            pending,
            stop,
            writer,
        })
    }

    /// Queues one decision for `tenant`'s audit outbox without waiting.
    ///
    /// When the queue is full the decision is dropped, counted in
    /// `oracle_audit_commit_failures_total`, and logged.
    fn stage(&self, tenant: DataTenantId, event: AuditEvent) {
        self.pending.fetch_add(1, Ordering::AcqRel);
        if let Err(error) = self.queue.try_send((tenant, event)) {
            self.pending.fetch_sub(1, Ordering::AcqRel);
            let (mpsc::error::TrySendError::Full((_, event))
            | mpsc::error::TrySendError::Closed((_, event))) = error;
            record_commit_failure(&event, "Oracle audit queue is full");
        }
    }

    /// Stops the writer once it has committed every queued decision, waiting
    /// until `deadline`, and returns how many decisions remain uncommitted.
    ///
    /// Decisions still queued at the deadline keep draining in the background;
    /// a nonzero return lets role shutdown report them.
    pub async fn shutdown(&self, deadline: Instant) -> usize {
        self.stop.cancel();
        let _ = tokio::time::timeout_at(deadline.into(), self.writer.wait()).await;
        self.pending.load(Ordering::Acquire)
    }

    /// Returns the number of decisions queued or being written.
    #[cfg(feature = "test-support")]
    #[must_use]
    pub fn pending(&self) -> usize {
        self.pending.load(Ordering::Acquire)
    }
}

/// Background writer that drains [`OracleQueryAudit`]'s queue into the
/// tenant audit outbox.
///
/// It is moved into its one task by [`OracleQueryAudit::new`] and owns the
/// receiving half of the queue for the task's lifetime.
struct OracleAuditWriter {
    /// Vala Postgres owner the batches commit through.
    vala: ValaPostgres,
    /// Receiving half of the decision queue.
    decisions: mpsc::Receiver<(DataTenantId, AuditEvent)>,
    /// Count shared with the owner; decremented once a batch is committed or
    /// counted lost.
    pending: Arc<AtomicUsize>,
    /// Cancelled by the owner's shutdown to close the queue.
    stop: CancellationToken,
}

impl OracleAuditWriter {
    /// The writer loop: takes every waiting decision, up to [`BATCH_EVENTS`],
    /// and commits it, until stopped and the queue is empty.
    ///
    /// After `stop` the queue refuses new decisions, and the writer keeps
    /// committing what was already queued before it exits.
    async fn run(mut self) {
        let mut batch = Vec::with_capacity(BATCH_EVENTS);
        loop {
            let received = tokio::select! {
                biased;
                received = self.decisions.recv_many(&mut batch, BATCH_EVENTS) => received,
                () = self.stop.cancelled() => {
                    self.decisions.close();
                    self.decisions.recv_many(&mut batch, BATCH_EVENTS).await
                }
            };
            if received == 0 {
                return;
            }
            self.commit_batch(&mut batch).await;
            self.pending.fetch_sub(received, Ordering::AcqRel);
        }
    }

    /// Commits one drained batch, one transaction per tenant, in queue order.
    ///
    /// A tenant whose acquire, append, or commit fails has each of its
    /// decisions counted and logged as lost; other tenants in the batch still
    /// commit. Leaves `batch` empty.
    async fn commit_batch(&self, batch: &mut Vec<(DataTenantId, AuditEvent)>) {
        // ponytail: linear tenant grouping; a map when one batch spans many tenants.
        let mut tenants: Vec<(DataTenantId, Vec<AuditEvent>)> = Vec::new();
        for (tenant, event) in batch.drain(..) {
            match tenants.iter_mut().find(|(owner, _)| *owner == tenant) {
                Some((_, events)) => events.push(event),
                None => tenants.push((tenant, vec![event])),
            }
        }
        for (tenant, events) in tenants {
            let committed = async {
                let mut conn = self
                    .vala
                    .tenant_conn(tenant)
                    .await
                    .map_err(|e| e.to_string())?;
                append_audit_batch(&mut conn, &events)
                    .await
                    .map_err(|e| e.to_string())?;
                conn.commit().await.map_err(|e| e.to_string())
            }
            .await;
            if let Err(error) = committed {
                for event in &events {
                    record_commit_failure(event, &error);
                }
            }
        }
    }
}

impl OracleAudit for OracleQueryAudit {
    /// Stages the immutable read decision in the tenant audit outbox.
    ///
    /// Commit failures are logged and counted, never returned.
    fn append_read_decision(
        &self,
        context: &AuthorizedQueryContext,
        decision: BifrostQueryReadDecision,
    ) {
        let event = build_event(
            context,
            "bifrost.query.read_decision",
            AuditOutcome::Allowed,
            decision.into_detail(),
        );
        self.stage(context.data_tenant_id, event);
    }

    /// Stages a verified security violation in the tenant audit outbox.
    ///
    /// Commit failures are logged and counted, never returned.
    fn append_security_violation(
        &self,
        context: VerifiedSecurityContext,
        violation: BifrostSecurityViolation,
    ) {
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
    }
}

/// Counts and logs one decision that did not reach the audit outbox.
///
/// Only the operation and request identity are logged, never the event detail.
fn record_commit_failure(event: &AuditEvent, error: &str) {
    metrics::counter!("oracle_audit_commit_failures_total").increment(1);
    tracing::error!(
        %error,
        operation = %event.operation,
        request_id = %event.request_id,
        "Oracle audit decision did not commit to the audit outbox"
    );
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
