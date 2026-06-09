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
    batch: &arrow::record_batch::RecordBatch,
    profile: &SpcProfile,
    features: &[FeatureName],
) -> Result<SpcBaseline, DriftFitError> {
    use crate::feature::resolve_column;
    use crate::spc::control_limits::{adaptive_sample_size, fit_control_limits};

    let row_count = batch.num_rows();
    let chunk_size = if profile.sample_size == 0 {
        adaptive_sample_size(row_count)
    } else {
        profile.sample_size
    };

    let mut fitted = BTreeMap::new();
    for feature in features {
        let column = resolve_column(batch, feature)?;
        if !column.is_numeric() {
            return Err(DriftFitError::FeatureNotNumeric {
                feature: feature.as_str().to_string(),
                arrow_type: column.data_type_string(),
            });
        }

        let values =
            column
                .collect_f64_non_null()
                .map_err(|()| DriftFitError::FeatureNotNumeric {
                    feature: feature.as_str().to_string(),
                    arrow_type: column.data_type_string(),
                })?;
        if values.is_empty() {
            return Err(DriftFitError::FeatureEmpty {
                feature: feature.as_str().to_string(),
            });
        }
        if values.iter().any(|value| !value.is_finite()) {
            return Err(DriftFitError::NonFiniteValuesInColumn {
                feature: feature.as_str().to_string(),
            });
        }

        let limits = fit_control_limits(&values, chunk_size).map_err(|error| match error {
            DriftFitError::InsufficientSamplesForChunk {
                rows, chunk_size, ..
            } => DriftFitError::InsufficientSamplesForChunk {
                feature: feature.as_str().to_string(),
                rows,
                chunk_size,
            },
            other => other,
        })?;
        fitted.insert(
            feature.clone(),
            FittedSpcFeature {
                center: limits.center,
                one_lcl: limits.one_lcl,
                one_ucl: limits.one_ucl,
                two_lcl: limits.two_lcl,
                two_ucl: limits.two_ucl,
                three_lcl: limits.three_lcl,
                three_ucl: limits.three_ucl,
            },
        );
    }

    Ok(SpcBaseline {
        features: fitted,
        chunk_size,
        wyrd_version: WyrdVersion::current(),
    })
}

pub fn score_spc(
    _baseline: &SpcBaseline,
    _target: &arrow::record_batch::RecordBatch,
    _profile: &SpcProfile,
) -> Result<DriftReport, DriftScoreError> {
    unimplemented!("score_spc body lands in Commit 8")
}
