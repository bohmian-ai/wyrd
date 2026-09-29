//! Best-effort post-acknowledgement enqueue of Eval observation runs.
//!
//! Gate hands every Eval observation frame Scribe has durably acknowledged to
//! [`ObservationEnqueue`], which derives each row's exact `record_id`, subject,
//! and committed `wyrd_event_time` and enqueues one run per active
//! `observations_ready` binding in a tracked background task. The step is not
//! part of Scribe's batch transaction and keeps no outbox or retry queue: a
//! failure is logged and counted, the observation stays acknowledged, and no
//! run exists for it. Gate hands over only the acknowledgement that first
//! committed a batch, so a suppressed replay never enqueues and its later
//! receipt instant can never be frozen as the run's event time.

use std::sync::Arc;

use bytes::Bytes;
use tokio::sync::Semaphore;
use tokio_util::task::TaskTracker;
use vala_bifrost_redux::gate::{AuthContext, ObservationAck};
use vala_bifrost_redux::tables::EvalObservationsTable;
use wyrd_runtime::Principal;
use wyrd_spec::DataTenantId;
use wyrd_sql::WyrdPostgres;
use wyrd_sql::queries::verifier_runs::VerifierRunQueue;

/// Enqueue tasks owned at once; a frame arriving beyond this is dropped and logged.
const PENDING_LIMIT: usize = 256;

/// Owns the tracked, fail-open enqueue of runs for acknowledged Eval observations.
#[derive(Clone)]
pub struct ObservationEnqueue {
    /// Wyrd Postgres owner each enqueue opens its tenant transaction through.
    postgres: WyrdPostgres,
    /// The shared durable run queue.
    queue: VerifierRunQueue,
    /// In-flight enqueue tasks.
    tasks: TaskTracker,
    /// Bounds the in-flight tasks so a stalled database cannot grow a backlog.
    pending: Arc<Semaphore>,
}

impl ObservationEnqueue {
    /// Build the hook over the server's Wyrd Postgres owner.
    #[must_use]
    pub fn new(postgres: WyrdPostgres) -> Self {
        Self {
            postgres,
            queue: VerifierRunQueue::default(),
            tasks: TaskTracker::new(),
            pending: Arc::new(Semaphore::new(PENDING_LIMIT)),
        }
    }

    /// Enqueue runs for every subject-bearing row of one acknowledged frame.
    ///
    /// Opens one tenant transaction, calls the queue's observation enqueue per
    /// row, and commits once, so a failure part-way leaves no run from this
    /// frame.
    ///
    /// # Errors
    /// Returns a description of the decode, connection, query, or commit failure.
    async fn enqueue(
        &self,
        tenant: DataTenantId,
        principal: &Principal,
        frame: &[u8],
        receipt_micros: i64,
    ) -> Result<usize, String> {
        let keys = EvalObservationsTable::acknowledged(frame, principal, receipt_micros)
            .map_err(|error| error.to_string())?;
        if keys.is_empty() {
            return Ok(0);
        }
        let mut conn = self
            .postgres
            .tenant_conn(tenant)
            .await
            .map_err(|error| error.to_string())?;
        let mut outcomes = 0;
        for key in &keys {
            outcomes += self
                .queue
                .enqueue_observation(&mut conn, &key.card_uid, &key.record_id, key.event_time)
                .await
                .map_err(|error| error.to_string())?
                .len();
        }
        conn.commit().await.map_err(|error| error.to_string())?;
        Ok(outcomes)
    }
}

impl ObservationAck for ObservationEnqueue {
    /// Spawn one tracked enqueue for the acknowledged frame and return at once.
    ///
    /// A full backlog, and any failure inside the task, is logged with the
    /// tenant and request and counted in
    /// `verification_observation_enqueue_failures_total`; the acknowledged
    /// observation is never affected.
    fn acknowledged(&self, auth: &AuthContext, frame: Bytes, receipt_micros: i64) {
        let (tenant, request_id) = (auth.tenant, auth.request_id.clone());
        let Ok(permit) = Arc::clone(&self.pending).try_acquire_owned() else {
            record_failure(
                tenant,
                request_id.as_str(),
                "observation enqueue backlog is full",
            );
            return;
        };
        let owner = self.clone();
        let principal = auth.principal.clone();
        self.tasks.spawn(async move {
            let _permit = permit;
            if let Err(error) = owner
                .enqueue(tenant, &principal, &frame, receipt_micros)
                .await
            {
                record_failure(tenant, request_id.as_str(), &error);
            }
        });
    }
}

/// Log and count one lost observation enqueue.
fn record_failure(tenant: DataTenantId, request_id: &str, error: &str) {
    metrics::counter!("verification_observation_enqueue_failures_total").increment(1);
    tracing::error!(
        %tenant,
        request_id,
        error,
        "acknowledged Eval observation did not enqueue verification runs"
    );
}
