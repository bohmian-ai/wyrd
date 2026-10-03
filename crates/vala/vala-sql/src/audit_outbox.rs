//! The one non-blocking audit outbox every audited surface stages through.
//!
//! Permissions block; audits do not. A surface evaluates its permission,
//! enforces the verdict, and hands the decision to [`AuditOutbox::stage`],
//! which only enqueues it: no request waits for, or is refused by, an audit
//! commit. One background writer per outbox moves queued decisions into
//! `vala.audit_staging` through the canonical batched append, which is private
//! to this crate, so this writer is the only production path that appends
//! staged audit. The server's `AuditPublisher` later moves staged rows into
//! retained history.
//!
//! The writer drains everything waiting, groups it by tenant, and commits each
//! tenant's events in one transaction: one chain-head lock and one commit per
//! batch rather than per decision. Tenants commit concurrently on at most
//! [`WRITER_CONNECTIONS`] pooled connections, and each tenant has at most one
//! commit in flight, so a tenant whose chain head is contended delays only its
//! own audit while its later decisions wait, in order, for that commit.
//!
//! A decision arriving while [`QUEUE_EVENTS`] decisions are already pending, or
//! one whose batch fails to commit, is logged with its operation and request
//! id and counted in `audit_outbox_commit_failures_total`, labelled by the
//! audited surface; that decision has no audit row. An event still queued at
//! abrupt process loss is likewise lost.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

use tokio::sync::{Notify, mpsc};
use tokio::task::{Id, JoinSet};
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;
use wyrd_spec::DataTenantId;
use wyrd_spec::vala::api::AuditEvent;

use crate::ValaPostgres;
use crate::queries::audit_staging::append_audit_events;

/// Decisions that may be pending (queued, waiting, or committing) at once.
///
/// Bounds the outbox's memory to a few megabytes of small events while leaving
/// room for several seconds of decisions at thousands of requests per second.
pub const QUEUE_EVENTS: usize = 16_384;

/// Most decisions the writer commits in one tenant transaction.
pub const BATCH_EVENTS: usize = 1_024;

/// Pooled connections the writer commits tenants on at once.
///
/// Fixed rather than configurable: it keeps the writer's demand on the Vala
/// pool predictable, leaving the rest of the pool to reader-epoch renewal,
/// readiness, query pins, and publication.
pub const WRITER_CONNECTIONS: usize = 4;

/// Owns one process's non-blocking audit outbox.
///
/// The server constructs exactly one per process and shares it with every
/// audited surface through an [`Arc`].
pub struct AuditOutbox {
    /// Queue of decisions waiting for the writer; bounded by `pending`.
    queue: mpsc::UnboundedSender<(DataTenantId, AuditEvent)>,
    /// Decisions queued, waiting, or committing, not yet committed or counted
    /// lost.
    pending: Arc<AtomicUsize>,
    /// Signalled by the writer whenever a settled commit leaves nothing pending.
    idle: Arc<Notify>,
    /// Asks the writer to stop taking new decisions and finish the queue.
    stop: CancellationToken,
    /// Tracks the writer so shutdown can wait for it.
    writer: TaskTracker,
}

impl AuditOutbox {
    /// Creates the outbox around the Vala Postgres owner and starts its writer.
    ///
    /// # Panics
    ///
    /// Panics when called outside a Tokio runtime, because the writer task is
    /// spawned immediately.
    #[must_use]
    pub fn new(vala: ValaPostgres) -> Arc<Self> {
        let (queue, events) = mpsc::unbounded_channel();
        let pending = Arc::new(AtomicUsize::new(0));
        let idle = Arc::new(Notify::new());
        let stop = CancellationToken::new();
        let writer = TaskTracker::new();
        writer.spawn(
            AuditOutboxWriter {
                vala,
                events,
                pending: Arc::clone(&pending),
                idle: Arc::clone(&idle),
                stop: stop.clone(),
                waiting: HashMap::new(),
                committing: JoinSet::new(),
                committing_tenants: HashMap::new(),
            }
            .run(),
        );
        writer.close();
        Arc::new(Self {
            queue,
            pending,
            idle,
            stop,
            writer,
        })
    }

    /// Stages one decision for `tenant` without waiting.
    ///
    /// The decision joins the queue and the caller continues. When
    /// [`QUEUE_EVENTS`] decisions are already pending, or the outbox has shut
    /// down, the decision is dropped, counted, and logged instead.
    pub fn stage(&self, tenant: DataTenantId, event: AuditEvent) {
        if self.pending.fetch_add(1, Ordering::AcqRel) >= QUEUE_EVENTS {
            self.pending.fetch_sub(1, Ordering::AcqRel);
            record_commit_failure(&event, "audit outbox is full");
            return;
        }
        if let Err(mpsc::error::SendError((_, event))) = self.queue.send((tenant, event)) {
            self.pending.fetch_sub(1, Ordering::AcqRel);
            record_commit_failure(&event, "audit outbox is shut down");
        }
    }

    /// Stops accepting decisions and waits until `deadline` for the writer to
    /// commit every queued one, returning how many remain uncommitted.
    ///
    /// Decisions still pending at the deadline keep draining in the
    /// background; a nonzero return lets shutdown report them.
    pub async fn shutdown(&self, deadline: Instant) -> usize {
        self.stop.cancel();
        let _ = tokio::time::timeout_at(deadline.into(), self.writer.wait()).await;
        self.pending.load(Ordering::Acquire)
    }

    /// Waits until `deadline` for every decision staged so far to commit or be
    /// counted lost, without closing the outbox, and returns how many remain.
    ///
    /// Callers that read `vala.audit_staging` right after an audited request
    /// use this to observe the decisions that request staged; the outbox keeps
    /// accepting new decisions throughout.
    pub async fn settle(&self, deadline: Instant) -> usize {
        loop {
            let idle = self.idle.notified();
            tokio::pin!(idle);
            idle.as_mut().enable();
            let pending = self.pending.load(Ordering::Acquire);
            if pending == 0 {
                return 0;
            }
            if tokio::time::timeout_at(deadline.into(), idle)
                .await
                .is_err()
            {
                return self.pending.load(Ordering::Acquire);
            }
        }
    }

    /// Returns the number of decisions queued, waiting, or committing.
    #[must_use]
    pub fn pending(&self) -> usize {
        self.pending.load(Ordering::Acquire)
    }
}

/// Background writer draining one [`AuditOutbox`] into the tenant audit chains.
///
/// It is moved into its one task by [`AuditOutbox::new`] and owns the
/// receiving half of the queue, the per-tenant waiting lists, and the in-flight
/// tenant commits for the task's lifetime.
struct AuditOutboxWriter {
    /// Vala Postgres owner the batches commit through.
    vala: ValaPostgres,
    /// Receiving half of the decision queue.
    events: mpsc::UnboundedReceiver<(DataTenantId, AuditEvent)>,
    /// Count shared with the owner; decremented once a decision is committed
    /// or counted lost.
    pending: Arc<AtomicUsize>,
    /// Woken whenever a settled commit leaves nothing pending.
    idle: Arc<Notify>,
    /// Cancelled by the owner's shutdown to close the queue.
    stop: CancellationToken,
    /// Received decisions per tenant, in queue order, not yet committing.
    waiting: HashMap<DataTenantId, Vec<AuditEvent>>,
    /// In-flight tenant commits.
    committing: JoinSet<()>,
    /// Tenant and decision count of each in-flight commit, keyed by its task.
    committing_tenants: HashMap<Id, (DataTenantId, usize)>,
}

impl AuditOutboxWriter {
    /// The writer loop: receives decisions, starts tenant commits as
    /// connections free up, and settles finished commits, until stopped with
    /// nothing left to commit.
    ///
    /// After `stop` the queue refuses new decisions, and the writer keeps
    /// committing everything already received before it exits.
    async fn run(mut self) {
        let mut received = Vec::with_capacity(BATCH_EVENTS);
        let mut open = true;
        let mut closing = false;
        loop {
            if !open && self.waiting.is_empty() && self.committing.is_empty() {
                return;
            }
            tokio::select! {
                biased;
                Some(done) = self.committing.join_next_with_id(), if !self.committing.is_empty() => {
                    self.settle(done);
                }
                count = self.events.recv_many(&mut received, BATCH_EVENTS), if open => {
                    if count == 0 {
                        open = false;
                    }
                    for (tenant, event) in received.drain(..) {
                        self.waiting.entry(tenant).or_default().push(event);
                    }
                }
                () = self.stop.cancelled(), if !closing => {
                    closing = true;
                    self.events.close();
                }
            }
            self.dispatch();
        }
    }

    /// Starts one commit for each idle tenant with waiting decisions while a
    /// writer connection is free.
    ///
    /// A tenant already committing keeps its later decisions waiting, so each
    /// tenant's events commit in queue order and one contended tenant holds at
    /// most one connection.
    fn dispatch(&mut self) {
        let ready: Vec<DataTenantId> = self
            .waiting
            .keys()
            .filter(|tenant| {
                !self
                    .committing_tenants
                    .values()
                    .any(|(busy, _)| busy == *tenant)
            })
            .copied()
            .collect();
        for tenant in ready {
            if self.committing.len() >= WRITER_CONNECTIONS {
                return;
            }
            let Some(queued) = self.waiting.get_mut(&tenant) else {
                continue;
            };
            let events: Vec<AuditEvent> = if queued.len() > BATCH_EVENTS {
                queued.drain(..BATCH_EVENTS).collect()
            } else {
                self.waiting.remove(&tenant).unwrap_or_default()
            };
            let count = events.len();
            let vala = self.vala.clone();
            let task = self
                .committing
                .spawn(async move { commit_tenant(&vala, tenant, &events).await });
            self.committing_tenants.insert(task.id(), (tenant, count));
        }
    }

    /// Releases a finished tenant commit's connection and settles its count.
    ///
    /// A commit task that panicked lost its decisions; it is logged, and its
    /// decisions leave the pending total like committed ones so shutdown never
    /// waits on them.
    fn settle(&mut self, done: Result<(Id, ()), tokio::task::JoinError>) {
        let id = match done {
            Ok((id, ())) => id,
            Err(error) => {
                tracing::error!(%error, "audit outbox commit task failed");
                error.id()
            }
        };
        if let Some((_, count)) = self.committing_tenants.remove(&id)
            && self.pending.fetch_sub(count, Ordering::AcqRel) == count
        {
            self.idle.notify_waiters();
        }
    }
}

/// Commits one tenant's batch in one transaction.
///
/// A failed acquire, append, or commit counts and logs each event as lost; the
/// transaction rolls back on drop, so nothing of the batch is staged.
async fn commit_tenant(vala: &ValaPostgres, tenant: DataTenantId, events: &[AuditEvent]) {
    let committed = async {
        let mut conn = vala.tenant_conn(tenant).await?;
        append_audit_events(&mut conn, events).await?;
        conn.commit().await
    }
    .await;
    if let Err(error) = committed {
        let error = error.to_string();
        for event in events {
            record_commit_failure(event, &error);
        }
    }
}

/// Counts and logs one decision that did not reach the audit outbox.
///
/// The counter is labelled by the audited surface, the first segment of the
/// event's operation (`bifrost`, `auth`, `gateway`, ...). Only the operation
/// and request identity are logged, never the event detail.
fn record_commit_failure(event: &AuditEvent, error: &str) {
    let surface = event
        .operation
        .split('.')
        .next()
        .unwrap_or_default()
        .to_owned();
    metrics::counter!("audit_outbox_commit_failures_total", "surface" => surface).increment(1);
    tracing::error!(
        %error,
        operation = %event.operation,
        request_id = %event.request_id,
        "audit decision did not commit to the audit outbox"
    );
}
