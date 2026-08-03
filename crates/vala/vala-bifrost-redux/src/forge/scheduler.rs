//! Supervision for the single durable Forge planning scheduler.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Instant;

use tokio_util::sync::CancellationToken;
#[cfg(feature = "test-support")]
use uuid::Uuid;

use super::error::ForgeError;
use super::metrics::ForgeLeaseResult;
use super::{Forge, ForgeScheduler, ForgeTickOutcome};
use crate::maintenance::StagingFileCommitted;

/// Test-tier control and observation for a supervised production scheduler loop.
///
/// The trigger only wakes the already-running [`Forge::run`] loop. It never
/// constructs a scheduler, claims work, or executes a planning pass itself.
#[derive(Clone, Default)]
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
    fn owner_for_test(&self) -> Option<Uuid> {
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

impl Forge {
    /// Runs durable hint ingestion and periodic planning until cancellation.
    ///
    /// This supervisor has no rewrite, `DataFusion`, object mutation, or Iceberg
    /// commit path. Execution belongs to the worker owner introduced separately.
    ///
    /// # Errors
    /// Returns [`ForgeError::AlreadyRunning`] for duplicate supervision or a
    /// construction error before the loop starts. Per-pass failures are logged
    /// and retained as durable demand for later retry.
    ///
    /// # Cancellation
    ///
    /// Cancellation interrupts idle waits, durable hint persistence, and
    /// scheduling passes. A cancelled hint write remains absent or committed
    /// according to the database transaction boundary and is safe to recreate
    /// from the durable staging-file roster on a later pass.
    pub async fn run(&self, shutdown: CancellationToken) -> Result<(), ForgeError> {
        let _guard = self.acquire_run_guard()?;
        #[cfg(feature = "test-support")]
        let scheduler = match self
            .core
            .scheduler_trigger
            .as_ref()
            .and_then(ForgeSchedulerTrigger::owner_for_test)
        {
            Some(owner) => ForgeScheduler::with_owner_for_test(self, owner)?,
            None => ForgeScheduler::new(self)?,
        };
        #[cfg(not(feature = "test-support"))]
        let scheduler = ForgeScheduler::new(self)?;
        let interval = self.core.maintenance_interval;
        let mut ticker = tokio::time::interval_at(tokio::time::Instant::now() + interval, interval);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        let mut hints_open = true;
        loop {
            tokio::select! {
                () = shutdown.cancelled() => return Ok(()),
                hint = self.receive_hint(), if hints_open => match hint {
                    Some(hint) => {
                        let result = tokio::select! {
                            () = shutdown.cancelled() => return Ok(()),
                            result = scheduler.record_hint(hint) => result,
                        };
                        if let Err(error) = result {
                            tracing::error!(error = %error, "Forge planning hint persistence failed");
                        }
                    },
                    None => hints_open = false,
                },
                _ = ticker.tick() => {
                    if !self.run_planning_pass(&scheduler, &shutdown, false).await {
                        return Ok(());
                    }
                },
                () = async {
                    if let Some(trigger) = &self.core.scheduler_trigger {
                        trigger.wait_for_request().await;
                    } else {
                        std::future::pending::<()>().await;
                    }
                } => {
                    if !self.run_planning_pass(&scheduler, &shutdown, true).await {
                        return Ok(());
                    }
                },
            }
        }
    }

    /// Runs and records one periodic or explicitly triggered planning pass.
    ///
    /// Per-pass scheduler failures remain supervised: they are recorded and
    /// logged, then the outer loop continues. Cancellation interrupts the pass
    /// without publishing a result or recording trigger completion.
    ///
    /// Returns `false` when shutdown interrupts the pass and the supervisor
    /// must exit, or `true` after recording a completed pass attempt.
    async fn run_planning_pass(
        &self,
        scheduler: &ForgeScheduler<'_>,
        shutdown: &CancellationToken,
        triggered: bool,
    ) -> bool {
        let started = Instant::now();
        let pass_span = tracing::info_span!(
            "bifrost.forge.scheduler.pass",
            result = tracing::field::Empty,
            role = "server",
        );
        let pass =
            tracing::Instrument::instrument(scheduler.schedule_once(shutdown), pass_span.clone());
        let result = tokio::select! {
            () = shutdown.cancelled() => return false,
            result = pass => result,
        };
        match result {
            Ok(outcome) => {
                pass_span.record(
                    "result",
                    if outcome.standby {
                        "standby"
                    } else {
                        "succeeded"
                    },
                );
                self.core
                    .telemetry
                    .record_scheduler_pass(&outcome, started.elapsed());
                if outcome.standby {
                    self.core
                        .telemetry
                        .record_lease(ForgeLeaseResult::Contention);
                    tracing::debug!(triggered, "Forge scheduler remains on standby");
                    if let Some(trigger) = &self.core.scheduler_trigger {
                        trigger.record_completed_pass();
                    }
                    return true;
                }
                tracing::debug!(
                    demands_seen = outcome.demands_seen,
                    tasks_enqueued = outcome.tasks_enqueued,
                    incomplete = outcome.incomplete,
                    triggered,
                    "Forge scheduling pass completed"
                );
            }
            Err(error) => {
                pass_span.record("result", "failed");
                tracing::error!(error = %error, triggered, "Forge scheduling pass failed");
            }
        }
        if let Some(trigger) = &self.core.scheduler_trigger {
            trigger.record_completed_pass();
        }
        true
    }

    /// Runs one durable planning pass without executing claimed work.
    ///
    /// # Errors
    /// Returns scheduler construction, discovery, catalog-read, or SQL errors.
    pub async fn run_once(&self) -> Result<ForgeTickOutcome, ForgeError> {
        let scheduled = ForgeScheduler::new(self)?
            .schedule_once(&CancellationToken::new())
            .await?;
        Ok(ForgeTickOutcome {
            groups_seen: scheduled.tasks_enqueued,
            tables_discovered: scheduled.demands_seen,
            tables_examined: scheduled.demands_seen,
            tables_succeeded: scheduled.demands_acknowledged,
            tables_failed: usize::from(scheduled.incomplete),
            pending_work: scheduled.incomplete,
            ..ForgeTickOutcome::default()
        })
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
