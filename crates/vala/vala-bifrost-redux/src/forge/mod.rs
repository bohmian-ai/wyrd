//! Forge owns background maintenance for Bifrost's staged and Iceberg data.
//!
//! A scheduler leases one tenant/table at a time, then runs reconciliation,
//! compaction, snapshot expiry, and orphan garbage collection under the same
//! fencing boundary. The stages use the audit outbox and durable file metadata
//! to recover work after a process or catalog failure.

pub(crate) mod binpack;
pub(crate) mod compact;
pub(crate) mod error;
pub(crate) mod expire;
pub(crate) mod lease;
pub(crate) mod orphan_gc;
mod scheduler;

pub use compact::{ForgeConfig, ForgeContext, ForgeObjectStore, ForgeTickOutcome};
pub use error::ForgeError;
pub use scheduler::{ForgeScheduler, run_maintenance_tick};
