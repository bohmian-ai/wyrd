//! Supervision for Forge leadership, hinted promotion and the leader timer.

use std::sync::Arc;
#[cfg(feature = "test-support")]
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::{AtomicBool, Ordering};

use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use super::error::ForgeError;
use super::leadership::LEADER_HEARTBEAT;
use super::{Forge, ForgeWorker, ForgeWorkerConfig};
use crate::maintenance::StagingFileCommitted;

/// Test-tier control and observation for a supervised production scheduler loop.
///
/// The trigger only wakes the already-running [`Forge::run`] loop. It never
/// constructs a scheduler, claims work, or executes a planning pass itself.
#[derive(Clone, Default)]
#[cfg(feature = "test-support")]
pub struct ForgeSchedulerTrigger {
    /// Wakeup consumed by the production supervisor select loop.
    requested: Arc<tokio::sync::Notify>,
    /// Wakeup consumed by the leader maintenance timer loop.
    maintenance: Arc<tokio::sync::Notify>,
    /// Completed scheduler passes observed after their durable result returns.
    completed: Arc<AtomicUsize>,
    /// Wakeup for deterministic test waits on completed passes.
    completed_ready: Arc<tokio::sync::Notify>,
    /// Optional stable scheduler owner shared across reconstructed test supervisors.
    #[cfg(feature = "test-support")]
    owner: Arc<std::sync::Mutex<Option<Uuid>>>,
}

#[cfg(feature = "test-support")]
impl ForgeSchedulerTrigger {
    /// Construct an idle control for one supervised Forge owner.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Construct a trigger whose real supervisor renews one stable test lease owner.
    ///
    /// This preserves production scheduling and supervision while allowing a
    /// restart fixture to reconstruct its Forge graph before the prior lease TTL.
    /// The scenario then owns every promotion sweep: the supervisor's
    /// heartbeats only renew the leader term, and debt is swept only on the
    /// passes [`Self::request_pass`] asks for.
    #[cfg(feature = "test-support")]
    #[must_use]
    pub fn with_owner_for_test(owner: Uuid) -> Self {
        let trigger = Self::default();
        *trigger
            .owner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(owner);
        trigger
    }

    /// Return the stable fixture owner selected for this supervised scheduler.
    #[cfg(feature = "test-support")]
    pub(super) fn owner_for_test(&self) -> Option<Uuid> {
        *self
            .owner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Request one pass from the production scheduler loop.
    pub fn request_pass(&self) {
        self.requested.notify_one();
    }

    /// Request one leader maintenance pass from the production timer loop.
    ///
    /// The pass runs manifest rewrite, snapshot expiry and cleanup exactly as
    /// one timer tick does, and counts as one completed pass.
    pub fn request_maintenance(&self) {
        self.maintenance.notify_one();
    }

    /// Return the number of production scheduler passes that have returned.
    #[must_use]
    pub fn completed_passes(&self) -> usize {
        self.completed.load(Ordering::Acquire)
    }

    /// Wait until at least `expected` supervised passes have returned.
    ///
    /// The notification is registered before inspecting the atomic count, so
    /// a scheduler result racing this wait cannot be lost.
    pub async fn wait_for_passes_at_least(&self, expected: usize) {
        loop {
            let notified = self.completed_ready.notified();
            if self.completed_passes() >= expected {
                return;
            }
            notified.await;
        }
    }

    /// Wait for one request without exposing the scheduler loop to tests.
    async fn wait_for_request(&self) {
        self.requested.notified().await;
    }

    /// Wait for one maintenance request without exposing the timer loop.
    async fn wait_for_maintenance(&self) {
        self.maintenance.notified().await;
    }

    /// Record that the real supervised pass returned, including handled failures.
    fn record_completed_pass(&self) {
        self.completed.fetch_add(1, Ordering::AcqRel);
        self.completed_ready.notify_waiters();
    }
}

/// Clears one Forge role's readiness when its supervised loop stops.
///
/// Every exit — clean shutdown, construction failure, or an unwind — runs the
/// same clear, so no path can leave a stopped role advertising itself.
struct ForgeReadinessGuard(super::ForgeRoleReadiness);

impl Drop for ForgeReadinessGuard {
    fn drop(&mut self) {
        self.0.publish(false);
    }
}

impl Forge {
    /// Runs leader election, hinted promotion and the leader's maintenance
    /// timer until cancellation.
    ///
    /// Every coordinator contends for the one leader term on a heartbeat and
    /// promotes the Scribe hot objects its own hints name, through a private
    /// attempt executor. The term holder sweeps `file_list` promotion debt on
    /// every heartbeat; under a test-owned trigger it sweeps only on the
    /// passes that trigger requests. Term renewal runs in its own loop and a
    /// separate timer runs the leader's Iceberg maintenance pass, so neither
    /// promotion nor maintenance can delay renewal. The term is resigned on
    /// stop, so a standby takes over at once.
    ///
    /// # Errors
    /// Returns [`ForgeError::AlreadyRunning`] for duplicate supervision or a
    /// construction error before the loop starts. Per-pass failures are logged
    /// and retried on the next pass.
    ///
    /// # Cancellation
    ///
    /// Cancellation interrupts idle waits, promotions and passes at their own
    /// durable boundaries; an interrupted attempt is recovered from its row.
    pub async fn run(
        self: &Arc<Self>,
        shutdown: CancellationToken,
        readiness: super::ForgeRoleReadiness,
    ) -> Result<(), ForgeError> {
        let _guard = self.acquire_run_guard()?;
        // Cleared whenever this loop stops for any reason, so routing closes
        // before the process finishes draining rather than after.
        let _readiness = ForgeReadinessGuard(readiness.clone());
        let executor = ForgeWorker::new(
            Arc::clone(self),
            ForgeWorkerConfig::default(),
            Uuid::now_v7(),
        )?;
        tokio::join!(
            self.renew(&shutdown, &readiness),
            self.supervise(&executor, &shutdown, &readiness),
            self.maintain(&executor, &shutdown),
        );
        if let Err(error) = self.leadership.resign().await {
            tracing::warn!(error = %error, "Forge leader term was not resigned; it will expire");
        }
        Ok(())
    }

    /// Renews or contends for the leader term on its own heartbeat until stop.
    ///
    /// This loop awaits nothing but the election row, so promotion, sweeps
    /// and maintenance can never delay renewal past the term. A renewal that
    /// fails or outlives the term revokes it inside
    /// [`super::leadership::ForgeLeadership::heartbeat`] and clears readiness
    /// until the next successful leader pass.
    async fn renew(&self, shutdown: &CancellationToken, readiness: &super::ForgeRoleReadiness) {
        let mut heartbeat = tokio::time::interval_at(
            tokio::time::Instant::now() + LEADER_HEARTBEAT,
            LEADER_HEARTBEAT,
        );
        heartbeat.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            tokio::select! {
                () = shutdown.cancelled() => break,
                _ = heartbeat.tick() => {
                    if let Err(error) = self.leadership.heartbeat(shutdown).await {
                        tracing::error!(error = %error, "Forge leader term renewal failed");
                        readiness.publish(false);
                    }
                }
            }
        }
    }

    /// Promotes hinted tables and runs leader passes until stop.
    async fn supervise(
        &self,
        executor: &ForgeWorker,
        shutdown: &CancellationToken,
        readiness: &super::ForgeRoleReadiness,
    ) {
        // Production elects at once on boot and sweeps promotion debt on
        // every heartbeat. A test that owns the trigger arranges its scenario
        // first and owns every sweep: its first heartbeat waits a period, and
        // its heartbeats only renew the term, so no unrequested sweep races
        // the world the scenario is building.
        #[cfg(feature = "test-support")]
        let quiet = self
            .core
            .scheduler_trigger
            .as_ref()
            .and_then(ForgeSchedulerTrigger::owner_for_test)
            .is_some();
        #[cfg(not(feature = "test-support"))]
        let quiet = false;
        let now = tokio::time::Instant::now();
        let mut heartbeat = tokio::time::interval_at(
            if quiet { now + LEADER_HEARTBEAT } else { now },
            LEADER_HEARTBEAT,
        );
        heartbeat.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        let mut hints_open = true;
        loop {
            tokio::select! {
                () = shutdown.cancelled() => break,
                hint = self.receive_hint(), if hints_open => {
                    match hint {
                        Some(hint) => {
                            let (binding, _) = hint.into_parts();
                            self.promote_hinted(executor, binding, shutdown).await;
                        }
                        None => hints_open = false,
                    }
                }
                _ = heartbeat.tick() => self.run_pass(executor, shutdown, readiness, !quiet).await,
                () = self.await_triggered_pass() => {
                    self.run_pass(executor, shutdown, readiness, true).await;
                    #[cfg(feature = "test-support")]
                    self.record_completed_pass();
                }
            }
        }
    }

    /// Runs the leader maintenance timer until stop.
    ///
    /// Like `RisingWave`'s GC loop, the first tick fires at once. A replica
    /// without the term skips the tick. A test that owns this loop drives
    /// every pass itself, so its timer waits one full interval.
    async fn maintain(&self, executor: &ForgeWorker, shutdown: &CancellationToken) {
        let interval = self.core.maintenance_interval;
        #[cfg(feature = "test-support")]
        let quiet = self
            .core
            .scheduler_trigger
            .as_ref()
            .and_then(ForgeSchedulerTrigger::owner_for_test)
            .is_some();
        #[cfg(not(feature = "test-support"))]
        let quiet = false;
        let now = tokio::time::Instant::now();
        let mut ticker =
            tokio::time::interval_at(if quiet { now + interval } else { now }, interval);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            tokio::select! {
                () = shutdown.cancelled() => break,
                _ = ticker.tick() => {}
                () = self.await_triggered_maintenance() => {}
            }
            // Boxed: the pass nests every attempt future, and inlining it in
            // this loop's state machine overflows the server's layout depth.
            tracing::Instrument::instrument(
                Box::pin(self.run_maintenance(executor)),
                tracing::info_span!("bifrost.forge.maintenance.pass", role = "server"),
            )
            .await;
            #[cfg(feature = "test-support")]
            self.record_completed_pass();
        }
    }

    /// Promotes what one hinted table owes on this coordinator.
    ///
    /// The commit notice reaches the leader from the executor after the
    /// Iceberg commit returns. A failure is logged; the leader's sweep of
    /// `file_list` debt retries it.
    async fn promote_hinted(
        &self,
        executor: &ForgeWorker,
        binding: crate::catalog::TenantTableBinding,
        stop: &CancellationToken,
    ) {
        let promoted = match vala_sql::row_types::forge_tasks::ForgeTaskTableIdentity::new(
            crate::catalog::BIFROST_CATALOG_NAME,
            binding.logical_namespace.clone(),
            binding.table_ref.name.clone(),
        ) {
            Ok(table) => {
                self.promote_table(executor, binding.tenant, &table, stop)
                    .await
            }
            Err(error) => Err(ForgeError::Sql(error)),
        };
        if let Err(error) = promoted {
            tracing::warn!(error = %error, table = %binding.table_ref.name, "hinted Forge promotion failed; the leader sweep retries it");
        }
    }

    /// Runs one heartbeat pass and publishes readiness.
    ///
    /// Every pass renews or contends for the leader term. A replica that
    /// finds another live leader is a healthy standby; when `sweep` is set,
    /// the term holder then sweeps promotion debt. Only a heartbeat under a
    /// test-owned trigger clears `sweep`.
    async fn run_pass(
        &self,
        executor: &ForgeWorker,
        stop: &CancellationToken,
        readiness: &super::ForgeRoleReadiness,
        sweep: bool,
    ) {
        let span = tracing::info_span!(
            "bifrost.forge.scheduler.pass",
            result = tracing::field::Empty,
            role = "server",
        );
        let result =
            tracing::Instrument::instrument(self.lead(executor, stop, sweep), span.clone()).await;
        match result {
            Ok(leader) => {
                span.record("result", if leader { "succeeded" } else { "standby" });
                readiness.publish(true);
            }
            Err(error) => {
                span.record("result", "failed");
                tracing::error!(error = %error, "Forge leader pass failed");
                // Cleared before the next pass, so a coordinator whose
                // dependency failed stops being routed to immediately.
                readiness.publish(false);
            }
        }
    }

    /// Holds the leader term for one heartbeat and, when `sweep` is set,
    /// sweeps promotion debt.
    ///
    /// Returns whether this replica holds the term after the pass. A pass
    /// without `sweep` only renews or contends for the term. The sweep runs
    /// under the term's revocation, so a term lost mid-sweep stops it before
    /// the next table and at each promotion's durable boundary.
    ///
    /// # Errors
    ///
    /// Returns election and promotion-debt read failures.
    async fn lead(
        &self,
        executor: &ForgeWorker,
        stop: &CancellationToken,
        sweep: bool,
    ) -> Result<bool, ForgeError> {
        self.leadership.heartbeat(stop).await?;
        let Some(term) = self.leadership.held() else {
            return Ok(false);
        };
        if !sweep {
            return Ok(true);
        }
        // ponytail: one indexed debt read per heartbeat; gate it on
        // acquisition plus a slower tick if the read ever shows up.
        self.sweep_promotion_debt(executor, term.revocation())
            .await?;
        Ok(self.leadership.held().is_some())
    }

    /// Promotes every table that still owes Scribe hot objects.
    ///
    /// This is the only durable read a new leader makes: Scribe's `file_list`
    /// is the hot-object authority, so a lost hint or a dead leader never
    /// strands a promotion. Ordinary compaction counts are not recovered.
    ///
    /// # Errors
    ///
    /// Returns the debt read's SQL error; per-table failures are logged and
    /// left for the next sweep.
    ///
    /// `stop` is the term's revocation; a sweep it interrupts leaves each
    /// attempt to recovery from its own row.
    ///
    /// Returns whether any table owed a promotion, whether this sweep ran it,
    /// left it to an attempt already active or queued, or failed it.
    async fn sweep_promotion_debt(
        &self,
        executor: &ForgeWorker,
        stop: &CancellationToken,
    ) -> Result<bool, ForgeError> {
        // ponytail: sequential sweep inside the supervisor loop; fan out over
        // the executor if promotion debt after failover ever delays heartbeats.
        let tables =
            vala_sql::queries::forge_tasks::ForgeTasks::new(self.core.operator_pool.clone())
                .tables_owing_promotion()
                .await
                .map_err(ForgeError::Sql)?;
        let mut owed = false;
        for (tenant, table) in tables {
            if stop.is_cancelled() {
                break;
            }
            match self.promote_table(executor, tenant, &table, stop).await {
                Ok(table_owed) => owed |= table_owed,
                Err(error) => {
                    owed = true;
                    tracing::warn!(error = %error, table = %table.table, "Forge promotion sweep failed for one table");
                }
            }
        }
        Ok(owed)
    }

    /// Waits for the deterministic test trigger that requests one maintenance pass.
    ///
    /// Production has no such trigger, so the future never resolves there and
    /// the timer loop is driven only by its ticks and shutdown.
    async fn await_triggered_maintenance(&self) {
        #[cfg(feature = "test-support")]
        if let Some(trigger) = &self.core.scheduler_trigger {
            trigger.wait_for_maintenance().await;
            return;
        }
        std::future::pending::<()>().await;
    }

    /// Waits for the deterministic test trigger that requests one leader pass.
    ///
    /// Production has no such trigger, so the future never resolves there and
    /// the supervisor `select!` is driven only by ticks, hints, and shutdown.
    async fn await_triggered_pass(&self) {
        #[cfg(feature = "test-support")]
        if let Some(trigger) = &self.core.scheduler_trigger {
            trigger.wait_for_request().await;
            return;
        }
        std::future::pending::<()>().await;
    }

    /// Reports one completed pass to the deterministic test trigger, if any.
    ///
    /// Compiled only with `test-support`; production passes carry no trigger.
    #[cfg(feature = "test-support")]
    fn record_completed_pass(&self) {
        if let Some(trigger) = &self.core.scheduler_trigger {
            trigger.record_completed_pass();
        }
    }

    /// Receives one lossy wake-up while holding only the inbox mutex.
    async fn receive_hint(&self) -> Option<StagingFileCommitted> {
        self.hints.lock().await.recv().await
    }

    /// Acquires the process-local singleton supervision guard.
    ///
    /// # Errors
    /// Returns [`ForgeError::AlreadyRunning`] when this owner is already active.
    fn acquire_run_guard(&self) -> Result<ForgeRunGuard<'_>, ForgeError> {
        self.running
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| ForgeError::AlreadyRunning)?;
        Ok(ForgeRunGuard {
            running: &self.running,
        })
    }
}

/// RAII guard that releases one process-local scheduler supervision slot.
struct ForgeRunGuard<'owner> {
    /// Atomic flag owned by the enclosing Forge handle.
    running: &'owner AtomicBool,
}

impl Drop for ForgeRunGuard<'_> {
    /// Releases the supervision slot on every loop exit path.
    fn drop(&mut self) {
        self.running.store(false, Ordering::Release);
    }
}

#[cfg(test)]
mod tests {
    /// The production supervisor source contains no direct execution roots.
    #[test]
    fn direct_execution_roots_are_absent() {
        let source = include_str!("scheduler.rs");
        for forbidden in [
            concat!("run_hinted_", "batch"),
            concat!("run_periodic_", "tick"),
            concat!("run_one_live_", "replacement"),
            concat!("run_targeted_", "compaction"),
        ] {
            assert!(
                !source.contains(forbidden),
                "legacy direct root remains: {forbidden}"
            );
        }
    }
}
