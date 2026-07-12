//! Repair and maintenance workers for the Bifrost OLAP warehouse.
//!
//! Each sub-module owns one maintenance concern. Workers are driven by the
//! [`scheduler`] which ticks every 60 s. Health is reported per-concern via
//! [`MaintenanceHealthState`]; `Pending` means the owning slice has not yet
//! landed — it is honest state, never hardcoded `Ok`.

pub mod commit_recovery;
pub mod compaction;
pub mod heartbeat;
pub mod index_build;
pub mod object_store_orphans;
pub mod projection_health;
pub mod scheduler;
pub mod snapshot_expiry;

/// Per-concern maintenance health state.
///
/// Concerns that have not yet shipped their owning slice report `Pending`.
/// The health endpoint maps this to the `"pending"` wire value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MaintenanceHealthState {
    /// Worker is not yet implemented (owning slice not landed). Honest state.
    Pending,
    /// Worker is running and all checks pass.
    Ok,
    /// Worker encountered an error; detail carried by the concern's health fn.
    Degraded,
}

/// Aggregate health across all five maintenance concerns.
#[derive(Debug, Clone)]
pub struct MaintenanceHealthReport {
    /// Commit recovery sweep (slice 01).
    pub commit_recovery: MaintenanceHealthState,
    /// Snapshot expiry worker (slice 09).
    pub snapshot_expiry: MaintenanceHealthState,
    /// Compaction worker (slice 11).
    pub compaction: MaintenanceHealthState,
    /// Index-build worker (slice 05).
    pub index_build: MaintenanceHealthState,
    /// Object-store orphan GC (slice 10).
    pub object_store_orphans: MaintenanceHealthState,
    /// Projection health monitor (slice 05).
    pub projection_health: MaintenanceHealthState,
}

/// Health state returned by `commit_recovery` until slice 01 completes wiring.
///
/// Slice 01 implements the recovery sweep; the health concern itself transitions
/// from `Pending` to `Ok`/`Degraded` in slice 13 (journey closeout).
pub fn commit_recovery_health() -> MaintenanceHealthState {
    // commit_recovery sweep is wired in this slice; health remains Pending
    // until slice 13 wires real state tracking.
    MaintenanceHealthState::Pending
}
