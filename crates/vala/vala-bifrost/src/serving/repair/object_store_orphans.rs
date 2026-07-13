//! Object-store orphan GC worker.
//!
//! Enumerates all registered Bifrost tables and deletes object-store files that
//! are not referenced by any live Iceberg snapshot. Files younger than 2 h are
//! skipped to avoid racing with in-flight commits. Cache entries are purged
//! before storage deletion (M13).
//!
//! This module currently reports `pending` health until slice 10 lands the
//! full GC loop with the live-set enumeration and purge-before-delete semantics.

use super::MaintenanceHealthState;

/// Health state for the object-store orphan GC concern.
///
/// Returns [`MaintenanceHealthState::Pending`] until slice 10 implements the
/// real GC loop.
#[must_use]
pub fn health() -> MaintenanceHealthState {
    MaintenanceHealthState::Pending
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::serving::repair::MaintenanceHealthState;

    #[test]
    fn object_store_orphans_reports_pending() {
        assert_eq!(health(), MaintenanceHealthState::Pending);
    }
}
