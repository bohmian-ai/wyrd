//! Snapshot-expiry maintenance worker.
//!
//! Enumerates all registered Bifrost tables and expires Iceberg snapshots that
//! are older than 2 h and beyond the retention pin. The pin is the minimum
//! `built_for_snapshot_id` across read-eligible `LookupSet` projections plus the
//! derivation pin — resolved by Iceberg sequence/timestamp/ancestry order,
//! **never** by numeric snapshot-id comparison (RW-F02).
//!
//! This module currently reports `pending` health until slice 09 lands the
//! full expiry logic.

use super::MaintenanceHealthState;

/// Health state for the snapshot-expiry concern.
///
/// Returns [`MaintenanceHealthState::Pending`] until slice 09 implements the
/// real expiry loop with Iceberg ancestry resolution.
#[must_use]
pub fn health() -> MaintenanceHealthState {
    MaintenanceHealthState::Pending
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::serving::repair::MaintenanceHealthState;

    #[test]
    fn snapshot_expiry_reports_pending() {
        assert_eq!(health(), MaintenanceHealthState::Pending);
    }
}
