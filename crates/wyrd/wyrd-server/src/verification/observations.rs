//! Batched post-acknowledgement outbox of Eval observation run requests.
//!
//! Gate hands every Eval observation frame Scribe has durably acknowledged to
//! [`ObservationRunOutbox`], which derives each row's exact `record_id`,
//! subject, and committed `wyrd_event_time` and queues one run request per
//! record without waiting. Gate hands over only the acknowledgement that first
//! committed a batch, so a suppressed replay never queues and its later
//! receipt instant can never be frozen as a run's event time.
//!
//! One background writer drains the queue. It takes everything waiting,
//! groups it by tenant, and writes each tenant's requests through
//! [`VerifierRunQueue::enqueue_observation_batch`]: one multi-row insert per
//! tenant, keyed by binding and record, so a request written twice inserts
//! nothing. The queue has no count limit and never drops a request because
//! Postgres is slow or down: a tenant group whose write fails stays retained,
//! absorbs later requests of that tenant, and is retried with exponential
//! backoff. Graceful shutdown flushes the queue until its deadline; requests a
//! hard kill or an expired deadline leaves unwritten are lost, and every loss
//! the process observes is counted in
//! `verification_observation_enqueue_failures_total` and logged.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use bytes::Bytes;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;
use vala_bifrost_redux::gate::{AuthContext, ObservationAck};
use vala_bifrost_redux::tables::EvalObservationsTable;
use wyrd_spec::DataTenantId;
use wyrd_sql::WyrdPostgres;
use wyrd_sql::queries::verifier_runs::{ObservationRecord, VerifierRunQueue};

/// First pause before a failed tenant group is retried.
const INITIAL_BACKOFF: Duration = Duration::from_millis(50);

/// Longest pause between retries of a failed tenant group.
const MAX_BACKOFF: Duration = Duration::from_secs(5);

/// Owns the non-blocking run-request outbox for acknowledged Eval observations.
pub struct ObservationRunOutbox {
    /// Unbounded queue of run requests waiting for the writer.
    queue: mpsc::UnboundedSender<(DataTenantId, ObservationRecord)>,
    /// Requests queued, retained, or being written, not yet written.
    pending: Arc<AtomicUsize>,
    /// Asks the writer to stop taking new requests and finish the queue.
    stop: CancellationToken,
    /// Tracks the writer so shutdown can wait for it.
    writer: TaskTracker,
}

impl ObservationRunOutbox {
    /// Creates the outbox around the server's Wyrd Postgres owner and starts
    /// its writer task.
    ///
    /// # Panics
    /// Panics when called outside a Tokio runtime, because the writer task is
    /// spawned immediately.
    #[must_use]
    pub fn new(postgres: WyrdPostgres) -> Arc<Self> {
        let (queue, requests) = mpsc::unbounded_channel();
        let pending = Arc::new(AtomicUsize::new(0));
        let stop = CancellationToken::new();
        let writer = TaskTracker::new();
        writer.spawn(
            ObservationRunWriter {
                postgres,
                runs: VerifierRunQueue::default(),
                requests,
                pending: Arc::clone(&pending),
                stop: stop.clone(),
                retained: Vec::new(),
                backoff: INITIAL_BACKOFF,
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

    /// Queues one run request per record for `tenant` without waiting.
    ///
    /// A request staged after shutdown has closed the queue cannot be written;
    /// it is counted and logged as lost.
    pub fn stage(&self, tenant: DataTenantId, records: Vec<ObservationRecord>) {
        for record in records {
            self.pending.fetch_add(1, Ordering::AcqRel);
            if self.queue.send((tenant, record)).is_err() {
                self.pending.fetch_sub(1, Ordering::AcqRel);
                record_lost(tenant, 1, "the observation run outbox is closed");
            }
        }
    }

    /// Stops the writer once it has written every queued request, waiting
    /// until `deadline`, and returns how many requests remain unwritten.
    ///
    /// A nonzero count is logged and counted as lost: the writer keeps
    /// retrying in the background, but the process is about to exit.
    pub async fn shutdown(&self, deadline: Instant) -> usize {
        self.stop.cancel();
        let _ = tokio::time::timeout_at(deadline.into(), self.writer.wait()).await;
        let unwritten = self.pending.load(Ordering::Acquire);
        if unwritten != 0 {
            metrics::counter!("verification_observation_enqueue_failures_total")
                .increment(u64::try_from(unwritten).unwrap_or(u64::MAX));
            tracing::error!(
                unwritten,
                "observation run requests were not written before the shutdown deadline"
            );
        }
        unwritten
    }

    /// Returns the number of requests queued, retained, or being written.
    #[cfg(feature = "test-support")]
    #[must_use]
    pub fn pending(&self) -> usize {
        self.pending.load(Ordering::Acquire)
    }
}

impl ObservationAck for ObservationRunOutbox {
    /// Decodes the acknowledged frame's records and queues their run requests.
    ///
    /// A frame that does not decode is logged and counted as lost; the
    /// acknowledged observation is never affected.
    fn acknowledged(&self, auth: &AuthContext, frame: Bytes, receipt_micros: i64) {
        match EvalObservationsTable::acknowledged(&frame, &auth.principal, receipt_micros) {
            Ok(keys) => self.stage(
                auth.tenant,
                keys.into_iter()
                    .map(|key| ObservationRecord {
                        subject: key.card_uid,
                        record_id: key.record_id,
                        event_time: key.event_time,
                    })
                    .collect(),
            ),
            Err(error) => {
                tracing::error!(
                    tenant = %auth.tenant,
                    request_id = auth.request_id.as_str(),
                    %error,
                    "acknowledged Eval observation frame did not decode into run requests"
                );
                metrics::counter!("verification_observation_enqueue_failures_total").increment(1);
            }
        }
    }
}

/// Background writer that drains [`ObservationRunOutbox`]'s queue into the
/// durable run queue.
///
/// It is moved into its one task by [`ObservationRunOutbox::new`] and owns the
/// receiving half of the queue and the retained failed groups.
struct ObservationRunWriter {
    /// Wyrd Postgres owner each tenant write opens its transaction through.
    postgres: WyrdPostgres,
    /// The shared durable run queue.
    runs: VerifierRunQueue,
    /// Receiving half of the request queue.
    requests: mpsc::UnboundedReceiver<(DataTenantId, ObservationRecord)>,
    /// Count shared with the owner; decremented once a group is written.
    pending: Arc<AtomicUsize>,
    /// Cancelled by the owner's shutdown to close the queue.
    stop: CancellationToken,
    /// Tenant groups not yet written, in first-arrival order.
    retained: Vec<(DataTenantId, Vec<ObservationRecord>)>,
    /// Pause before the next retry while a group is retained.
    backoff: Duration,
}

impl ObservationRunWriter {
    /// The writer loop: takes every waiting request and writes it, retrying
    /// retained groups with backoff, until stopped with nothing left.
    ///
    /// After `stop` the queue refuses new requests, and the writer keeps
    /// writing what was already queued or retained before it exits.
    async fn run(mut self) {
        let mut arrivals = Vec::new();
        loop {
            if self.stop.is_cancelled() {
                self.requests.close();
            }
            if self.retained.is_empty() {
                let received = tokio::select! {
                    biased;
                    received = self.requests.recv_many(&mut arrivals, usize::MAX) => received,
                    () = self.stop.cancelled() => {
                        self.requests.close();
                        self.requests.recv_many(&mut arrivals, usize::MAX).await
                    }
                };
                if received == 0 {
                    return;
                }
            } else {
                tokio::time::sleep(self.backoff).await;
                while let Ok(request) = self.requests.try_recv() {
                    arrivals.push(request);
                }
            }
            self.retain(&mut arrivals);
            self.flush().await;
        }
    }

    /// Moves `arrivals` into the retained tenant groups, leaving it empty.
    fn retain(&mut self, arrivals: &mut Vec<(DataTenantId, ObservationRecord)>) {
        // ponytail: linear tenant grouping; a map when one pass spans many tenants.
        for (tenant, record) in arrivals.drain(..) {
            match self.retained.iter_mut().find(|(owner, _)| *owner == tenant) {
                Some((_, records)) => records.push(record),
                None => self.retained.push((tenant, vec![record])),
            }
        }
    }

    /// Writes every retained group, one transaction per tenant.
    ///
    /// A written group leaves the pending count. A group whose write fails is
    /// logged and kept for the next retry, which waits twice as long as the
    /// last, up to [`MAX_BACKOFF`]; a pass that writes everything resets it.
    async fn flush(&mut self) {
        let mut failed = Vec::new();
        for (tenant, records) in std::mem::take(&mut self.retained) {
            match self.write(tenant, &records).await {
                Ok(()) => {
                    self.pending.fetch_sub(records.len(), Ordering::AcqRel);
                }
                Err(error) => {
                    metrics::counter!("verification_observation_enqueue_retries_total")
                        .increment(1);
                    tracing::warn!(
                        %tenant,
                        requests = records.len(),
                        error,
                        retry_in_ms = u64::try_from(self.backoff.as_millis()).unwrap_or(u64::MAX),
                        "observation run requests did not write; retrying"
                    );
                    failed.push((tenant, records));
                }
            }
        }
        self.backoff = if failed.is_empty() {
            INITIAL_BACKOFF
        } else {
            (self.backoff * 2).min(MAX_BACKOFF)
        };
        self.retained = failed;
    }

    /// Writes one tenant's requests in one transaction.
    ///
    /// # Errors
    /// Returns a description of the connection, query, or commit failure;
    /// nothing from the group is committed then.
    async fn write(
        &self,
        tenant: DataTenantId,
        records: &[ObservationRecord],
    ) -> Result<(), String> {
        let mut conn = self
            .postgres
            .tenant_conn(tenant)
            .await
            .map_err(|error| error.to_string())?;
        self.runs
            .enqueue_observation_batch(&mut conn, records)
            .await
            .map_err(|error| error.to_string())?;
        conn.commit().await.map_err(|error| error.to_string())
    }
}

/// Log and count `count` run requests of `tenant` that will never be written.
fn record_lost(tenant: DataTenantId, count: usize, error: &str) {
    metrics::counter!("verification_observation_enqueue_failures_total")
        .increment(u64::try_from(count).unwrap_or(u64::MAX));
    tracing::error!(
        %tenant,
        count,
        error,
        "acknowledged Eval observations lost their verification run requests"
    );
}
