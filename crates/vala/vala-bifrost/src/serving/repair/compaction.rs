//! Compaction maintenance worker.
//!
//! Bin-packs small Iceberg files into target-sized files using a copy-on-write REPLACE
//! strategy backed by the iceberg-rust REPLACE fork. Tracks intent/effect in
//! `vala.compaction_ledger` and reconciles on boot for crash safety.
//!
//! This module currently reports `pending` health until slice 11 lands the
//! full compaction loop with the REPLACE commit and ledger FSM.

use super::MaintenanceHealthState;

/// Health state for the compaction concern.
///
/// Returns [`MaintenanceHealthState::Pending`] until slice 11 implements the
/// real CoW-REPLACE compaction loop.
#[must_use]
pub fn health() -> MaintenanceHealthState {
    MaintenanceHealthState::Pending
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::serving::repair::MaintenanceHealthState;

    #[test]
    fn compaction_reports_pending() {
        assert_eq!(health(), MaintenanceHealthState::Pending);
    }
}
