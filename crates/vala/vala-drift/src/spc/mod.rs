//! SPC baseline fit and target scoring.

pub(crate) mod control_limits;
pub(crate) mod weco;

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use wyrd_spec::card::drift::SpcProfile;
use wyrd_spec::ids::FeatureName;
use wyrd_version::WyrdVersion;

use crate::error::{DriftFitError, DriftScoreError};
use crate::report::DriftReport;

/// SPC fitted baseline, one entry per feature plus the chunk size used.
///
/// Serializable so the server's baseline store can persist it as the fitted
/// profile of one Verifier version.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpcBaseline {
    pub features: BTreeMap<FeatureName, FittedSpcFeature>,
    pub chunk_size: u32,
    /// Vala version at fit time; used by the persistence layer for forward-compatibility checks.
    pub wyrd_version: WyrdVersion,
}

/// Per-feature fitted SPC state.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
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
        let column = resolve_column(batch, feature).map_err(|_| DriftFitError::FeatureMissing {
            feature: feature.as_str().to_string(),
        })?;
        if !column.is_numeric() {
            return Err(DriftFitError::FeatureNotNumeric {
                feature: feature.as_str().to_string(),
                arrow_type: column.data_type_string(),
            });
        }

        let values =
            column
                .collect_f64_non_null()
                .map_err(|_| DriftFitError::FeatureNotNumeric {
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

/// Score a target `RecordBatch` against a fitted SPC baseline.
///
/// Forms consecutive chunk means of each feature's non-null values in row
/// order (the trailing partial chunk included) and scores them through
/// [`score_spc_chunks`], so raw-batch and server-aggregated inputs share one
/// zone and rule evaluation.
///
/// # Errors
/// Returns [`DriftScoreError`] when a feature is missing, mistyped, empty,
/// non-finite, or shorter than the chunk size, or when the WECO rule is
/// malformed.
pub fn score_spc(
    baseline: &SpcBaseline,
    target: &arrow::record_batch::RecordBatch,
    profile: &SpcProfile,
) -> Result<DriftReport, DriftScoreError> {
    use crate::feature::resolve_column;

    let chunk_size = baseline.chunk_size as usize;
    let mut chunks = BTreeMap::new();
    for feature_name in baseline.features.keys() {
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
        chunks.insert(
            feature_name.clone(),
            SpcTargetChunks {
                rows: values.len() as u64,
                means: sample_chunk_means(&values, chunk_size)?,
            },
        );
    }
    score_spc_chunks(baseline, &chunks, profile)
}

/// Ordered chunk means of one SPC feature's target window.
///
/// `means[i]` is the mean of the `i`-th consecutive run of `chunk_size`
/// values in target order; the last mean may cover a shorter trailing chunk.
#[derive(Debug, Clone, PartialEq)]
pub struct SpcTargetChunks {
    /// Non-null target values across every chunk.
    pub rows: u64,
    /// Chunk means in chunk order.
    pub means: Vec<f64>,
}

/// Score per-feature ordered chunk means against a fitted SPC baseline.
///
/// This is the aggregate-input entry point: the server forms the chunk means
/// and passes only them. A feature with fewer values than the frozen chunk
/// size (including zero) is `Inconclusive` with NaN score. Otherwise each
/// mean is assigned its control-limit zone, the profile's WECO rule and alert
/// threshold are evaluated, and the violation count is the score: any
/// violation is `Drift`. The threshold is NaN because the verdict is
/// rule-driven.
///
/// # Errors
/// Returns [`DriftScoreError::WecoMalformed`] for an unparseable rule, and
/// [`DriftScoreError::SpcInternal`] when a baseline feature has no chunks or
/// a chunk mean is not finite.
pub fn score_spc_chunks(
    baseline: &SpcBaseline,
    chunks: &BTreeMap<FeatureName, SpcTargetChunks>,
    profile: &SpcProfile,
) -> Result<DriftReport, DriftScoreError> {
    use crate::report::{DriftVerdict, FeatureDriftReport};
    use crate::spc::control_limits::ControlLimits;
    use crate::spc::weco::{assign_zone, evaluate, parse_rule};
    use wyrd_spec::card::drift::DriftMethod;

    let rule = parse_rule(&profile.weco_rule.rule_string)?;
    let mut feature_reports = BTreeMap::new();
    for (feature_name, fitted) in &baseline.features {
        let target = chunks
            .get(feature_name)
            .ok_or_else(|| DriftScoreError::SpcInternal {
                message: format!("feature {} has no target chunks", feature_name.as_str()),
            })?;
        if target.means.iter().any(|mean| !mean.is_finite()) {
            return Err(DriftScoreError::SpcInternal {
                message: "non-finite chunk mean in target".into(),
            });
        }
        let report = if target.rows < u64::from(baseline.chunk_size) {
            FeatureDriftReport {
                feature: feature_name.clone(),
                score: f64::NAN,
                threshold: f64::NAN,
                verdict: DriftVerdict::Inconclusive,
            }
        } else {
            let limits = ControlLimits {
                center: fitted.center,
                one_lcl: fitted.one_lcl,
                one_ucl: fitted.one_ucl,
                two_lcl: fitted.two_lcl,
                two_ucl: fitted.two_ucl,
                three_lcl: fitted.three_lcl,
                three_ucl: fitted.three_ucl,
            };
            let drift_array: Vec<i8> = target
                .means
                .iter()
                .map(|value| assign_zone(*value, &limits))
                .collect();
            let violations = evaluate(&drift_array, &rule, profile.alert_threshold);
            let score = violations.len() as f64;
            FeatureDriftReport {
                feature: feature_name.clone(),
                score,
                threshold: f64::NAN,
                verdict: if score > 0.0 {
                    DriftVerdict::Drift
                } else {
                    DriftVerdict::NoDrift
                },
            }
        };
        feature_reports.insert(feature_name.clone(), report);
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

#[cfg(test)]
mod spc_fit {
    //! Unit tests for `fit_spc_baseline`.

    use std::sync::Arc;

    use crate::{DriftFitError, fit_spc_baseline};
    use arrow::array::{Float64Array, Int64Array, StringArray};
    use arrow::record_batch::RecordBatch;
    use arrow_schema::{DataType, Field, Schema};
    use wyrd_spec::card::drift::{SpcAlertThreshold, SpcProfile, SpcWecoRule};
    use wyrd_spec::ids::FeatureName;

    fn feature(name: &str) -> FeatureName {
        FeatureName::new(name).expect("valid feature name")
    }

    fn numeric_batch(name: &str, values: Vec<f64>) -> RecordBatch {
        RecordBatch::try_new(
            Arc::new(Schema::new(vec![Field::new(name, DataType::Float64, true)])),
            vec![Arc::new(Float64Array::from(values))],
        )
        .expect("record batch")
    }

    fn int_batch(name: &str, values: Vec<i64>) -> RecordBatch {
        RecordBatch::try_new(
            Arc::new(Schema::new(vec![Field::new(name, DataType::Int64, true)])),
            vec![Arc::new(Int64Array::from(values))],
        )
        .expect("record batch")
    }

    fn spc_profile(sample_size: u32) -> SpcProfile {
        SpcProfile {
            sample_size,
            weco_rule: SpcWecoRule::default(),
            alert_threshold: SpcAlertThreshold::Zone4,
        }
    }

    #[test]
    fn fit_adaptive_chunk_size_small_data() {
        let values: Vec<f64> = (0..500).map(|value| (value as f64).sin()).collect();
        let batch = numeric_batch("x", values);
        let profile = spc_profile(0);
        let fname = feature("x");

        let baseline =
            fit_spc_baseline(&batch, &profile, std::slice::from_ref(&fname)).expect("baseline");

        assert_eq!(baseline.chunk_size, 25);
        let fitted = baseline.features.get(&fname).expect("feature");
        assert!(fitted.three_lcl < fitted.center && fitted.center < fitted.three_ucl);
    }

    #[test]
    fn fit_explicit_chunk_size() {
        let values: Vec<f64> = (0..1_000).map(|value| (value as f64) * 0.1).collect();
        let batch = numeric_batch("x", values);
        let profile = spc_profile(50);
        let fname = feature("x");

        let baseline = fit_spc_baseline(&batch, &profile, &[fname]).expect("baseline");

        assert_eq!(baseline.chunk_size, 50);
    }

    #[test]
    fn fit_from_int_column() {
        let values: Vec<i64> = (0..500).collect();
        let batch = int_batch("x", values);
        let profile = spc_profile(0);
        let fname = feature("x");

        let baseline =
            fit_spc_baseline(&batch, &profile, std::slice::from_ref(&fname)).expect("baseline");

        let fitted = baseline.features.get(&fname).expect("feature");
        assert!(fitted.center > 0.0);
    }

    #[test]
    fn fit_includes_trailing_partial_chunk_for_center() {
        let batch = numeric_batch("x", vec![0.0, 2.0, 2.0, 4.0, 100.0, 104.0]);
        let profile = spc_profile(4);
        let fname = feature("x");

        let baseline =
            fit_spc_baseline(&batch, &profile, std::slice::from_ref(&fname)).expect("baseline");

        let fitted = baseline.features.get(&fname).expect("feature");
        assert!((fitted.center - 52.0).abs() < 1e-12);
    }

    #[test]
    fn rejects_non_numeric_column() {
        let batch = RecordBatch::try_new(
            Arc::new(Schema::new(vec![Field::new("name", DataType::Utf8, true)])),
            vec![Arc::new(StringArray::from(vec!["a", "b", "c"]))],
        )
        .expect("record batch");
        let profile = spc_profile(0);
        let fname = feature("name");

        let err = fit_spc_baseline(&batch, &profile, &[fname]).expect_err("fit error");

        assert!(matches!(err, DriftFitError::FeatureNotNumeric { .. }));
    }

    #[test]
    fn rejects_empty_column() {
        let batch = numeric_batch("x", Vec::new());
        let profile = spc_profile(0);
        let fname = feature("x");

        let err = fit_spc_baseline(&batch, &profile, &[fname]).expect_err("fit error");

        assert!(matches!(err, DriftFitError::FeatureEmpty { .. }));
    }

    #[test]
    fn rejects_insufficient_chunks() {
        let values: Vec<f64> = (0..25).map(|value| value as f64).collect();
        let batch = numeric_batch("x", values);
        let profile = spc_profile(25);
        let fname = feature("x");

        let err = fit_spc_baseline(&batch, &profile, &[fname]).expect_err("fit error");

        assert!(matches!(
            err,
            DriftFitError::InsufficientSamplesForChunk { .. }
        ));
    }

    #[test]
    fn rejects_missing_feature() {
        let batch = numeric_batch("x", (0..500).map(|value| value as f64).collect());
        let profile = spc_profile(0);
        let fname = feature("not_x");

        let err = fit_spc_baseline(&batch, &profile, &[fname]).expect_err("fit error");

        assert!(matches!(err, DriftFitError::FeatureMissing { .. }));
    }

    #[test]
    fn rejects_non_finite_values() {
        let batch = numeric_batch("x", vec![1.0, f64::NAN, 3.0]);
        let profile = spc_profile(2);
        let fname = feature("x");

        let err = fit_spc_baseline(&batch, &profile, &[fname]).expect_err("fit error");

        assert!(matches!(err, DriftFitError::NonFiniteValuesInColumn { .. }));
    }
}

#[cfg(test)]
mod spc_score {
    //! End-to-end tests for SPC scoring.

    use std::sync::Arc;

    use arrow::array::{Float64Array, StringArray};
    use arrow::record_batch::RecordBatch;
    use arrow_schema::{DataType, Field, Schema};

    use crate::{DriftScoreError, DriftVerdict, fit_spc_baseline, score_spc};
    use wyrd_spec::card::drift::{SpcAlertThreshold, SpcProfile, SpcWecoRule};
    use wyrd_spec::ids::FeatureName;

    fn numeric_batch(name: &str, values: Vec<f64>) -> RecordBatch {
        let schema = Schema::new(vec![Field::new(name, DataType::Float64, true)]);
        let array = Float64Array::from(values);
        RecordBatch::try_new(Arc::new(schema), vec![Arc::new(array)])
            .expect("test batch should be valid")
    }

    fn string_batch(name: &str, values: Vec<&str>) -> RecordBatch {
        let schema = Schema::new(vec![Field::new(name, DataType::Utf8, true)]);
        let array = StringArray::from(values);
        RecordBatch::try_new(Arc::new(schema), vec![Arc::new(array)])
            .expect("test batch should be valid")
    }

    fn profile_default() -> SpcProfile {
        SpcProfile {
            sample_size: 0,
            weco_rule: SpcWecoRule::default(),
            alert_threshold: SpcAlertThreshold::Zone4,
        }
    }

    #[test]
    fn spc_in_control_target_no_drift() {
        let baseline_batch = numeric_batch("x", vec![0.0; 500]);
        let target = numeric_batch("x", vec![0.0; 500]);
        let profile = profile_default();
        let feature = FeatureName::new("x").expect("valid feature name");
        let baseline =
            fit_spc_baseline(&baseline_batch, &profile, &[feature]).expect("SPC fit should pass");
        let report = score_spc(&baseline, &target, &profile).expect("SPC score should pass");
        assert_eq!(report.verdict, DriftVerdict::NoDrift);
    }

    #[test]
    fn spc_out_of_bounds_target_drift_zone4() {
        let baseline_values: Vec<f64> = (0..500).map(|idx| ((idx as f64) * 0.01).sin()).collect();
        let baseline_batch = numeric_batch("x", baseline_values);
        let target = numeric_batch("x", vec![100.0; 200]);
        let profile = profile_default();
        let feature = FeatureName::new("x").expect("valid feature name");
        let baseline =
            fit_spc_baseline(&baseline_batch, &profile, &[feature]).expect("SPC fit should pass");
        let report = score_spc(&baseline, &target, &profile).expect("SPC score should pass");
        assert_eq!(report.verdict, DriftVerdict::Drift);
    }

    #[test]
    fn spc_drift_with_zone1_threshold_detects_run() {
        let baseline_values: Vec<f64> = (0..500).map(|idx| ((idx as f64) * 0.1).sin()).collect();
        let baseline_batch = numeric_batch("x", baseline_values);
        let target = numeric_batch("x", vec![0.05; 200]);
        let profile = SpcProfile {
            sample_size: 0,
            weco_rule: SpcWecoRule::default(),
            alert_threshold: SpcAlertThreshold::Zone1,
        };
        let feature = FeatureName::new("x").expect("valid feature name");
        let baseline =
            fit_spc_baseline(&baseline_batch, &profile, &[feature]).expect("SPC fit should pass");
        let report = score_spc(&baseline, &target, &profile).expect("SPC score should pass");
        assert_eq!(report.verdict, DriftVerdict::Drift);
    }

    #[test]
    fn spc_target_smaller_than_chunk_size_errors() {
        let baseline_batch = numeric_batch("x", (0..500).map(|idx| idx as f64).collect());
        let target = numeric_batch("x", vec![1.0, 2.0, 3.0]);
        let profile = profile_default();
        let feature = FeatureName::new("x").expect("valid feature name");
        let baseline =
            fit_spc_baseline(&baseline_batch, &profile, &[feature]).expect("SPC fit should pass");
        let err = score_spc(&baseline, &target, &profile).expect_err("target should be too small");
        assert!(matches!(err, DriftScoreError::TargetTooSmall { .. }));
    }

    #[test]
    fn spc_missing_feature_in_target() {
        let baseline_batch = numeric_batch("x", (0..500).map(|idx| idx as f64).collect());
        let target = numeric_batch("y", vec![1.0; 100]);
        let profile = profile_default();
        let feature = FeatureName::new("x").expect("valid feature name");
        let baseline =
            fit_spc_baseline(&baseline_batch, &profile, &[feature]).expect("SPC fit should pass");
        let err =
            score_spc(&baseline, &target, &profile).expect_err("target feature should be absent");
        assert!(matches!(
            err,
            DriftScoreError::FeatureMissingInTarget { .. }
        ));
    }

    #[test]
    fn spc_non_numeric_target_errors() {
        let baseline_batch = numeric_batch("x", (0..500).map(|idx| idx as f64).collect());
        let target = string_batch("x", vec!["1", "2", "3"]);
        let profile = profile_default();
        let feature = FeatureName::new("x").expect("valid feature name");
        let baseline =
            fit_spc_baseline(&baseline_batch, &profile, &[feature]).expect("SPC fit should pass");
        let err =
            score_spc(&baseline, &target, &profile).expect_err("target feature is not numeric");
        assert!(matches!(err, DriftScoreError::FeatureTypeMismatch { .. }));
    }
}
