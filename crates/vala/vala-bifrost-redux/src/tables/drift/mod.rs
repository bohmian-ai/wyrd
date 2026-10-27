//! Drift-owned Bifrost table: the raw observation input.
//!
//! `bifrost.drift.observations` holds the tall one-row-per-feature samples a
//! scoped run records for scheduled windows; a drift check's per-feature
//! scoring detail lives in its result's
//! `bifrost.verification.execution_outputs` row.

mod observations;

pub use observations::ObservationsTable;
