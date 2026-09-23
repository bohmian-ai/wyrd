//! Eval-owned Bifrost tables: the raw observation input and the per-task
//! outcome detail.
//!
//! `vala.eval.observations` holds one row per Eval input record a scoped run
//! emits; `vala.eval.result_items` holds one row per executed or skipped task
//! of a completed Eval run, the detail beneath its `vala.verification.results`
//! row.

mod observations;
mod result_items;

pub use observations::ObservationsTable;
pub use result_items::ResultItemsTable;
