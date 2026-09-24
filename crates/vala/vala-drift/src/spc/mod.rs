//! SPC baseline fit and target scoring.

pub(crate) mod control_limits;
pub(crate) mod weco;

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use wyrd_spec::card::drift::{DriftMethod, SpcProfile};
use wyrd_spec::ids::FeatureName;
use wyrd_version::WyrdVersion;

use crate::error::{DriftFitError, DriftScoreError};
use crate::report::{DriftReport, DriftVerdict, FeatureDriftReport};
use crate::spc::control_limits::ControlLimits;
use crate::spc::weco::{WecoChecks, WecoScan, assign_zone, parse_rule};

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
/// order (the trailing partial chunk included) and feeds them through one
/// [`SpcScorer`], so raw-batch and server-streamed inputs share one zone and
/// rule evaluation.
///
/// # Errors
/// Returns [`DriftScoreError`] when the WECO rule is malformed, or when a
/// feature is missing, mistyped, empty, non-finite, or shorter than the
/// chunk size, or the baseline chunk size is zero.
pub fn score_spc(
    baseline: &SpcBaseline,
    target: &arrow::record_batch::RecordBatch,
    profile: &SpcProfile,
) -> Result<DriftReport, DriftScoreError> {
    use crate::feature::resolve_column;

    let chunk_size = baseline.chunk_size as usize;
    let mut scorer = SpcScorer::new(baseline, profile)?;
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
        if chunk_size == 0 {
            return Err(DriftScoreError::SpcInternal {
                message: "chunk_size must be greater than zero".into(),
            });
        }
        for chunk in values.chunks(chunk_size) {
            let mean = chunk.iter().sum::<f64>() / chunk.len() as f64;
            scorer.push(feature_name, chunk.len() as u64, mean)?;
        }
    }
    Ok(scorer.finish())
}

/// Streaming SPC scorer over ordered chunk aggregates for one fitted baseline.
///
/// The caller feeds each feature's chunk means in chunk order (the trailing
/// shorter chunk included) and then calls [`SpcScorer::finish`]. Each mean is
/// assigned its control-limit zone and run through an incremental WECO scan,
/// so per-feature state is bounded by the largest rule or trend lookback, not
/// by the number of chunks. Features are scored independently; their chunks
/// may arrive in any interleaving as long as each feature's own order holds.
#[derive(Debug, Clone)]
pub struct SpcScorer {
    /// Frozen baseline chunk size; a feature with fewer total rows is
    /// inconclusive.
    chunk_size: u32,
    /// WECO checks resolved once from the profile's rule and alert threshold.
    checks: WecoChecks,
    /// Per-feature scan state, one entry per baseline feature.
    features: BTreeMap<FeatureName, SpcFeatureScan>,
}

/// Running SPC state of one baseline feature inside an [`SpcScorer`].
#[derive(Debug, Clone)]
struct SpcFeatureScan {
    /// The feature's fitted control limits used for zone assignment.
    limits: ControlLimits,
    /// Total target rows across every chunk pushed so far.
    rows: u64,
    /// Rule violations fired so far; the feature's score.
    violations: u64,
    /// Bounded trailing-zone WECO scan.
    scan: WecoScan,
}

impl SpcScorer {
    /// Parse the profile's WECO rule once and seed empty state for every
    /// baseline feature.
    ///
    /// # Errors
    /// Returns [`DriftScoreError::WecoMalformed`] when the profile's rule is
    /// not eight positive integers.
    pub fn new(baseline: &SpcBaseline, profile: &SpcProfile) -> Result<Self, DriftScoreError> {
        let rule = parse_rule(&profile.weco_rule.rule_string)?;
        let features = baseline
            .features
            .iter()
            .map(|(name, fitted)| {
                let state = SpcFeatureScan {
                    limits: ControlLimits {
                        center: fitted.center,
                        one_lcl: fitted.one_lcl,
                        one_ucl: fitted.one_ucl,
                        two_lcl: fitted.two_lcl,
                        two_ucl: fitted.two_ucl,
                        three_lcl: fitted.three_lcl,
                        three_ucl: fitted.three_ucl,
                    },
                    rows: 0,
                    violations: 0,
                    scan: WecoScan::default(),
                };
                (name.clone(), state)
            })
            .collect();
        Ok(Self {
            chunk_size: baseline.chunk_size,
            checks: WecoChecks::new(&rule, profile.alert_threshold),
            features,
        })
    }

    /// Feed the next chunk of `feature`, in chunk order: `rows` values whose
    /// mean is `mean`.
    ///
    /// Adds `rows` to the feature's total, assigns the mean its zone, and
    /// counts every WECO rule firing at that chunk. Nothing changes on error.
    ///
    /// # Errors
    /// Returns [`DriftScoreError::SpcInternal`] when `feature` is not in the
    /// baseline or `mean` is not finite.
    pub fn push(
        &mut self,
        feature: &FeatureName,
        rows: u64,
        mean: f64,
    ) -> Result<(), DriftScoreError> {
        let state = self
            .features
            .get_mut(feature)
            .ok_or_else(|| DriftScoreError::SpcInternal {
                message: format!("feature {} is not in the SPC baseline", feature.as_str()),
            })?;
        if !mean.is_finite() {
            return Err(DriftScoreError::SpcInternal {
                message: "non-finite chunk mean in target".into(),
            });
        }
        state.rows = state.rows.saturating_add(rows);
        let violations = &mut state.violations;
        state
            .scan
            .push(&self.checks, assign_zone(mean, &state.limits), |_| {
                *violations += 1;
            });
        Ok(())
    }

    /// Build the drift report from every pushed chunk.
    ///
    /// A feature whose total rows are below the baseline chunk size (including
    /// one that received no chunk) is `Inconclusive` with NaN score and
    /// threshold. Otherwise the violation count is the score and any
    /// violation is `Drift`; the threshold is NaN because the verdict is
    /// rule-driven.
    #[must_use]
    pub fn finish(self) -> DriftReport {
        let chunk_size = u64::from(self.chunk_size);
        let features: BTreeMap<FeatureName, FeatureDriftReport> = self
            .features
            .into_iter()
            .map(|(feature, state)| {
                let (score, verdict) = if state.rows < chunk_size {
                    (f64::NAN, DriftVerdict::Inconclusive)
                } else if state.violations > 0 {
                    (state.violations as f64, DriftVerdict::Drift)
                } else {
                    (0.0, DriftVerdict::NoDrift)
                };
                let report = FeatureDriftReport {
                    feature: feature.clone(),
                    score,
                    threshold: f64::NAN,
                    verdict,
                };
                (feature, report)
            })
            .collect();
        let verdict = DriftReport::aggregate_verdict(&features);
        DriftReport {
            method: DriftMethod::Spc,
            features,
            verdict,
        }
    }

    /// Zones currently retained for `feature`, or `None` for an unknown
    /// feature; lets tests prove history stays within the lookback cap.
    #[cfg(test)]
    fn retained(&self, feature: &FeatureName) -> Option<usize> {
        self.features
            .get(feature)
            .map(|state| state.scan.retained())
    }
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

#[cfg(test)]
mod spc_scorer {
    //! Parity tests for the streaming [`SpcScorer`]. Expected counts are the
    //! violation counts the pre-streaming slice-rescanning evaluation produced
    //! for the same ordered zones.

    use wyrd_spec::card::drift::{SpcAlertThreshold, SpcProfile, SpcWecoRule};
    use wyrd_spec::ids::FeatureName;
    use wyrd_version::WyrdVersion;

    use super::{FittedSpcFeature, SpcBaseline, SpcScorer};
    use crate::{DriftReport, DriftScoreError, DriftVerdict};

    /// Parse a fixture feature name.
    ///
    /// # Panics
    /// Panics when `name` is not a valid feature name.
    fn feature(name: &str) -> FeatureName {
        FeatureName::new(name).expect("valid feature name")
    }

    /// Baseline over `names` with unit sigma limits around zero and chunk size 5.
    fn baseline(names: &[&str]) -> SpcBaseline {
        let fitted = FittedSpcFeature {
            center: 0.0,
            one_lcl: -1.0,
            one_ucl: 1.0,
            two_lcl: -2.0,
            two_ucl: 2.0,
            three_lcl: -3.0,
            three_ucl: 3.0,
        };
        SpcBaseline {
            features: names.iter().map(|name| (feature(name), fitted)).collect(),
            chunk_size: 5,
            wyrd_version: WyrdVersion::current(),
        }
    }

    /// Profile with `rule` and `alert_threshold`.
    fn profile(rule: &str, alert_threshold: SpcAlertThreshold) -> SpcProfile {
        SpcProfile {
            sample_size: 5,
            weco_rule: SpcWecoRule {
                rule_string: rule.to_owned(),
            },
            alert_threshold,
        }
    }

    /// A chunk mean that lands in signed zone `zone` under [`baseline`]'s limits.
    fn mean_in(zone: i8) -> f64 {
        match zone {
            0 => 0.0,
            z if z > 0 => f64::from(z) - 0.5,
            z => f64::from(z) + 0.5,
        }
    }

    /// Stream full chunks landing in `zones` for feature `x` and report.
    ///
    /// # Panics
    /// Panics when the rule is malformed or a push fails.
    fn score(zones: &[i8], rule: &str, threshold: SpcAlertThreshold) -> DriftReport {
        let mut scorer =
            SpcScorer::new(&baseline(&["x"]), &profile(rule, threshold)).expect("rule parses");
        for &zone in zones {
            scorer.push(&feature("x"), 5, mean_in(zone)).expect("push");
        }
        scorer.finish()
    }

    /// Assert feature `x` scored `expected` violations with the matching verdict.
    ///
    /// # Panics
    /// Panics when the score or verdict differ, or the threshold is not NaN.
    fn assert_score(report: &DriftReport, expected: f64) {
        let x = &report.features[&feature("x")];
        assert_eq!(x.score, expected);
        assert!(x.threshold.is_nan());
        let verdict = if expected > 0.0 {
            DriftVerdict::Drift
        } else {
            DriftVerdict::NoDrift
        };
        assert_eq!(x.verdict, verdict);
    }

    /// Nine same-side zone-1 chunks fire zone-1 consecutive (run 8) at the
    /// eighth and ninth chunk: score 2.
    ///
    /// # Panics
    /// Panics when the count differs from the old algorithm's.
    #[test]
    fn consecutive_run_counts_every_full_window() {
        assert_score(
            &score(&[1; 9], "8 16 4 8 2 4 1 1", SpcAlertThreshold::Zone1),
            2.0,
        );
    }

    /// The same zone-1 run is filtered out at alert threshold Zone2: score 0.
    ///
    /// # Panics
    /// Panics when a filtered zone still counts.
    #[test]
    fn alert_threshold_filters_lower_zones() {
        assert_score(
            &score(&[1; 9], "8 16 4 8 2 4 1 1", SpcAlertThreshold::Zone2),
            0.0,
        );
    }

    /// An authored alternating threshold of 16 fires once after 16
    /// alternating zone-1 chunks and never again; 15 chunks do not fire.
    ///
    /// # Panics
    /// Panics when the 16-long window is not honored or fires more than once.
    #[test]
    fn alternating_threshold_sixteen_fires_once() {
        let alternating: Vec<i8> = (0..20)
            .map(|idx| if idx % 2 == 0 { 1 } else { -1 })
            .collect();
        assert_score(
            &score(&alternating, "8 16 4 8 2 4 1 1", SpcAlertThreshold::Zone1),
            1.0,
        );
        assert_score(
            &score(
                &alternating[..15],
                "8 16 4 8 2 4 1 1",
                SpcAlertThreshold::Zone1,
            ),
            0.0,
        );
    }

    /// A strictly rising seven-chunk run fires the trend rule regardless of
    /// the alert threshold: score 1.
    ///
    /// # Panics
    /// Panics when the trend window does not fire exactly once.
    #[test]
    fn trend_window_fires_once() {
        assert_score(
            &score(
                &[-3, -2, -1, 0, 1, 2, 3],
                "8 16 4 8 2 4 1 1",
                SpcAlertThreshold::Zone4,
            ),
            1.0,
        );
    }

    /// A shorter trailing chunk is scored as a chunk: three zone-4 chunks
    /// (5, 5, and 3 rows) fire zone-4 consecutive three times plus zone-4
    /// alternating once: score 4.
    ///
    /// # Panics
    /// Panics when the trailing chunk is dropped or miscounted.
    #[test]
    fn trailing_short_chunk_is_scored() {
        let x = feature("x");
        let mut scorer = SpcScorer::new(
            &baseline(&["x"]),
            &profile("8 16 4 8 2 4 1 1", SpcAlertThreshold::Zone4),
        )
        .expect("rule parses");
        for rows in [5, 5, 3] {
            scorer.push(&x, rows, mean_in(4)).expect("push");
        }
        assert_score(&scorer.finish(), 4.0);
    }

    /// Fewer total rows than the chunk size is inconclusive with NaN score
    /// even when a rule fired, and a baseline feature that received no chunk
    /// is inconclusive while its sibling is scored.
    ///
    /// # Panics
    /// Panics when either feature is not inconclusive or the sibling is not scored.
    #[test]
    fn short_and_missing_features_are_inconclusive() {
        let mut scorer = SpcScorer::new(
            &baseline(&["x"]),
            &profile("8 16 4 8 2 4 1 1", SpcAlertThreshold::Zone4),
        )
        .expect("rule parses");
        scorer.push(&feature("x"), 4, mean_in(4)).expect("push");
        let short = &scorer.finish().features[&feature("x")];
        assert_eq!(short.verdict, DriftVerdict::Inconclusive);
        assert!(short.score.is_nan() && short.threshold.is_nan());

        let mut scorer = SpcScorer::new(
            &baseline(&["x", "y"]),
            &profile("8 16 4 8 2 4 1 1", SpcAlertThreshold::Zone4),
        )
        .expect("rule parses");
        scorer.push(&feature("x"), 5, mean_in(0)).expect("push");
        let report = scorer.finish();
        assert_score(&report, 0.0);
        let missing = &report.features[&feature("y")];
        assert_eq!(missing.verdict, DriftVerdict::Inconclusive);
        assert!(missing.score.is_nan() && missing.threshold.is_nan());
    }

    /// A malformed rule, an unknown feature, and a non-finite mean are
    /// rejected with their documented errors.
    ///
    /// # Panics
    /// Panics when any input is accepted or maps to the wrong error.
    #[test]
    fn invalid_inputs_are_rejected() {
        assert!(matches!(
            SpcScorer::new(
                &baseline(&["x"]),
                &profile("8 16 4", SpcAlertThreshold::Zone4)
            ),
            Err(DriftScoreError::WecoMalformed { .. })
        ));
        let mut scorer = SpcScorer::new(
            &baseline(&["x"]),
            &profile("8 16 4 8 2 4 1 1", SpcAlertThreshold::Zone4),
        )
        .expect("rule parses");
        assert!(matches!(
            scorer.push(&feature("nope"), 5, 0.0),
            Err(DriftScoreError::SpcInternal { .. })
        ));
        assert!(matches!(
            scorer.push(&feature("x"), 5, f64::NAN),
            Err(DriftScoreError::SpcInternal { .. })
        ));
    }

    /// Streaming 100_000 chunks keeps the retained per-feature history at or
    /// below the lookback cap of 16 (the default rule's largest threshold).
    ///
    /// # Panics
    /// Panics when the history outgrows the cap.
    #[test]
    fn retained_history_is_bounded_over_many_chunks() {
        let x = feature("x");
        let mut scorer = SpcScorer::new(
            &baseline(&["x"]),
            &profile("8 16 4 8 2 4 1 1", SpcAlertThreshold::Zone1),
        )
        .expect("rule parses");
        let mut max_retained = 0;
        for idx in 0..100_000i32 {
            scorer
                .push(&x, 5, mean_in((idx % 9 - 4) as i8))
                .expect("push");
            max_retained = max_retained.max(scorer.retained(&x).expect("known feature"));
        }
        assert_eq!(max_retained, 16);
    }
}
