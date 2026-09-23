//! Verifier-shared Bifrost tables.
//!
//! `vala.verification.results` is the one verdict surface every Verifier
//! implementation writes, whatever its per-implementation detail table.

mod results;

pub use results::ResultsTable;
