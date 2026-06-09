//! Top-level baseline dispatch.
//!
//! The `FittedBaseline` enum and the `fit_baseline` / `score` dispatch
//! functions land in Commit 10 (`11-report-and-tests.md`). This module is
//! introduced here so `lib.rs` can `pub use` from it without missing
//! identifiers during Commits 4-9.

use crate::psi::PsiBaseline;
use crate::spc::SpcBaseline;

/// Fitted baseline state produced by `fit_baseline`.
#[derive(Debug, Clone)]
pub enum FittedBaseline {
    /// PSI fitted baseline state.
    Psi(PsiBaseline),
    /// SPC fitted baseline state.
    Spc(SpcBaseline),
    /// Custom carries no state because there is no fit phase.
    Custom,
}

// `fit_baseline` and `score` are added in Commit 10.
pub fn fit_baseline(
    _batch: &arrow::record_batch::RecordBatch,
    _spec: &wyrd_spec::card::drift::DriftSpec,
) -> Result<FittedBaseline, crate::DriftFitError> {
    unimplemented!("fit_baseline lands in Commit 10")
}

pub fn score(
    _baseline: &FittedBaseline,
    _target: &arrow::record_batch::RecordBatch,
    _spec: &wyrd_spec::card::drift::DriftSpec,
) -> Result<crate::report::DriftReport, crate::DriftScoreError> {
    unimplemented!("score lands in Commit 10")
}
