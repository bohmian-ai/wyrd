//! Projection health monitor.
//!
//! Scans `vala.olap_projections` for stale or failed projections and degrades
//! or rebuilds them. Tracks per-projection health derived from the last refresh
//! state and the source snapshot currency.
//!
//! This module currently reports `pending` health until slice 05 lands the
//! real projection refresh loop with `refresh_worker` and the `LookupSet` /
//! `Rollup` / `MV` runtimes.

use super::MaintenanceHealthState;

/// Health state for the projection health concern.
///
/// Returns [`MaintenanceHealthState::Pending`] until slice 05 implements the
/// real projection refresh cycle with scan/degrade/rebuild semantics.
#[must_use]
pub fn health() -> MaintenanceHealthState {
    MaintenanceHealthState::Pending
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::serving::repair::MaintenanceHealthState;

    #[test]
    fn projection_health_reports_pending() {
        assert_eq!(health(), MaintenanceHealthState::Pending);
    }
}
