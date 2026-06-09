//! SPC baseline fit and target scoring.

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
    baseline: &SpcBaseline,
    target: &arrow::record_batch::RecordBatch,
    profile: &SpcProfile,
) -> Result<DriftReport, DriftScoreError> {
    use crate::feature::resolve_column;
    use crate::report::{DriftVerdict, FeatureDriftReport};
    use crate::spc::control_limits::ControlLimits;
    use crate::spc::weco::{assign_zone, evaluate, parse_rule};
    use wyrd_spec::card::drift::DriftMethod;

    let rule = parse_rule(&profile.weco_rule.rule_string)?;
    let chunk_size = baseline.chunk_size as usize;
    let mut feature_reports = BTreeMap::new();

    for (feature_name, fitted) in &baseline.features {
        let column = resolve_column(target, feature_name).map_err(|_| {
            DriftScoreError::FeatureMissingInTarget {
                feature: feature_name.as_str().to_string(),
            }
        })?;
        if !column.is_numeric() {
            return Err(DriftScoreError::FeatureTypeMismatch {
                feature: feature_name.as_str().to_string(),
            });
        }
        let values =
            column
                .collect_f64_non_null()
                .map_err(|_| DriftScoreError::FeatureTypeMismatch {
                    feature: feature_name.as_str().to_string(),
                })?;
        if values.is_empty() {
            return Err(DriftScoreError::FeatureEmpty {
                feature: feature_name.as_str().to_string(),
            });
        }
        if values.iter().any(|value| !value.is_finite()) {
            return Err(DriftScoreError::SpcInternal {
                message: "non-finite value in target column".into(),
            });
        }
        if values.len() < chunk_size {
            return Err(DriftScoreError::TargetTooSmall {
                feature: feature_name.as_str().to_string(),
                rows: values.len(),
                chunk_size,
            });
        }

        let chunk_means = sample_chunk_means(&values, chunk_size)?;
        let limits = ControlLimits {
            center: fitted.center,
            one_lcl: fitted.one_lcl,
            one_ucl: fitted.one_ucl,
            two_lcl: fitted.two_lcl,
            two_ucl: fitted.two_ucl,
            three_lcl: fitted.three_lcl,
            three_ucl: fitted.three_ucl,
        };
        let drift_array: Vec<i8> = chunk_means
            .iter()
            .map(|value| assign_zone(*value, &limits))
            .collect();
        let violations = evaluate(&drift_array, &rule, profile.alert_threshold);
        let score = violations.len() as f64;
        let verdict = if score > 0.0 {
            DriftVerdict::Drift
        } else {
            DriftVerdict::NoDrift
        };
        feature_reports.insert(
            feature_name.clone(),
            FeatureDriftReport {
                feature: feature_name.clone(),
                score,
                threshold: 0.0,
                verdict,
            },
        );
    }

    let verdict = DriftReport::aggregate_verdict(&feature_reports);
    Ok(DriftReport {
        method: DriftMethod::Spc,
        features: feature_reports,
        verdict,
    })
}

fn sample_chunk_means(values: &[f64], chunk_size: usize) -> Result<Vec<f64>, DriftScoreError> {
    if chunk_size == 0 {
        return Err(DriftScoreError::SpcInternal {
            message: "chunk_size must be greater than zero".into(),
        });
    }

    Ok(values
        .chunks(chunk_size)
        .map(|chunk| chunk.iter().sum::<f64>() / chunk.len() as f64)
        .collect())
}
