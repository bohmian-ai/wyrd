//! Turns due binding schedules and expired pending groups into Verifier
//! runs.
//!
//! [`VerificationScheduler`] lists tenants with a due scheduled binding
//! through the operator pool, then decides each tenant's due bindings one
//! occurrence per short tenant transaction: [`VerifierExecutionQueue::claim_due`]
//! locks the occurrence with `SKIP LOCKED`, [`BackgroundAuthority`] resolves
//! the owner/SYSTEM authority a run would be accepted under, and
//! [`VerifierExecutionQueue::settle_due`] decides it. A tick that calls for
//! no new run moves the cursor and commits. A run is prepared, never
//! written: the transaction rolls back, [`DriftEngine::select`] freezes its
//! observation population, and its schedule input record is staged with
//! its [`Queued`] admission on the Scribe and queue slots reserved before
//! the claim. Only once Scribe acknowledges that record does the queue
//! writer admit the run and move the expected cursor in one transaction, so
//! no run is claimable before its input is durable.
//!
//! In a process hosting Verifier blocks a separate group loop hands expired
//! pending groups off through [`GroupPoller`]: per item it takes one permit
//! of the shared execution capacity and the same two slots, then in one
//! short transaction claims the pending row with
//! [`VerifierExecutionQueue::claim_due_group`] and pins one snapshot. A new
//! snapshot is prepared and staged like a scheduled run, its admission
//! consuming the pending row's generation, so newer activity stays pending;
//! an existing snapshot consumes the row and commits. The permit is
//! released at handoff, before any Verifier runs. An inactive owner or a
//! rejected owner authority is recorded as one terminal errored group
//! execution in that same transaction, which also consumes the row. A
//! retryable failure rolls back and ends that tenant's group pass, leaving
//! the item pending.
//!
//! A bounded in-flight set keeps this process from preparing a source again
//! while its staged candidate is outstanding: each claim is held by the
//! candidate's Scribe write and queued admission and released when the
//! admission is written or its input record is lost. Another process may
//! still prepare the same source; the queue's occurrence and snapshot
//! dedupe and the expected cursor admit at most one run, and the loser's
//! input record stays an unreferenced row. A crash before admission loses
//! nothing durable: the source stays due and is prepared again.

use std::collections::HashSet;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use tokio::sync::Semaphore;
use tokio_util::sync::CancellationToken;
use wyrd_runtime::outbox::Reservation;
use wyrd_runtime::principal::PrincipalId;
use wyrd_spec::DataTenantId;
use wyrd_spec::error::WyrdError;
use wyrd_spec::ids::{BindingId, ExecutionId, GroupId};
use wyrd_spec::verification::VerificationError;
use wyrd_sql::queries::auth::service_accounts::system_principal_id;
use wyrd_sql::queries::verifier_executions::{
    Admission, DueBinding, DueGroup, DueSettlement, GroupedAdmission, Prepared, QueueCounts,
    ScheduleOutcome, ScheduleSkip, ScheduleTick, ScheduledAdmission, VerifierExecutionQueue,
};
use wyrd_sql::{OperatorPool, SqlError, TenantConn, WyrdPostgres};

use super::authority::{AuthorityRefusal, BackgroundAuthority};
use super::drift::DriftEngine;
use super::groups::{GroupPollError, GroupPoller};
use super::inputs::ScheduleSelector;
use super::results::{InputRecord, ResultPayloadBuilder, ResultPayloadError, ResultRow, ResultRun};
use super::submissions::{Queued, SubmissionSink};
use crate::scribe_outbox::{ScribeSink, ScribeWrite};

#[cfg(feature = "test-support")]
use super::CapabilityCrash;
#[cfg(feature = "test-support")]
use super::health::RuntimeCapability;

/// Tenants examined per scheduler pass, most overdue first.
const TENANTS_PER_PASS: i64 = 64;

/// Occurrences or pending groups one tenant may hand off per pass before the
/// next tenant runs.
const TICKS_PER_TENANT: usize = 32;

/// One handoff of an expired pending group.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GroupHandoff {
    /// The pinned snapshot was staged as this group execution, admitted, and
    /// the pending generation consumed, once Scribe acknowledges its input.
    Enqueued(ExecutionId),
    /// An identical snapshot's group execution already existed.
    AlreadyEnqueued(ExecutionId),
    /// The owner was inactive or refused; this terminal errored group
    /// execution records why, with nothing evaluated.
    Errored(ExecutionId),
}

/// A due source this process prepares work from.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum Source {
    /// One scheduled binding's due occurrence.
    Binding(DataTenantId, BindingId),
    /// One binding's pending group.
    Group(DataTenantId, BindingId, GroupId),
}

/// The sources this process has staged candidates for that are not yet
/// admitted or lost. Each entry is held by a candidate holding a reserved
/// queue slot, so the set is bounded by the queue outbox's capacity.
#[derive(Debug, Default)]
struct InFlightSet(Mutex<HashSet<Source>>);

impl InFlightSet {
    /// Claim `source`, or `None` when a candidate of it is outstanding.
    fn claim(self: &Arc<Self>, source: Source) -> Option<Arc<InFlight>> {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(source.clone())
            .then(|| {
                Arc::new(InFlight {
                    source,
                    set: Arc::clone(self),
                })
            })
    }

    /// The claimed sources of `tenant` matching `pick`, which the next claim
    /// passes over.
    fn claimed<T>(&self, pick: impl Fn(&Source) -> Option<T>) -> Vec<T> {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .filter_map(pick)
            .collect()
    }
}

/// One process-local claim on a due source, shared by its candidate's
/// Scribe write and queued admission and released when the last drops: once
/// the admission is written, its input record is lost, or the candidate is
/// abandoned before staging.
#[derive(Debug)]
pub struct InFlight {
    /// The claimed source.
    source: Source,
    /// The set the claim is released from.
    set: Arc<InFlightSet>,
}

impl Drop for InFlight {
    fn drop(&mut self) {
        self.set
            .0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(&self.source);
    }
}

/// What one claimed source's transaction decided, reported as `O`.
enum Step<'c, O> {
    /// Settled without new work; the connection commits it.
    Settled(TenantConn<'c>, O),
    /// New work and its claim, rolled back, to stage with its input row.
    Candidate(O, Box<Queued>, ResultRow),
}

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
    /// Freezes each scheduled run's observation population.
    drift: DriftEngine,
    /// Idle wait between passes.
    poll_interval: Duration,
    /// Pending group handoff, present only where Verifier blocks are hosted.
    groups: Option<Groups>,
    /// Sources whose staged candidates are outstanding.
    in_flight: Arc<InFlightSet>,
    /// Test-only crash switch.
    #[cfg(feature = "test-support")]
    crash: Option<CapabilityCrash>,
    /// Test-only: treat every claimed pending group as overdue.
    #[cfg(feature = "test-support")]
    overdue: bool,
}

impl VerificationScheduler {
    /// Build a scheduler over the Wyrd Postgres owner, the operator pool, the
    /// queue, the background authority resolver, and the Drift engine that
    /// freezes each scheduled run's population.
    ///
    /// The scheduler carries no clock: every due decision is PostgreSQL's, and
    /// `poll_interval` is only this process's idle wait between passes.
    #[must_use]
    pub fn new(
        postgres: WyrdPostgres,
        operator: OperatorPool,
        queue: VerifierExecutionQueue,
        authority: BackgroundAuthority,
        drift: DriftEngine,
        poll_interval: Duration,
    ) -> Self {
        Self {
            postgres,
            operator,
            queue,
            authority,
            drift,
            poll_interval,
            groups: None,
            in_flight: Arc::default(),
            #[cfg(feature = "test-support")]
            crash: None,
            #[cfg(feature = "test-support")]
            overdue: false,
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

    /// Treat every claimed pending group as more than five minutes past due.
    ///
    /// A group's due time is also its selection cutoff, so a test cannot age
    /// a group past the incomplete-snapshot bound without excluding the rows
    /// it wrote; this reaches that bound's error over current rows.
    #[cfg(feature = "test-support")]
    #[must_use]
    pub fn with_overdue_groups(mut self) -> Self {
        self.overdue = true;
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
    #[tracing::instrument(name = "verification.schedule.pass", skip_all, fields(pass = "cron"))]
    pub async fn pass(&self, stop: &CancellationToken) -> Result<Vec<ScheduleTick>, SqlError> {
        let started = Instant::now();
        let tenants = tokio::select! {
            biased;
            () = stop.cancelled() => return Ok(Vec::new()),
            tenants = self.queue.tenants_with_due_bindings(&self.operator, TENANTS_PER_PASS) => tenants,
        };
        let tenants = match tenants {
            Ok(tenants) => tenants,
            Err(error) => return record_pass("cron", started, Err(SqlError::from(error))),
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
        record_pass("cron", started, Ok(ticks))
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
    #[tracing::instrument(name = "verification.schedule.pass", skip_all, fields(pass = "group"))]
    pub async fn group_pass(
        &self,
        stop: &CancellationToken,
    ) -> Result<Vec<GroupHandoff>, SqlError> {
        let Some(groups) = &self.groups else {
            return Ok(Vec::new());
        };
        let started = Instant::now();
        let tenants = tokio::select! {
            biased;
            () = stop.cancelled() => return Ok(Vec::new()),
            tenants = self.queue.tenants_with_due_groups(&self.operator, TENANTS_PER_PASS) => tenants,
        };
        let tenants = match tenants {
            Ok(tenants) => tenants,
            Err(error) => return record_pass("group", started, Err(SqlError::from(error))),
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
        record_pass("group", started, Ok(handed))
    }

    /// Decide up to [`TICKS_PER_TENANT`] due occurrences of `tenant`, one
    /// transaction each, appending every tick to `ticks`.
    ///
    /// Each occurrence first reserves one Scribe outbox slot and one queue
    /// slot; none ends the tenant's pass before any claim. Bindings with an
    /// outstanding candidate are passed over. A settled tick commits. A run
    /// is claimed in flight, rolled back, its population selected, and its
    /// input record staged with its admission, reported as
    /// [`ScheduleOutcome::Enqueued`]; a failed selection releases the claim,
    /// leaving the occurrence due, and ends the tenant's pass.
    ///
    /// Committing a settled tick or staging a candidate is the occurrence's
    /// linearization point. Everything before it races `stop`; if `stop`
    /// wins, the dropped transaction rolls back and the method returns. A
    /// selected commit is awaited without cancellation, so its result is
    /// always known, and no later occurrence starts when `stop` arrived
    /// meanwhile.
    ///
    /// # Errors
    /// Returns [`SqlError`] when a tenant transaction fails; earlier ticks
    /// stay committed or staged.
    ///
    /// # Cancellation
    /// Cancellation before the linearization point rolls back the
    /// in-progress occurrence; a selected commit completes before returning.
    async fn schedule_tenant(
        &self,
        tenant: DataTenantId,
        stop: &CancellationToken,
        ticks: &mut Vec<ScheduleTick>,
    ) -> Result<(), SqlError> {
        for _ in 0..TICKS_PER_TENANT {
            let Ok(slots) = self.drift.reserve() else {
                tracing::warn!(%tenant, "the Scribe or queue outbox share is full; occurrences stay due");
                return Ok(());
            };
            let skip = self.in_flight.claimed(|source| match source {
                Source::Binding(owner, binding) if *owner == tenant => Some(*binding),
                _ => None,
            });
            let applied = async {
                let mut conn = self.postgres.tenant_conn(tenant).await?;
                let Some(due) = self.queue.claim_due(&mut conn, &skip).await? else {
                    return Ok::<_, SqlError>(None);
                };
                let authority = match due.owner() {
                    Some((owner, root)) => {
                        self.authority.resolve(&mut conn, owner, root).await?.ok()
                    }
                    None => None,
                };
                let prepared = due.clone();
                let admission = match self.queue.settle_due(&mut conn, due, authority).await? {
                    DueSettlement::Settled(tick) => return Ok(Some(Step::Settled(conn, tick))),
                    DueSettlement::Candidate(admission) => *admission,
                };
                let Some(claim) = self
                    .in_flight
                    .claim(Source::Binding(tenant, admission.binding_id))
                else {
                    return Ok(None);
                };
                drop(conn);
                match self.selector(tenant, &prepared, &admission).await {
                    Ok(row) => Ok(Some(Step::Candidate(
                        admission.tick(ScheduleOutcome::Enqueued(admission.execution_id)),
                        Box::new(Queued {
                            admission: Admission::Scheduled(admission),
                            claim: Some(claim),
                        }),
                        row,
                    ))),
                    Err(error) => {
                        tracing::warn!(%tenant, binding_id = %prepared.binding_id, %error, "scheduled population selection failed; the occurrence stays due");
                        Ok(None)
                    }
                }
            };
            let applied = tokio::select! {
                biased;
                () = stop.cancelled() => return Ok(()),
                applied = applied => applied?,
            };
            let tick = match applied {
                None => return Ok(()),
                Some(Step::Settled(conn, tick)) => {
                    conn.commit().await?;
                    tick
                }
                Some(Step::Candidate(tick, queued, row)) => {
                    stage(tenant, slots, queued, row);
                    tick
                }
            };
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

    /// Freeze the population of `admission`, the run prepared for `due`, as
    /// its schedule input record.
    ///
    /// # Errors
    /// Returns the selection read's failure, or the record's encoding
    /// failure.
    async fn selector(
        &self,
        tenant: DataTenantId,
        due: &DueBinding,
        admission: &ScheduledAdmission,
    ) -> Result<ResultRow, String> {
        let window = admission.window;
        let record_ids = self
            .drift
            .select(tenant, admission.authority(), &due.card_uid, &window)
            .await?;
        let selector = ScheduleSelector { window, record_ids };
        let now = chrono::Utc::now();
        ResultPayloadBuilder::new(
            ResultRun {
                execution_id: admission.execution_id,
                run_id: None,
                // The inputs row records no Verifier version.
                verifier_version: "",
                card_uid: &due.card_uid,
                owner_card_uid: due.owner().map(|(_, root)| root),
                group_id: None,
                deployment_environment: None,
                binding_id: Some(due.binding_id),
                trigger: None,
                window: Some(&window),
            },
            &due.verifier_card_uid,
            now,
            now,
        )
        .inputs(InputRecord::Schedule(&selector))
        .map_err(|error: ResultPayloadError| error.to_string())
    }

    /// Hand off up to [`TICKS_PER_TENANT`] expired pending groups of
    /// `tenant`, one transaction each, appending every outcome to `handed`.
    ///
    /// Each item first takes one permit of the shared execution capacity and
    /// reserves one Scribe outbox slot and one queue slot; no permit or slot
    /// ends the tenant's pass before any claim or read. Groups with an
    /// outstanding candidate are passed over. The transaction claims one
    /// pending row and decides it through [`Self::hand_off`]: a settled item
    /// consumes the row and commits; a new snapshot is claimed in flight,
    /// rolled back, and staged with its admission, which consumes the row's
    /// generation once Scribe acknowledges its input. The connection and
    /// permit are released before any Verifier runs. An item `hand_off`
    /// leaves pending rolls back and ends the tenant's pass, so the same
    /// oldest row is not reclaimed until the next pass.
    ///
    /// # Errors
    /// Returns [`SqlError`] when a tenant transaction fails; earlier handoffs
    /// stay committed or staged.
    ///
    /// # Cancellation
    /// As [`Self::schedule_tenant`]: cancellation before the linearization
    /// point rolls back the in-progress handoff.
    async fn hand_off_groups(
        &self,
        groups: &Groups,
        tenant: DataTenantId,
        stop: &CancellationToken,
        handed: &mut Vec<GroupHandoff>,
    ) -> Result<(), SqlError> {
        for _ in 0..TICKS_PER_TENANT {
            let Ok(_permit) = groups.capacity.try_acquire() else {
                return Ok(());
            };
            let Ok(slots) = groups.poller.reserve() else {
                tracing::warn!(%tenant, "the Scribe or queue outbox share is full; groups stay pending");
                return Ok(());
            };
            let skip = self.in_flight.claimed(|source| match source {
                Source::Group(owner, binding, group) if *owner == tenant => {
                    Some((*binding, group.clone()))
                }
                _ => None,
            });
            let started = Instant::now();
            let applied = async {
                let mut conn = self.postgres.tenant_conn(tenant).await?;
                let Some(due) = self.queue.claim_due_group(&mut conn, &skip).await? else {
                    return Ok::<_, SqlError>(None);
                };
                #[cfg(feature = "test-support")]
                let due = DueGroup {
                    overdue: due.overdue || self.overdue,
                    ..due
                };
                match self.hand_off(groups, &mut conn, &due).await? {
                    None => Ok(None),
                    Some(Ok(handoff)) => {
                        self.queue.consume_group(&mut conn, &due).await?;
                        Ok(Some(Step::Settled(conn, handoff)))
                    }
                    Some(Err((admission, row))) => {
                        let Some(claim) = self.in_flight.claim(Source::Group(
                            tenant,
                            due.binding_id,
                            due.group_id.clone(),
                        )) else {
                            return Ok(None);
                        };
                        drop(conn);
                        Ok(Some(Step::Candidate(
                            GroupHandoff::Enqueued(admission.execution_id),
                            Box::new(Queued {
                                admission: Admission::Grouped(admission),
                                claim: Some(claim),
                            }),
                            row,
                        )))
                    }
                }
            };
            let applied = tokio::select! {
                biased;
                () = stop.cancelled() => return Ok(()),
                applied = applied => applied,
            };
            let handoff = match applied {
                Ok(Some(Step::Settled(conn, handoff))) => {
                    record_pass("group_handoff", started, conn.commit().await)?;
                    handoff
                }
                Ok(Some(Step::Candidate(handoff, queued, row))) => {
                    stage(tenant, slots, queued, row);
                    record_pass("group_handoff", started, Ok(handoff))?
                }
                other => return record_pass("group_handoff", started, other.map(drop)),
            };
            metrics::counter!(
                crate::app::metrics::VERIFICATION_SCHEDULE_TICKS_TOTAL,
                "outcome" => group_label(handoff)
            )
            .increment(1);
            handed.push(handoff);
            if stop.is_cancelled() {
                return Ok(());
            }
        }
        Ok(())
    }

    /// Decide the claimed pending group `due` on `conn`: a settled handoff
    /// whose row the caller consumes, a prepared new snapshot and its
    /// selector input record to stage, or `None` to leave it pending.
    ///
    /// An active owner's recovered authority pins and prepares one snapshot
    /// (or finds the identical snapshot's execution), read as the tenant's
    /// SYSTEM principal. An inactive owner or a rejected owner authority is
    /// recorded through [`VerifierExecutionQueue::refuse_group`] as one
    /// terminal errored execution, serving the owner's stored principal and
    /// recorded by SYSTEM, as is a snapshot still incomplete (a result
    /// visible without its input or output) more than five minutes past due, with
    /// `WYRD_VERIFICATION_409_SNAPSHOT_INCOMPLETE` naming the executions.
    /// Everything else is a Wyrd-side blockage that is
    /// traced and leaves the group pending: an owner with no Card-bound
    /// principal, a tenant without SYSTEM, unavailable authority, a non-auth
    /// enqueue refusal, a failed, refused or degraded read, or an
    /// incomplete snapshot not yet five minutes past due.
    ///
    /// # Errors
    /// Returns [`SqlError`] when a statement fails, including the refusal's
    /// insert; the caller's transaction is then unusable and rolls back.
    async fn hand_off(
        &self,
        groups: &Groups,
        conn: &mut TenantConn<'_>,
        due: &DueGroup,
    ) -> Result<Option<Result<GroupHandoff, (GroupedAdmission, ResultRow)>>, SqlError> {
        let pending = |label: &'static str, reason: &dyn std::fmt::Display| {
            metrics::counter!(
                crate::app::metrics::VERIFICATION_GROUP_HANDOFFS_BLOCKED_TOTAL,
                "reason" => label
            )
            .increment(1);
            tracing::warn!(
                binding_id = %due.binding_id,
                group_id = %due.group_id,
                %reason,
                "group handoff blocked; the group stays pending"
            );
            None
        };
        let Some(owner) = due.owner else {
            return Ok(pending(
                "no_owner_principal",
                &"the owner family has no runtime account",
            ));
        };
        let Some(system) = system_principal_id(conn)
            .await
            .map_err(SqlError::Query)?
            .map(PrincipalId::new)
        else {
            return Ok(pending(
                "no_system_principal",
                &"the tenant has no SYSTEM principal",
            ));
        };
        let refusal = 'refusal: {
            if !due.active {
                break 'refusal WyrdError::CredentialRevoked {
                    message: "the binding owner is not runtime-active".to_owned(),
                    details: serde_json::json!({}),
                };
            }
            let authority = match self
                .authority
                .resolve(conn, owner, &due.owner_card_uid)
                .await?
            {
                Ok(authority) => authority,
                Err(AuthorityRefusal::Rejected(error)) => break 'refusal error,
                Err(AuthorityRefusal::Unavailable) => {
                    return Ok(pending(
                        "authority_unavailable",
                        &"the owner authority is unavailable",
                    ));
                }
            };
            match groups
                .poller
                .poll(conn, &self.queue, due, &authority, system)
                .await
            {
                Ok(Prepared::Candidate(candidate)) => return Ok(Some(Err(candidate))),
                Ok(Prepared::Existing(id)) => {
                    return Ok(Some(Ok(GroupHandoff::AlreadyEnqueued(id))));
                }
                Ok(Prepared::Refused(refusal)) => {
                    return Ok(pending("enqueue_refused", &format!("{refusal:?}")));
                }
                Err(GroupPollError::Sql(error)) => return Err(SqlError::from(error)),
                Err(GroupPollError::Incomplete(executions)) if due.overdue => {
                    let ids: Vec<_> = executions.iter().map(ToString::to_string).collect();
                    break 'refusal WyrdError::VerificationSnapshotIncomplete {
                        message: format!(
                            "group snapshot still saw results without their inputs or outputs \
                             more than five minutes past due for executions {}",
                            ids.join(", ")
                        ),
                        details: serde_json::json!({ "execution_ids": executions }),
                    };
                }
                Err(error @ GroupPollError::Read(_)) => {
                    return Ok(pending("snapshot_read", &error));
                }
                Err(error @ GroupPollError::Incomplete(_)) => {
                    return Ok(pending("snapshot_incomplete", &error));
                }
            }
        };
        let execution = self
            .queue
            .refuse_group(conn, due, owner, system, &VerificationError::from(&refusal))
            .await
            .inspect_err(|error| {
                tracing::warn!(
                    binding_id = %due.binding_id,
                    group_id = %due.group_id,
                    %error,
                    "recording the group refusal failed; the group stays pending"
                );
            })?;
        tracing::info!(
            binding_id = %due.binding_id,
            group_id = %due.group_id,
            %execution,
            reason = %refusal,
            "group refused; recorded a terminal errored execution"
        );
        Ok(Some(Ok(GroupHandoff::Errored(execution))))
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

/// Stage `queued`'s input `row` on the reserved Scribe slot of `slots`,
/// carrying the reserved queue slot that admits it once Scribe acknowledges
/// the row.
fn stage(
    tenant: DataTenantId,
    (scribe, queue): (Reservation<ScribeSink>, Reservation<SubmissionSink>),
    queued: Box<Queued>,
    row: ResultRow,
) {
    scribe.stage(
        tenant,
        ScribeWrite::ExecutionInputs {
            row,
            attribution: queued.admission.attribution(),
            queued: Some((queue, queued)),
        },
    );
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

/// Record one scheduler pass or handoff of `pass` begun at `started` into
/// the schedule-pass histogram, labelled by whether `result` succeeded, and
/// return `result` unchanged so the caller can propagate it with `?`.
///
/// # Errors
/// Returns `result`'s own error untouched.
fn record_pass<T>(
    pass: &'static str,
    started: Instant,
    result: Result<T, SqlError>,
) -> Result<T, SqlError> {
    metrics::histogram!(
        crate::app::metrics::VERIFICATION_SCHEDULE_PASS_SECONDS,
        "pass" => pass,
        "outcome" => if result.is_ok() { "ok" } else { "error" }
    )
    .record(started.elapsed().as_secs_f64());
    result
}

/// Stable telemetry label of one committed group handoff.
const fn group_label(handoff: GroupHandoff) -> &'static str {
    match handoff {
        GroupHandoff::Enqueued(_) => "group_enqueued",
        GroupHandoff::AlreadyEnqueued(_) => "group_already_enqueued",
        GroupHandoff::Errored(_) => "group_errored",
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

#[cfg(test)]
mod tests {
    //! Scheduler pass telemetry.

    use super::*;

    /// A pass is timed under its `pass` with the outcome of its result, and
    /// the result passes through unchanged.
    ///
    /// # Panics
    /// Panics when a result changes or a labelled sample is missing.
    #[test]
    fn passes_are_timed_by_pass_and_outcome() {
        let recorder = metrics_exporter_prometheus::PrometheusBuilder::new().build_recorder();
        let handle = recorder.handle();
        let _guard = metrics::set_default_local_recorder(&recorder);
        assert_eq!(record_pass("cron", Instant::now(), Ok(3)).ok(), Some(3));
        let failed: Result<(), _> = Err(SqlError::from(sqlx::Error::RowNotFound));
        assert!(record_pass("group_handoff", Instant::now(), failed).is_err());
        let rendered = handle.render();
        for (pass, outcome) in [("cron", "ok"), ("group_handoff", "error")] {
            let sample = format!(
                "{}_count{{pass=\"{pass}\",outcome=\"{outcome}\"}} 1",
                crate::app::metrics::VERIFICATION_SCHEDULE_PASS_SECONDS
            );
            assert!(
                rendered.contains(&sample),
                "{sample} missing from {rendered}"
            );
        }
    }
}
