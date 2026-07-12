//! Index-build maintenance worker.
//!
//! Builds bloom filters and lookup sets for declared indexes in `vala.olap_indexes`.
//! Dispatches to the correct runtime based on index kind and flips each index
//! from `building` to `ready` when complete.
//!
//! This module currently reports `pending` health until slice 05 lands the
//! real index-build loop.

use super::MaintenanceHealthState;

/// Health state for the index-build concern.
///
/// Returns [`MaintenanceHealthState::Pending`] until slice 05 implements the
/// real index-build worker that materialises bloom filters and lookup sets.
#[must_use]
pub fn health() -> MaintenanceHealthState {
    MaintenanceHealthState::Pending
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::serving::repair::MaintenanceHealthState;

    #[test]
    fn index_build_reports_pending() {
        assert_eq!(health(), MaintenanceHealthState::Pending);
    }
}
