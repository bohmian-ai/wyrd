//! Drift-owned Bifrost tables: the raw observation input and the per-feature
//! scoring detail.
//!
//! `vala.drift.observations` holds the tall one-row-per-feature input a scoped
//! run emits; `vala.drift.result_features` holds one row per scored feature of
//! a completed Drift run, the detail beneath its `vala.verification.results`
//! row.

mod observations;
mod result_features;

pub use observations::ObservationsTable;
pub use result_features::ResultFeaturesTable;
