//! SPC baseline fit and target scoring.
//!
//! Fit body lands in Commit 7. Score body lands in Commit 8.

pub mod control_limits;
pub mod weco;

use std::collections::BTreeMap;

use wyrd_spec::card::drift::SpcProfile;
use wyrd_spec::ids::FeatureName;
use wyrd_version::WyrdVersion;

use crate::error::{DriftFitError, DriftScoreError};
use crate::report::DriftReport;

/// SPC fitted baseline, one entry per feature plus the chunk size used.
#[derive(Debug, Clone)]
pub struct SpcBaseline {
    pub features: BTreeMap<FeatureName, FittedSpcFeature>,
    pub chunk_size: u32,
    pub wyrd_version: WyrdVersion,
}

/// Per-feature fitted SPC state.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FittedSpcFeature {
    pub center: f64,
    pub one_lcl: f64,
    pub one_ucl: f64,
    pub two_lcl: f64,
    pub two_ucl: f64,
    pub three_lcl: f64,
    pub three_ucl: f64,
}

pub fn fit_spc_baseline(
    _batch: &arrow::record_batch::RecordBatch,
    _profile: &SpcProfile,
    _features: &[FeatureName],
) -> Result<SpcBaseline, DriftFitError> {
    unimplemented!("fit_spc_baseline body lands in Commit 7")
}

pub fn score_spc(
    _baseline: &SpcBaseline,
    _target: &arrow::record_batch::RecordBatch,
    _profile: &SpcProfile,
) -> Result<DriftReport, DriftScoreError> {
    unimplemented!("score_spc body lands in Commit 8")
}
