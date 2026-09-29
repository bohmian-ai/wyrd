//! Turns due binding schedules into Verifier runs.
//!
//! [`VerificationScheduler`] lists tenants with a due scheduled binding
//! through the operator pool, then advances each tenant's due bindings one
//! occurrence per short tenant transaction through
//! [`VerifierRunQueue::schedule_next_due`]. The queue locks each binding with
//! `SKIP LOCKED` and dedupes an occurrence's run, so concurrent schedulers on
//! many processes, duplicate ticks, and a restart mid-pass never create a
//! second run for one occurrence.

use std::sync::Arc;
use std::time::Duration;

use tokio_util::sync::CancellationToken;
use wyrd_spec::DataTenantId;
use wyrd_sql::queries::verifier_runs::{
    QueueCounts, ScheduleOutcome, ScheduleSkip, ScheduleTick, VerifierRunQueue,
};
use wyrd_sql::{OperatorPool, SqlError, WyrdPostgres};

#[cfg(feature = "test-support")]
use super::CapabilityCrash;
#[cfg(feature = "test-support")]
use super::health::RuntimeCapability;

/// Tenants examined per scheduler pass, most overdue first.
const TENANTS_PER_PASS: i64 = 64;

/// Occurrences one tenant may schedule per pass before the next tenant runs.
const TICKS_PER_TENANT: usize = 32;

/// Owner of the binding-schedule loop.
pub struct VerificationScheduler {
    /// Wyrd Postgres owner that opens every tenant-scoped schedule transaction.
    postgres: WyrdPostgres,
    /// Operator pool for the cross-tenant due list and queue depth.
    operator: OperatorPool,
    /// Queue policies and transitions.
    queue: VerifierRunQueue,
    /// Idle wait between passes.
    poll_interval: Duration,
    /// Test-only crash switch.
    #[cfg(feature = "test-support")]
    crash: Option<CapabilityCrash>,
}

impl VerificationScheduler {
    /// Build a scheduler over the Wyrd Postgres owner, the operator pool, and
    /// the queue.
    ///
    /// The scheduler carries no clock: every due decision is PostgreSQL's, and
    /// `poll_interval` is only this process's idle wait between passes.
    #[must_use]
    pub fn new(
        postgres: WyrdPostgres,
        operator: OperatorPool,
        queue: VerifierRunQueue,
        poll_interval: Duration,
    ) -> Self {
        Self {
            postgres,
            operator,
            queue,
            poll_interval,
            #[cfg(feature = "test-support")]
            crash: None,
        }
    }

    /// Let `crash` panic this scheduler's loop.
    #[cfg(feature = "test-support")]
    #[must_use]
    pub fn with_crash(mut self, crash: CapabilityCrash) -> Self {
        self.crash = Some(crash);
        self
    }

    /// Run passes every poll interval until `stop` is cancelled.
    ///
    /// A failed pass is logged and retried on the next interval; the loop
    /// only returns on `stop`.
    ///
    /// The depth report and the idle wait race `stop` directly. A pass
    /// observes `stop` only at its occurrence linearization point (see
    /// [`Self::pass`]), so every occurrence has a known outcome when the loop
    /// returns: rolled back, or committed and admitted before closure.
    ///
    /// # Panics
    /// Panics under `test-support` when a test armed a scheduler crash.
    ///
    /// # Cancellation
    /// Cancellation before an occurrence's commit is selected rolls it back;
    /// a selected commit is awaited to its result before the loop returns.
    pub async fn run(self: Arc<Self>, stop: CancellationToken) {
        loop {
            #[cfg(feature = "test-support")]
            if let Some(crash) = &self.crash {
                crash.check(RuntimeCapability::Scheduler);
            }
            tokio::select! {
                biased;
                () = stop.cancelled() => return,
                reported = self.report_depth() => if let Err(error) = reported {
                    tracing::warn!(%error, "verification queue depth read failed");
                }
            }
            if let Err(error) = self.pass(&stop).await {
                tracing::warn!(%error, "verification scheduler pass failed");
            }
            tokio::select! {
                () = stop.cancelled() => return,
                () = tokio::time::sleep(self.poll_interval) => {}
            }
        }
    }

    /// Schedule every due occurrence PostgreSQL considers due until `stop` is
    /// cancelled.
    ///
    /// Returns the ticks committed. Each tenant gets at most
    /// [`TICKS_PER_TENANT`] occurrences per pass, so one tenant's backlog
    /// cannot starve another's schedule; a tenant whose transaction fails is
    /// logged and skipped without stopping the pass. No tenant or occurrence
    /// starts once `stop` is cancelled.
    ///
    /// # Errors
    /// Returns [`SqlError`] when the cross-tenant due list cannot be read.
    ///
    /// # Cancellation
    /// See [`Self::schedule_tenant`]: cancellation rolls back an uncommitted
    /// occurrence and never interrupts a selected commit.
    pub async fn pass(&self, stop: &CancellationToken) -> Result<Vec<ScheduleTick>, SqlError> {
        let tenants = tokio::select! {
            biased;
            () = stop.cancelled() => return Ok(Vec::new()),
            tenants = self.queue.tenants_with_due_bindings(&self.operator, TENANTS_PER_PASS) => tenants?,
        };
        let mut ticks = Vec::new();
        for tenant in tenants {
            if stop.is_cancelled() {
                break;
            }
            if let Err(error) = self.schedule_tenant(tenant, stop, &mut ticks).await {
                tracing::warn!(%tenant, %error, "tenant verification schedule failed");
            }
        }
        Ok(ticks)
    }

    /// Commit up to [`TICKS_PER_TENANT`] due occurrences of `tenant`, one
    /// transaction each, appending every committed tick to `ticks`.
    ///
    /// Selecting an occurrence's commit is its linearization point. Opening
    /// the transaction and applying the atomic run insert and cursor advance
    /// race `stop`; if `stop` wins, the dropped transaction rolls back and the
    /// method returns. Once the commit is selected it is awaited without
    /// cancellation, so its result is always known: a committed tick is
    /// recorded as admitted before scheduler closure, and no later occurrence
    /// starts when `stop` arrived meanwhile.
    ///
    /// # Errors
    /// Returns [`SqlError`] when a tenant transaction fails; earlier ticks
    /// stay committed.
    ///
    /// # Cancellation
    /// Cancellation before commit selection rolls back the in-progress
    /// occurrence; a selected commit completes before returning.
    async fn schedule_tenant(
        &self,
        tenant: DataTenantId,
        stop: &CancellationToken,
        ticks: &mut Vec<ScheduleTick>,
    ) -> Result<(), SqlError> {
        for _ in 0..TICKS_PER_TENANT {
            let applied = async {
                let mut conn = self.postgres.tenant_conn(tenant).await?;
                let tick = self.queue.schedule_next_due(&mut conn).await?;
                Ok::<_, SqlError>(tick.map(|tick| (conn, tick)))
            };
            let applied = tokio::select! {
                biased;
                () = stop.cancelled() => return Ok(()),
                applied = applied => applied?,
            };
            let Some((conn, tick)) = applied else {
                return Ok(());
            };
            conn.commit().await?;
            metrics::counter!(
                crate::app::metrics::VERIFICATION_SCHEDULE_TICKS_TOTAL,
                "outcome" => tick_label(tick.outcome)
            )
            .increment(1);
            ticks.push(tick);
            if stop.is_cancelled() {
                return Ok(());
            }
        }
        Ok(())
    }

    /// Publish the cross-tenant queue depth gauges.
    ///
    /// # Errors
    /// Returns [`SqlError`] when the depth query fails.
    async fn report_depth(&self) -> Result<(), SqlError> {
        let depth = self.queue.queue_depth(&self.operator).await?;
        record_depth("runs", depth.runs);
        record_depth("dispatches", depth.dispatches);
        Ok(())
    }
}

/// Set the depth gauge of one queue's three non-terminal states.
///
/// Counts saturate at `u32::MAX`, far above any real queue depth.
fn record_depth(queue: &'static str, counts: QueueCounts) {
    for (status, count) in [
        ("pending", counts.pending),
        ("retrying", counts.retrying),
        ("running", counts.running),
    ] {
        metrics::gauge!(
            crate::app::metrics::VERIFICATION_QUEUE_DEPTH,
            "queue" => queue,
            "status" => status
        )
        .set(f64::from(u32::try_from(count).unwrap_or(u32::MAX)));
    }
}

/// Stable telemetry label of one schedule outcome.
const fn tick_label(outcome: ScheduleOutcome) -> &'static str {
    match outcome {
        ScheduleOutcome::Enqueued(_) => "enqueued",
        ScheduleOutcome::AlreadyEnqueued(_) => "already_enqueued",
        ScheduleOutcome::Skipped(ScheduleSkip::Missed) => "missed",
        ScheduleOutcome::Skipped(ScheduleSkip::Inactive) => "inactive",
        ScheduleOutcome::Skipped(ScheduleSkip::Refused(_)) => "refused",
        ScheduleOutcome::Skipped(ScheduleSkip::InvalidSchedule) => "invalid_schedule",
    }
}
