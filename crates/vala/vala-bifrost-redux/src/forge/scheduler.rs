//! Supervision for the single durable Forge planning scheduler.

use std::sync::atomic::{AtomicBool, Ordering};

use tokio_util::sync::CancellationToken;

use super::error::ForgeError;
use super::{Forge, ForgeScheduler, ForgeTickOutcome};
use crate::maintenance::StagingFileCommitted;

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
                _ = ticker.tick() => match scheduler.schedule_once(&shutdown).await {
                    Ok(outcome) => tracing::debug!(demands_seen = outcome.demands_seen, tasks_enqueued = outcome.tasks_enqueued, incomplete = outcome.incomplete, "Forge scheduling pass completed"),
                    Err(error) => tracing::error!(error = %error, "Forge scheduling pass failed"),
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
