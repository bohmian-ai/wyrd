//! Eval-owned Bifrost tables.
//!
//! `vala.eval.result_items` holds one row per executed or skipped task of a
//! completed Eval run, the historical detail beneath its
//! `vala.verification.results` row. No current Verifier writes it.

mod result_items;

pub use result_items::ResultItemsTable;
