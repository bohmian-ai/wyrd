//! Supervision for Forge leadership, hinted promotion and leader passes.

use std::sync::Arc;
#[cfg(feature = "test-support")]
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::{AtomicBool, Ordering};

use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use super::error::ForgeError;
use super::leadership::LEADER_HEARTBEAT;
use super::{Forge, ForgeScheduler, ForgeWorker, ForgeWorkerConfig};
use crate::maintenance::StagingFileCommitted;

/// The kind of supervised pass the loop runs next.
#[derive(Debug, Clone, Copy)]
enum ForgePass {
    /// Renew or contend for the leader term only.
    Heartbeat,
    /// Hold the term, sweep promotion debt and plan durable demand.
    Planning,
}

/// Test-tier control and observation for a supervised production scheduler loop.
///
/// The trigger only wakes the already-running [`Forge::run`] loop. It never
/// constructs a scheduler, claims work, or executes a planning pass itself.
#[derive(Clone, Default)]
#[cfg(feature = "test-support")]
pub struct ForgeSchedulerTrigger {
    /// Wakeup consumed by the production supervisor select loop.
    requested: Arc<tokio::sync::Notify>,
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
    /// Runs leader election, hinted promotion and the leader's passes until
    /// cancellation.
    ///
    /// Every coordinator contends for the one leader term on a heartbeat and
    /// promotes the Scribe hot objects its own hints name, through a private
    /// attempt executor. Only the term holder sweeps `file_list` promotion
    /// debt — on acquisition and on every planning pass — and plans the table
    /// work still driven by durable demand. The term is resigned on stop, so a
    /// standby takes over at once.
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
        #[cfg(feature = "test-support")]
        let test_owner = self
            .core
            .scheduler_trigger
            .as_ref()
            .and_then(ForgeSchedulerTrigger::owner_for_test);
        #[cfg(feature = "test-support")]
        let mut scheduler = match test_owner {
            Some(owner) => ForgeScheduler::with_owner_for_test(self, owner)?,
            None => ForgeScheduler::new(self)?,
        };
        #[cfg(not(feature = "test-support"))]
        let mut scheduler = ForgeScheduler::new(self)?;
        let interval = self.core.maintenance_interval;
        // A test that owns this scheduler drives every pass itself, so its
        // loop stays quiet until asked rather than racing the arrangement.
        #[cfg(feature = "test-support")]
        let quiet = test_owner.is_some();
        #[cfg(not(feature = "test-support"))]
        let quiet = false;
        let now = tokio::time::Instant::now();
        let mut ticker =
            tokio::time::interval_at(if quiet { now + interval } else { now }, interval);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        let mut heartbeat = tokio::time::interval_at(now + LEADER_HEARTBEAT, LEADER_HEARTBEAT);
        heartbeat.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        let mut hints_open = true;
        loop {
            let pass = tokio::select! {
                () = shutdown.cancelled() => break,
                hint = self.receive_hint(), if hints_open => {
                    match hint {
                        Some(hint) => {
                            let (binding, _) = hint.into_parts();
                            self.promote_hinted(&executor, binding, &shutdown).await;
                        }
                        None => hints_open = false,
                    }
                    continue;
                }
                _ = heartbeat.tick() => ForgePass::Heartbeat,
                _ = ticker.tick() => ForgePass::Planning,
                () = self.await_triggered_pass() => ForgePass::Planning,
            };
            // Not raced against shutdown: an in-flight promotion observes the
            // token and drains, releasing its lease as a worker attempt does.
            self.run_pass(pass, &mut scheduler, &executor, &shutdown, &readiness)
                .await;
        }
        if let Err(error) = self.leadership.resign().await {
            tracing::warn!(error = %error, "Forge leader term was not resigned; it will expire");
        }
        Ok(())
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

    /// Runs one heartbeat or planning pass and publishes readiness.
    ///
    /// Every pass first renews or contends for the leader term. A replica that
    /// finds another live leader is a healthy standby. The term holder sweeps
    /// promotion debt on acquisition and on every planning pass, and runs the
    /// demand-driven table planning on planning passes.
    async fn run_pass(
        &self,
        pass: ForgePass,
        scheduler: &mut ForgeScheduler<'_>,
        executor: &ForgeWorker,
        stop: &CancellationToken,
        readiness: &super::ForgeRoleReadiness,
    ) {
        let span = tracing::info_span!(
            "bifrost.forge.scheduler.pass",
            result = tracing::field::Empty,
            role = "server",
        );
        let result = tracing::Instrument::instrument(
            self.lead(pass, scheduler, executor, stop),
            span.clone(),
        )
        .await;
        match result {
            Ok(leader) => {
                span.record("result", if leader { "leader" } else { "standby" });
                readiness.publish(true);
            }
            Err(error) => {
                span.record("result", "failed");
                tracing::error!(error = %error, ?pass, "Forge leader pass failed");
                // Cleared before the next pass, so a coordinator whose
                // dependency failed stops being routed to immediately.
                readiness.publish(false);
            }
        }
        #[cfg(feature = "test-support")]
        if matches!(pass, ForgePass::Planning) {
            self.record_completed_pass();
        }
    }

    /// Holds the leader term for one pass and runs the leader's work.
    ///
    /// Returns whether this replica leads after the pass.
    ///
    /// # Errors
    ///
    /// Returns election, promotion-debt read and demand-planning failures.
    async fn lead(
        &self,
        pass: ForgePass,
        scheduler: &mut ForgeScheduler<'_>,
        executor: &ForgeWorker,
        stop: &CancellationToken,
    ) -> Result<bool, ForgeError> {
        let acquired = self.leadership.heartbeat().await?;
        if self.leadership.held().is_none() {
            return Ok(false);
        }
        let planning = matches!(pass, ForgePass::Planning);
        let promoted = if acquired || planning {
            self.sweep_promotion_debt(executor, stop).await?
        } else {
            false
        };
        // ponytail: a pass that promoted plans nothing else, keeping the old
        // one-slot order where promotion outranks rewrite; drop it once
        // rewrite is driven by the leader's commit tracks.
        if planning && !promoted {
            let outcome = scheduler.schedule_once(stop).await?;
            tracing::debug!(
                demands_seen = outcome.demands_seen,
                tasks_enqueued = outcome.tasks_enqueued,
                incomplete = outcome.incomplete,
                "Forge demand planning pass completed"
            );
        }
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
    /// Returns whether any table was promoted.
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
        let mut promoted = false;
        for (tenant, table) in tables {
            if stop.is_cancelled() {
                break;
            }
            match self.promote_table(executor, tenant, &table, stop).await {
                Ok(done) => promoted |= done,
                Err(error) => {
                    tracing::warn!(error = %error, table = %table.table, "Forge promotion sweep failed for one table");
                }
            }
        }
        Ok(promoted)
    }

    /// Waits for the deterministic test trigger that requests one extra pass.
    ///
    /// Production has no such trigger, so the future never resolves there and
    /// the surrounding `select!` is driven only by ticks, hints, and shutdown.
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
