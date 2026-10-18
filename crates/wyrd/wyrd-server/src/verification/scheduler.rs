//! Turns due binding schedules into Verifier runs.
//!
//! [`VerificationScheduler`] lists tenants with a due scheduled binding
//! through the operator pool, then advances each tenant's due bindings one
//! occurrence per short tenant transaction: [`VerifierExecutionQueue::claim_due`]
//! decides the occurrence, [`BackgroundAuthority`] resolves the owner/SYSTEM
//! authority a run would be accepted under, and
//! [`VerifierExecutionQueue::schedule_due`] settles it. The queue locks each binding with
//! `SKIP LOCKED` and dedupes an occurrence's run, so concurrent schedulers on
//! many processes, duplicate ticks, and a restart mid-pass never create a
//! second run for one occurrence.
//!
//! In a process hosting Verifier blocks a separate group loop hands expired
//! pending groups off through [`GroupPoller`]: per item it takes one permit
//! of the shared execution capacity, then in one short transaction claims
//! the pending row with [`VerifierExecutionQueue::claim_due_group`], pins
//! and enqueues one snapshot, consumes the row, and commits; the permit is
//! released at handoff, before any Verifier runs. A failure rolls back and
//! ends that tenant's group pass, leaving the item pending.

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::Semaphore;
use tokio_util::sync::CancellationToken;
use wyrd_spec::DataTenantId;
use wyrd_sql::queries::verifier_executions::{
    EnqueueOutcome, QueueCounts, ScheduleOutcome, ScheduleSkip, ScheduleTick,
    VerifierExecutionQueue,
};
use wyrd_sql::{OperatorPool, SqlError, WyrdPostgres};

use super::authority::BackgroundAuthority;
use super::groups::{GroupPollError, GroupPoller};

#[cfg(feature = "test-support")]
use super::CapabilityCrash;
#[cfg(feature = "test-support")]
use super::health::RuntimeCapability;

/// Tenants examined per scheduler pass, most overdue first.
const TENANTS_PER_PASS: i64 = 64;

/// Occurrences or pending groups one tenant may hand off per pass before the
/// next tenant runs.
const TICKS_PER_TENANT: usize = 32;

/// The group loop's snapshot reader and the shared execution capacity each
/// handoff takes one permit of.
struct Groups {
    /// Pins and enqueues one pending group's snapshot.
    poller: GroupPoller,
    /// Execution capacity shared with the runner's claim loop.
    capacity: Arc<Semaphore>,
}

/// Owner of the binding-schedule loop.
pub struct VerificationScheduler {
    /// Wyrd Postgres owner that opens every tenant-scoped schedule transaction.
    postgres: WyrdPostgres,
    /// Operator pool for the cross-tenant due list and queue depth.
    operator: OperatorPool,
    /// Queue policies and transitions.
    queue: VerifierExecutionQueue,
    /// Resolves the owner/SYSTEM authority each scheduled run is accepted under.
    authority: BackgroundAuthority,
    /// Idle wait between passes.
    poll_interval: Duration,
    /// Pending group handoff, present only where Verifier blocks are hosted.
    groups: Option<Groups>,
    /// Test-only crash switch.
    #[cfg(feature = "test-support")]
    crash: Option<CapabilityCrash>,
}

impl VerificationScheduler {
    /// Build a scheduler over the Wyrd Postgres owner, the operator pool, the
    /// queue, and the background authority resolver.
    ///
    /// The scheduler carries no clock: every due decision is PostgreSQL's, and
    /// `poll_interval` is only this process's idle wait between passes.
    #[must_use]
    pub fn new(
        postgres: WyrdPostgres,
        operator: OperatorPool,
        queue: VerifierExecutionQueue,
        authority: BackgroundAuthority,
        poll_interval: Duration,
    ) -> Self {
        Self {
            postgres,
            operator,
            queue,
            authority,
            poll_interval,
            groups: None,
            #[cfg(feature = "test-support")]
            crash: None,
        }
    }

    /// Also hand off expired pending groups through `poller`, taking one
    /// permit of `capacity`, the execution capacity the runner's claim loop
    /// shares, per handoff.
    #[must_use]
    pub fn with_groups(mut self, poller: GroupPoller, capacity: Arc<Semaphore>) -> Self {
        self.groups = Some(Groups { poller, capacity });
        self
    }

    /// Let `crash` panic this scheduler's loop.
    #[cfg(feature = "test-support")]
    #[must_use]
    pub fn with_crash(mut self, crash: CapabilityCrash) -> Self {
        self.crash = Some(crash);
        self
    }

    /// Run the cron loop and, where groups are hosted, the separate group
    /// loop until `stop` is cancelled.
    ///
    /// Each loop finishes its pass before sleeping the poll interval, so slow
    /// group snapshot reads never delay cron occurrences. A failed pass is
    /// logged and retried on the next interval; the loops only return on
    /// `stop`.
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
        tokio::join!(self.cron_loop(&stop), self.group_loop(&stop));
    }

    /// Run cron passes every poll interval until `stop` is cancelled.
    ///
    /// # Panics
    /// Panics under `test-support` when a test armed a scheduler crash.
    ///
    /// # Cancellation
    /// As [`Self::run`].
    async fn cron_loop(&self, stop: &CancellationToken) {
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
            if let Err(error) = self.pass(stop).await {
                tracing::warn!(%error, "verification scheduler pass failed");
            }
            tokio::select! {
                () = stop.cancelled() => return,
                () = tokio::time::sleep(self.poll_interval) => {}
            }
        }
    }

    /// Run group passes every poll interval until `stop` is cancelled; a
    /// process hosting no groups returns at once.
    ///
    /// # Cancellation
    /// As [`Self::run`].
    async fn group_loop(&self, stop: &CancellationToken) {
        if self.groups.is_none() {
            return;
        }
        loop {
            if let Err(error) = self.group_pass(stop).await {
                tracing::warn!(%error, "verification group pass failed");
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

    /// Hand off every expired pending group PostgreSQL considers due until
    /// `stop` is cancelled, returning each committed handoff's outcome.
    ///
    /// Visits at most [`TENANTS_PER_PASS`] most-overdue tenants and hands off
    /// at most [`TICKS_PER_TENANT`] groups per tenant, sequentially. A tenant
    /// whose handoff fails is logged and skipped without stopping the pass. A
    /// process hosting no groups hands off nothing.
    ///
    /// # Errors
    /// Returns [`SqlError`] when the cross-tenant due list cannot be read.
    ///
    /// # Cancellation
    /// As [`Self::pass`].
    pub async fn group_pass(
        &self,
        stop: &CancellationToken,
    ) -> Result<Vec<EnqueueOutcome>, SqlError> {
        let Some(groups) = &self.groups else {
            return Ok(Vec::new());
        };
        let tenants = tokio::select! {
            biased;
            () = stop.cancelled() => return Ok(Vec::new()),
            tenants = self.queue.tenants_with_due_groups(&self.operator, TENANTS_PER_PASS) => tenants?,
        };
        let mut handed = Vec::new();
        for tenant in tenants {
            if stop.is_cancelled() {
                break;
            }
            if let Err(error) = self
                .hand_off_groups(groups, tenant, stop, &mut handed)
                .await
            {
                tracing::warn!(%tenant, %error, "tenant group handoff failed");
            }
        }
        Ok(handed)
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
                let Some(due) = self.queue.claim_due(&mut conn).await? else {
                    return Ok::<_, SqlError>(None);
                };
                let authority = match due.owner() {
                    Some(owner) => self.authority.resolve(&mut conn, owner).await?,
                    None => None,
                };
                let tick = self.queue.schedule_due(&mut conn, due, authority).await?;
                Ok(Some((conn, tick)))
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

    /// Hand off up to [`TICKS_PER_TENANT`] expired pending groups of
    /// `tenant`, one transaction each, appending every committed outcome to
    /// `handed`.
    ///
    /// Each item first takes one permit of the shared execution capacity; no
    /// permit ends the tenant's pass before any claim or read. The
    /// transaction claims one pending row, resolves the owner/SYSTEM
    /// authority, pins and enqueues one snapshot (or finds the execution of
    /// the same snapshot), consumes the row, and commits; the connection and
    /// permit are released before any Verifier runs. A group whose owner has
    /// no authority is consumed without an execution, as a scheduled
    /// occurrence without authority is skipped. A failed or degraded read
    /// rolls back, leaving the group pending, and ends the tenant's pass.
    ///
    /// # Errors
    /// Returns [`SqlError`] when a tenant transaction fails; earlier handoffs
    /// stay committed.
    ///
    /// # Cancellation
    /// As [`Self::schedule_tenant`]: cancellation before commit selection
    /// rolls back the in-progress handoff.
    async fn hand_off_groups(
        &self,
        groups: &Groups,
        tenant: DataTenantId,
        stop: &CancellationToken,
        handed: &mut Vec<EnqueueOutcome>,
    ) -> Result<(), SqlError> {
        for _ in 0..TICKS_PER_TENANT {
            let Ok(_permit) = groups.capacity.try_acquire() else {
                return Ok(());
            };
            let applied = async {
                let mut conn = self.postgres.tenant_conn(tenant).await?;
                let Some(due) = self.queue.claim_due_group(&mut conn).await? else {
                    return Ok::<_, SqlError>(None);
                };
                let authority = match due.owner {
                    Some(owner) => self.authority.resolve(&mut conn, owner).await?,
                    None => None,
                };
                let outcome = match authority {
                    None => None,
                    Some(authority) => {
                        match groups
                            .poller
                            .poll(&mut conn, &self.queue, &due, &authority)
                            .await
                        {
                            Ok(outcome) => Some(outcome),
                            Err(GroupPollError::Sql(error)) => return Err(SqlError::from(error)),
                            Err(error) => {
                                tracing::warn!(
                                    binding_id = %due.binding_id,
                                    group_id = %due.group_id,
                                    %error,
                                    "group snapshot failed; the group stays pending"
                                );
                                return Ok(None);
                            }
                        }
                    }
                };
                self.queue.consume_group(&mut conn, &due).await?;
                Ok(Some((conn, outcome)))
            };
            let applied = tokio::select! {
                biased;
                () = stop.cancelled() => return Ok(()),
                applied = applied => applied?,
            };
            let Some((conn, outcome)) = applied else {
                return Ok(());
            };
            conn.commit().await?;
            metrics::counter!(
                crate::app::metrics::VERIFICATION_SCHEDULE_TICKS_TOTAL,
                "outcome" => group_label(outcome.as_ref())
            )
            .increment(1);
            handed.extend(outcome);
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
        record_depth("executions", depth.executions);
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

/// Stable telemetry label of one group handoff; `None` is a group consumed
/// because its owner had no authority.
const fn group_label(outcome: Option<&EnqueueOutcome>) -> &'static str {
    match outcome {
        Some(EnqueueOutcome::Enqueued(_)) => "group_enqueued",
        Some(EnqueueOutcome::AlreadyEnqueued(_)) => "group_already_enqueued",
        Some(EnqueueOutcome::Refused(_)) => "group_refused",
        None => "group_authority_unavailable",
    }
}

/// Stable telemetry label of one schedule outcome.
const fn tick_label(outcome: ScheduleOutcome) -> &'static str {
    match outcome {
        ScheduleOutcome::Enqueued(_) => "enqueued",
        ScheduleOutcome::AlreadyEnqueued(_) => "already_enqueued",
        ScheduleOutcome::Skipped(ScheduleSkip::Missed) => "missed",
        ScheduleOutcome::Skipped(ScheduleSkip::Inactive) => "inactive",
        ScheduleOutcome::Skipped(ScheduleSkip::AuthorityUnavailable) => "authority_unavailable",
        ScheduleOutcome::Skipped(ScheduleSkip::Refused(_)) => "refused",
        ScheduleOutcome::Skipped(ScheduleSkip::InvalidSchedule) => "invalid_schedule",
    }
}
