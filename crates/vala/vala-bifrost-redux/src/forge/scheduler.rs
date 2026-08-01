//! Supervision for the single durable Forge planning scheduler.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Instant;

use tokio_util::sync::CancellationToken;

use super::error::ForgeError;
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
}

impl ForgeSchedulerTrigger {
    /// Construct an idle control for one supervised Forge owner.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
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
    pub async fn run(&self, shutdown: CancellationToken) -> Result<(), ForgeError> {
        let _guard = self.acquire_run_guard()?;
        let scheduler = ForgeScheduler::new(self)?;
        let interval = self.core.maintenance_interval;
        let mut ticker = tokio::time::interval_at(tokio::time::Instant::now() + interval, interval);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        let mut hints_open = true;
        loop {
            tokio::select! {
                () = shutdown.cancelled() => return Ok(()),
                hint = self.receive_hint(), if hints_open => match hint {
                    Some(hint) => if let Err(error) = scheduler.record_hint(hint).await {
                        tracing::error!(error = %error, "Forge planning hint persistence failed");
                    },
                    None => hints_open = false,
                },
                _ = ticker.tick() => {
                    let started = Instant::now();
                    let pass_span = tracing::info_span!(
                        "bifrost.forge.scheduler.pass",
                        result = tracing::field::Empty,
                        role = "server",
                    );
                    match tracing::Instrument::instrument(
                        scheduler.schedule_once(&shutdown),
                        pass_span.clone(),
                    ).await {
                    Ok(outcome) => {
                        pass_span.record("result", "succeeded");
                        self.core.telemetry.record_scheduler_pass(&outcome, started.elapsed());
                        tracing::debug!(demands_seen = outcome.demands_seen, tasks_enqueued = outcome.tasks_enqueued, incomplete = outcome.incomplete, "Forge scheduling pass completed");
                        if let Some(trigger) = &self.core.scheduler_trigger {
                            trigger.record_completed_pass();
                        }
                    }
                    Err(error) => {
                        pass_span.record("result", "failed");
                        tracing::error!(error = %error, "Forge scheduling pass failed");
                        if let Some(trigger) = &self.core.scheduler_trigger {
                            trigger.record_completed_pass();
                        }
                    }
                }
                },
                () = async {
                    if let Some(trigger) = &self.core.scheduler_trigger {
                        trigger.wait_for_request().await;
                    } else {
                        std::future::pending::<()>().await;
                    }
                } => {
                    let started = Instant::now();
                    let pass_span = tracing::info_span!(
                        "bifrost.forge.scheduler.pass",
                        result = tracing::field::Empty,
                        role = "server",
                    );
                    match tracing::Instrument::instrument(
                        scheduler.schedule_once(&shutdown),
                        pass_span.clone(),
                    ).await {
                    Ok(outcome) => {
                        pass_span.record("result", "succeeded");
                        self.core.telemetry.record_scheduler_pass(&outcome, started.elapsed());
                        tracing::debug!(demands_seen = outcome.demands_seen, tasks_enqueued = outcome.tasks_enqueued, incomplete = outcome.incomplete, "Forge triggered scheduling pass completed");
                        if let Some(trigger) = &self.core.scheduler_trigger {
                            trigger.record_completed_pass();
                        }
                    }
                    Err(error) => {
                        pass_span.record("result", "failed");
                        tracing::error!(error = %error, "Forge triggered scheduling pass failed");
                        if let Some(trigger) = &self.core.scheduler_trigger {
                            trigger.record_completed_pass();
                        }
                    }
                }
                },
            }
        }
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
