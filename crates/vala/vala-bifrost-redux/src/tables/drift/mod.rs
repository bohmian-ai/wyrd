//! Drift-owned Bifrost tables: the raw observation input and the per-feature
//! scoring detail.
//!
//! `bifrost.drift.observations` holds the tall one-row-per-feature samples a
//! scoped run records for scheduled windows; `bifrost.drift.result_features`
//! holds one row per scored feature of a completed drift check, the detail
//! beneath its `bifrost.verification.results` row.

mod observations;
mod result_features;

pub use observations::ObservationsTable;
pub use result_features::ResultFeaturesTable;
