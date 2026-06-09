//! PSI baseline fit and target scoring.
//!
//! Fit body lands in Commit 5. Score body lands in Commit 6.

pub mod binning;
pub mod score;
pub mod threshold;

use std::collections::BTreeMap;

use wyrd_spec::card::drift::PsiProfile;
use wyrd_spec::ids::FeatureName;
use wyrd_version::WyrdVersion;

use crate::error::{DriftFitError, DriftScoreError};
use crate::report::DriftReport;

/// PSI fitted baseline, one entry per feature.
#[derive(Debug, Clone)]
pub struct PsiBaseline {
    pub features: BTreeMap<FeatureName, FittedPsiFeature>,
    pub wyrd_version: WyrdVersion,
}

/// Per-feature fitted PSI state.
#[derive(Debug, Clone)]
pub struct FittedPsiFeature {
    pub feature: FeatureName,
    pub bin_type: BinType,
    pub bins: Vec<Bin>,
    pub total_count: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinType {
    Numeric,
    Categorical,
}

/// One bin.
#[derive(Debug, Clone, PartialEq)]
pub struct Bin {
    pub id: i32,
    pub lower: Option<f64>,
    pub upper: Option<f64>,
    pub categorical_value: Option<String>,
    pub proportion: f64,
}

/// Body lands in Commit 5.
pub fn fit_psi_baseline(
    _batch: &arrow::record_batch::RecordBatch,
    _profile: &PsiProfile,
    _features: &[FeatureName],
) -> Result<PsiBaseline, DriftFitError> {
    unimplemented!("fit_psi_baseline body lands in Commit 5")
}

/// Body lands in Commit 6.
pub fn score_psi(
    _baseline: &PsiBaseline,
    _target: &arrow::record_batch::RecordBatch,
    _profile: &PsiProfile,
) -> Result<DriftReport, DriftScoreError> {
    unimplemented!("score_psi body lands in Commit 6")
}
