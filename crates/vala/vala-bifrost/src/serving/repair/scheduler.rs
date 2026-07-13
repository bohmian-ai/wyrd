//! Maintenance worker scheduler.
//!
//! The scheduler drives the repair loop: on each 60-second tick it polls each
//! maintenance concern for its current [`MaintenanceHealthState`] and runs the
//! recovery sweep when a recovery pool is available.
//!
//! Per-slice owning concern:
//! - `commit_recovery` — slice 01 (this slice)
//! - `snapshot_expiry` — slice 09
//! - `compaction` — slice 11
//! - `index_build` — slice 05
//! - `object_store_orphans` — slice 10
//! - `projection_health` — slice 05
//!
//! Each concern that has not yet landed reports `Pending`. The scheduler does
//! not treat `Pending` as an error; it logs at `debug` and moves on.

use std::sync::Arc;

use vala_sql::postgres::ValaPostgres;

use crate::catalog::WyrdCatalog;

use super::{
    MaintenanceHealthReport, commit_recovery, compaction, index_build, object_store_orphans,
    projection_health, snapshot_expiry,
};

/// Tick interval for the maintenance scheduler (60 s).
pub const TICK_INTERVAL_SECS: u64 = 60;

/// Collect the current health report across all five maintenance concerns.
/// Called by the scheduler on each tick and by the health endpoint.
#[must_use]
pub fn collect_health() -> MaintenanceHealthReport {
    MaintenanceHealthReport {
        commit_recovery: commit_recovery::health(),
        snapshot_expiry: snapshot_expiry::health(),
        compaction: compaction::health(),
        index_build: index_build::health(),
        object_store_orphans: object_store_orphans::health(),
        projection_health: projection_health::health(),
    }
}

/// Run one maintenance tick: collect health, run the recovery sweep when
/// the recovery pool is available, and log the outcome.
pub async fn tick(vala: &ValaPostgres, catalog: &Arc<WyrdCatalog>) {
    let health = collect_health();
    tracing::debug!(
        commit_recovery = ?health.commit_recovery,
        snapshot_expiry = ?health.snapshot_expiry,
        compaction = ?health.compaction,
        index_build = ?health.index_build,
        object_store_orphans = ?health.object_store_orphans,
        projection_health = ?health.projection_health,
        "maintenance health tick"
    );

    // Run the recovery sweep (slice 01). Other workers are deferred to their
    // owning slices and will be wired in here.
    match commit_recovery::tick(vala, catalog, 50).await {
        Ok(outcome) => {
            if outcome.claimed > 0 || outcome.stuck > 0 {
                tracing::info!(
                    claimed = outcome.claimed,
                    resolved_committed = outcome.resolved_committed,
                    resolved_aborted = outcome.resolved_aborted,
                    scan_failed = outcome.scan_failed,
                    stuck = outcome.stuck,
                    "commit_recovery: sweep tick complete"
                );
            }
        }
        Err(e) => {
            tracing::error!(error = %e, "commit_recovery: tick failed");
        }
    }
}

#[cfg(test)]
mod wiring {
    use super::*;
    use crate::serving::repair::MaintenanceHealthState;

    /// Verify scheduler wiring: `collect_health()` returns a full report with all
    /// five concerns populated. Before any owning slice lands, each concern
    /// reports Pending — never hardcoded Ok or a missing field.
    #[test]
    fn wiring_all_concerns_present_and_pending() {
        let report = collect_health();

        // All five concerns must be present and report Pending before their
        // owning slices land.
        assert_eq!(
            report.commit_recovery,
            MaintenanceHealthState::Pending,
            "commit_recovery must report Pending"
        );
        assert_eq!(
            report.snapshot_expiry,
            MaintenanceHealthState::Pending,
            "snapshot_expiry must report Pending"
        );
        assert_eq!(
            report.compaction,
            MaintenanceHealthState::Pending,
            "compaction must report Pending"
        );
        assert_eq!(
            report.index_build,
            MaintenanceHealthState::Pending,
            "index_build must report Pending"
        );
        assert_eq!(
            report.object_store_orphans,
            MaintenanceHealthState::Pending,
            "object_store_orphans must report Pending"
        );
        assert_eq!(
            report.projection_health,
            MaintenanceHealthState::Pending,
            "projection_health must report Pending"
        );
    }

    /// No concern may report Ok before its owning slice lands. The drift floor
    /// (§6) forbids hardcoded-green health.
    #[test]
    fn no_concern_hardcoded_ok() {
        let report = collect_health();
        let all_states = [
            ("commit_recovery", report.commit_recovery),
            ("snapshot_expiry", report.snapshot_expiry),
            ("compaction", report.compaction),
            ("index_build", report.index_build),
            ("object_store_orphans", report.object_store_orphans),
            ("projection_health", report.projection_health),
        ];
        for (name, state) in all_states {
            assert_ne!(
                state,
                MaintenanceHealthState::Ok,
                "concern {name} must not report Ok before its owning slice lands"
            );
        }
    }
}
