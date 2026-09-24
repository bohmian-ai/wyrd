//! Top-level baseline fit and score dispatch.

use serde::{Deserialize, Serialize};
use wyrd_spec::card::drift::{DriftMethod, DriftProfile, DriftSignal, DriftSpec};

use crate::custom::score_custom;
use crate::error::{DriftFitError, DriftScoreError};
use crate::psi::PsiBaseline;
use crate::psi::{fit_psi_baseline_until, score_psi};
use crate::report::DriftReport;
use crate::spc::SpcBaseline;
use crate::spc::{fit_spc_baseline_until, score_spc};

/// Fitted baseline state produced by `fit_baseline`.
///
/// Serializable so the server's baseline store persists it as the fitted
/// profile of one Verifier version and the Drift engine reads it back.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum FittedBaseline {
    /// PSI fitted baseline state.
    Psi(PsiBaseline),
    /// SPC fitted baseline state.
    Spc(SpcBaseline),
    /// Custom carries no state because there is no fit phase.
    Custom,
}

/// Format of every PSI and SPC fitted profile this crate writes.
///
/// Format 2 is the exhaustive-bin PSI and NIST X-bar/S SPC fit. Profiles
/// fitted earlier carry no format and are refused rather than rescored under
/// different math; their Verifier needs a new version and a new fit.
pub const FITTED_FORMAT: u32 = 2;

/// Rows a fit loop processes between two cancellation checks.
///
/// Bounds how long a cancelled fit keeps computing inside one large feature.
pub(crate) const CANCEL_CHECK_ROWS: usize = 65_536;

/// Fail with [`DriftFitError::Cancelled`] once `cancelled` reports true.
///
/// Fit loops call this before each feature, between a feature's phases, and
/// every [`CANCEL_CHECK_ROWS`] rows, so a caller's stop signal ends the fit
/// without waiting for the whole batch.
///
/// # Errors
/// Returns [`DriftFitError::Cancelled`] when `cancelled()` is true.
pub(crate) fn ensure_live(cancelled: &dyn Fn() -> bool) -> Result<(), DriftFitError> {
    if cancelled() {
        return Err(DriftFitError::Cancelled);
    }
    Ok(())
}

/// Fit a method-specific baseline from an Arrow `RecordBatch`.
///
/// Runs to completion; see [`fit_baseline_until`] for a cancellable fit.
///
/// # Errors
/// Returns a [`DriftFitError`] when the `DriftSpec` does not carry the signal
/// and profile shape required by the selected method, or when the method-level
/// fit fails.
pub fn fit_baseline(
    batch: &arrow::record_batch::RecordBatch,
    spec: &DriftSpec,
) -> Result<FittedBaseline, crate::DriftFitError> {
    fit_baseline_until(batch, spec, &|| false)
}

/// Fit a method-specific baseline, stopping once `cancelled` reports true.
///
/// Produces exactly what [`fit_baseline`] produces while `cancelled` stays
/// false. PSI and SPC poll `cancelled` before each feature, between a
/// feature's collect, edge, and binning phases, and every
/// [`CANCEL_CHECK_ROWS`] rows, so a blocking caller can stop a long fit
/// promptly. A single quantile sort is not interrupted.
///
/// # Errors
/// Returns [`DriftFitError::Cancelled`] once `cancelled` reports true, and
/// otherwise the errors of [`fit_baseline`].
pub fn fit_baseline_until(
    batch: &arrow::record_batch::RecordBatch,
    spec: &DriftSpec,
    cancelled: &dyn Fn() -> bool,
) -> Result<FittedBaseline, crate::DriftFitError> {
    match spec.method {
        DriftMethod::Psi => {
            let features = match &spec.signal {
                DriftSignal::Distribution { features, .. } => features.as_slice(),
                other => {
                    return Err(DriftFitError::SignalShapeMismatch {
                        got: signal_variant(other),
                    });
                }
            };
            let profile = match spec.profile.as_ref() {
                Some(DriftProfile::Psi(profile)) => profile,
                _ => {
                    return Err(DriftFitError::PsiInternal {
                        message: "PSI method requires DriftProfile::Psi".to_string(),
                    });
                }
            };
            fit_psi_baseline_until(batch, profile, features, cancelled).map(FittedBaseline::Psi)
        }
        DriftMethod::Spc => {
            let profile = match spec.profile.as_ref() {
                Some(DriftProfile::Spc(profile)) => profile,
                _ => {
                    return Err(DriftFitError::SpcInternal {
                        message: "SPC method requires DriftProfile::Spc".to_string(),
                    });
                }
            };
            let features = match &spec.signal {
                DriftSignal::Distribution { features, .. } => features.as_slice(),
                other @ DriftSignal::Metric { .. } => {
                    return Err(DriftFitError::SignalShapeMismatch {
                        got: signal_variant(other),
                    });
                }
            };
            fit_spc_baseline_until(batch, profile, features, cancelled).map(FittedBaseline::Spc)
        }
        DriftMethod::Custom => Ok(FittedBaseline::Custom),
    }
}

/// Score a target Arrow `RecordBatch` against a fitted baseline.
///
/// # Errors
/// Returns a [`DriftScoreError`] when the fitted baseline does not match the
/// selected method, the method profile is missing or mismatched, or scoring
/// fails.
pub fn score_drift(
    baseline: &FittedBaseline,
    target: &arrow::record_batch::RecordBatch,
    spec: &DriftSpec,
) -> Result<DriftReport, crate::DriftScoreError> {
    match spec.method {
        DriftMethod::Psi => {
            let FittedBaseline::Psi(baseline) = baseline else {
                return Err(DriftScoreError::PsiInternal {
                    message: "PSI method requires FittedBaseline::Psi".to_string(),
                });
            };
            let profile = match spec.profile.as_ref() {
                Some(DriftProfile::Psi(profile)) => profile,
                _ => {
                    return Err(DriftScoreError::PsiInternal {
                        message: "PSI method requires DriftProfile::Psi".to_string(),
                    });
                }
            };
            score_psi(baseline, target, profile)
        }
        DriftMethod::Spc => {
            let FittedBaseline::Spc(baseline) = baseline else {
                return Err(DriftScoreError::SpcInternal {
                    message: "SPC method requires FittedBaseline::Spc".to_string(),
                });
            };
            score_spc(baseline, target)
        }
        DriftMethod::Custom => {
            if !matches!(baseline, FittedBaseline::Custom) {
                return Err(DriftScoreError::CustomInternal {
                    message: "Custom method requires FittedBaseline::Custom".to_string(),
                });
            }
            let profile = match spec.profile.as_ref() {
                Some(DriftProfile::Custom(profile)) => profile,
                _ => {
                    return Err(DriftScoreError::CustomInternal {
                        message: "Custom method requires DriftProfile::Custom".to_string(),
                    });
                }
            };
            score_custom(target, profile)
        }
    }
}

fn signal_variant(signal: &DriftSignal) -> &'static str {
    match signal {
        DriftSignal::Distribution { .. } => "Distribution",
        DriftSignal::Metric { .. } => "Metric",
    }
}

#[cfg(test)]
mod dispatch_errors {
    //! Tests for the four untested error arms in the fit_baseline / score_drift dispatch layer.

    use std::collections::BTreeMap;
    use std::sync::Arc;

    use crate::{DriftFitError, DriftScoreError};
    use crate::{FittedBaseline, SpcBaseline, fit_baseline, score_drift};
    use arrow::array::Float64Array;
    use arrow::record_batch::RecordBatch;
    use arrow_schema::{DataType, Field, Schema};
    use wyrd_semver::VersionBlock;
    use wyrd_spec::card::drift::{
        DriftCondition, DriftMethod, DriftProfile, DriftSignal, DriftSpec, PsiBinningStrategy,
        PsiProfile, PsiThreshold, SpcProfile,
    };
    use wyrd_spec::envelope::CardKind;
    use wyrd_spec::ids::{CardName, FeatureName, SpaceName};
    use wyrd_spec::reference::CardRef;

    fn data_ref(name: &str) -> CardRef {
        CardRef {
            kind: CardKind::Data,
            name: CardName::new(name).expect("valid name"),
            version: VersionBlock::parse("1.0.0").expect("valid version"),
            space: Some(SpaceName::new("default").expect("valid space")),
            uid: None,
        }
    }

    fn empty_batch() -> RecordBatch {
        RecordBatch::try_new(
            Arc::new(Schema::new(vec![Field::new("x", DataType::Float64, true)])),
            vec![Arc::new(Float64Array::from(vec![1.0_f64]))],
        )
        .expect("record batch")
    }

    /// Build an SPC + Metric spec without validation.
    ///
    /// Registration rejects this pair; the literal bypasses `DriftSpec::new`
    /// to prove the fitter also refuses it rather than reinterpreting it.
    fn spc_metric_unvalidated_spec() -> DriftSpec {
        DriftSpec {
            description: None,
            method: DriftMethod::Spc,
            signal: DriftSignal::Metric {
                name: "latency".to_owned(),
            },
            condition: DriftCondition::Statistical,
            profile: Some(DriftProfile::Spc(SpcProfile { sample_size: 5 })),
        }
    }

    fn psi_spec() -> DriftSpec {
        let feature = FeatureName::new("x").expect("valid feature");
        DriftSpec::new(
            DriftMethod::Psi,
            DriftSignal::Distribution {
                baseline_ref: data_ref("baseline").into(),
                features: vec![feature],
            },
            DriftCondition::Statistical,
            Some(DriftProfile::Psi(PsiProfile {
                binning_strategy: PsiBinningStrategy::EqualWidth { n_bins: 10 },
                categorical_features: vec![],
                threshold: PsiThreshold::Fixed { value: 0.25 },
            })),
            None,
        )
        .expect("valid psi spec")
    }

    fn spc_baseline_stub() -> FittedBaseline {
        use wyrd_version::WyrdVersion;
        FittedBaseline::Spc(SpcBaseline {
            features: BTreeMap::new(),
            subgroup_size: 25,
            format: super::FITTED_FORMAT,
            wyrd_version: WyrdVersion::current(),
        })
    }

    #[test]
    /// SPC fitting refuses a Metric signal instead of fitting it as Custom.
    fn fit_baseline_signal_shape_mismatch() {
        let spec = spc_metric_unvalidated_spec();
        let batch = empty_batch();
        let err = fit_baseline(&batch, &spec).expect_err("should error");
        assert!(
            matches!(err, DriftFitError::SignalShapeMismatch { .. }),
            "unexpected error: {err:?}"
        );
    }

    #[test]
    fn score_drift_cross_method_mismatch() {
        let spec = psi_spec();
        let baseline = spc_baseline_stub();
        let batch = empty_batch();
        let err = score_drift(&baseline, &batch, &spec).expect_err("should error");
        assert!(
            matches!(err, DriftScoreError::PsiInternal { .. }),
            "unexpected error: {err:?}"
        );
    }
}

#[cfg(test)]
mod end_to_end {
    //! In-memory end-to-end tests for top-level drift dispatch.

    use std::error::Error;
    use std::sync::Arc;

    use crate::{DriftReport, DriftVerdict, FittedBaseline, fit_baseline, score_drift};
    use arrow::array::Float64Array;
    use arrow::record_batch::RecordBatch;
    use arrow_schema::{DataType, Field, Schema};
    use wyrd_semver::VersionBlock;
    use wyrd_spec::card::drift::{
        CustomProfile, DriftCondition, DriftMethod, DriftProfile, DriftSignal, DriftSpec,
        PsiBinningStrategy, PsiProfile, PsiThreshold, SpcProfile,
    };
    use wyrd_spec::envelope::CardKind;
    use wyrd_spec::ids::{CardName, FeatureName, SpaceName};
    use wyrd_spec::reference::CardRef;

    fn numeric_batch(name: &str, values: Vec<f64>) -> Result<RecordBatch, Box<dyn Error>> {
        let schema = Schema::new(vec![Field::new(name, DataType::Float64, true)]);
        let array = Float64Array::from(values);
        Ok(RecordBatch::try_new(
            Arc::new(schema),
            vec![Arc::new(array)],
        )?)
    }

    fn card_ref(kind: CardKind, name: &str) -> Result<CardRef, Box<dyn Error>> {
        Ok(CardRef {
            kind,
            name: CardName::new(name)?,
            version: VersionBlock::parse("1.0.0")?,
            space: Some(SpaceName::new("default")?),
            uid: None,
        })
    }

    fn data_ref(name: &str) -> Result<CardRef, Box<dyn Error>> {
        card_ref(CardKind::Data, name)
    }

    fn psi_spec(feature: &FeatureName) -> Result<DriftSpec, Box<dyn Error>> {
        Ok(DriftSpec::new(
            DriftMethod::Psi,
            DriftSignal::Distribution {
                baseline_ref: data_ref("baseline-data")?.into(),
                features: vec![feature.clone()],
            },
            DriftCondition::Statistical,
            Some(DriftProfile::Psi(PsiProfile {
                binning_strategy: PsiBinningStrategy::EqualWidth { n_bins: 10 },
                categorical_features: vec![],
                threshold: PsiThreshold::Fixed { value: 0.25 },
            })),
            None,
        )?)
    }

    fn spc_distribution_spec(feature: &FeatureName) -> Result<DriftSpec, Box<dyn Error>> {
        Ok(DriftSpec::new(
            DriftMethod::Spc,
            DriftSignal::Distribution {
                baseline_ref: data_ref("baseline-data")?.into(),
                features: vec![feature.clone()],
            },
            DriftCondition::Statistical,
            Some(DriftProfile::Spc(spc_profile())),
            None,
        )?)
    }

    fn custom_spec(name: &str) -> Result<DriftSpec, Box<dyn Error>> {
        Ok(DriftSpec::new(
            DriftMethod::Custom,
            DriftSignal::Metric {
                name: name.to_string(),
            },
            DriftCondition::Statistical,
            Some(DriftProfile::Custom(CustomProfile {
                metric_name: name.to_string(),
                baseline_value: 100.0,
                alert_threshold: 5.0,
            })),
            None,
        )?)
    }

    fn spc_profile() -> SpcProfile {
        SpcProfile { sample_size: 5 }
    }

    fn assert_report_shape(
        report: &DriftReport,
        method: DriftMethod,
        feature: &FeatureName,
        verdict: DriftVerdict,
    ) {
        assert_eq!(report.method, method);
        assert_eq!(report.verdict, verdict);
        assert_eq!(report.features.len(), 1);
        let feature_report = report
            .features
            .get(feature)
            .unwrap_or_else(|| panic!("missing feature report for {feature}"));
        assert_eq!(feature_report.feature, *feature);
        assert_eq!(feature_report.verdict, verdict);
        assert!(feature_report.score.is_finite());
        assert!(feature_report.threshold.is_finite());
    }

    #[test]
    fn psi_dispatch_scores_report_shape() -> Result<(), Box<dyn Error>> {
        let feature = FeatureName::new("score")?;
        let spec = psi_spec(&feature)?;
        let baseline_batch = numeric_batch("score", (0..1_000).map(f64::from).collect())?;
        let baseline = fit_baseline(&baseline_batch, &spec)?;
        assert!(matches!(baseline, FittedBaseline::Psi(_)));

        let target = numeric_batch("score", (5_000..6_000).map(f64::from).collect())?;
        let report = score_drift(&baseline, &target, &spec)?;

        assert_report_shape(&report, DriftMethod::Psi, &feature, DriftVerdict::Drift);
        Ok(())
    }

    #[test]
    fn spc_distribution_dispatch_scores_report_shape() -> Result<(), Box<dyn Error>> {
        let feature = FeatureName::new("latency")?;
        let spec = spc_distribution_spec(&feature)?;
        let baseline_values = (0..500).map(|idx| (f64::from(idx) * 0.01).sin()).collect();
        let baseline_batch = numeric_batch("latency", baseline_values)?;
        let baseline = fit_baseline(&baseline_batch, &spec)?;
        assert!(matches!(baseline, FittedBaseline::Spc(_)));

        let target = numeric_batch("latency", vec![100.0; 200])?;
        let report = score_drift(&baseline, &target, &spec)?;

        assert_report_shape(&report, DriftMethod::Spc, &feature, DriftVerdict::Drift);
        Ok(())
    }

    #[test]
    fn custom_dispatch_scores_report_shape() -> Result<(), Box<dyn Error>> {
        let feature = FeatureName::new("latency_ms")?;
        let spec = custom_spec(feature.as_str())?;
        let baseline_batch = numeric_batch(feature.as_str(), vec![100.0])?;
        let baseline = fit_baseline(&baseline_batch, &spec)?;
        assert!(matches!(baseline, FittedBaseline::Custom));

        let target = numeric_batch(feature.as_str(), vec![200.0, 210.0, 220.0])?;
        let report = score_drift(&baseline, &target, &spec)?;

        assert_report_shape(&report, DriftMethod::Custom, &feature, DriftVerdict::Drift);
        Ok(())
    }
}

#[cfg(test)]
mod aggregate_inputs {
    //! Aggregate-input scoring matches raw-batch scoring, keeps insufficient
    //! input inconclusive, and fitted baselines round-trip through JSON.

    use std::collections::BTreeMap;
    use std::sync::Arc;

    use arrow::array::{ArrayRef, Float64Array, StringArray};
    use arrow::record_batch::RecordBatch;
    use arrow_schema::{DataType, Field, Schema};
    use wyrd_spec::card::drift::{
        CustomProfile, PsiBinningStrategy, PsiProfile, PsiThreshold, SpcProfile,
    };
    use wyrd_spec::ids::FeatureName;

    use crate::{
        DriftVerdict, FittedBaseline, SpcScorer, fit_psi_baseline, fit_spc_baseline,
        score_custom_mean, score_psi, score_psi_counts, score_spc,
    };

    /// Parse a fixture feature name.
    ///
    /// # Panics
    /// Panics when `name` is not a valid feature name.
    fn feature(name: &str) -> FeatureName {
        FeatureName::new(name).expect("valid feature name")
    }

    /// One-column batch named `name` holding `array`.
    ///
    /// # Panics
    /// Panics when the batch cannot be built.
    fn batch(name: &str, data_type: DataType, array: ArrayRef) -> RecordBatch {
        RecordBatch::try_new(
            Arc::new(Schema::new(vec![Field::new(name, data_type, true)])),
            vec![array],
        )
        .expect("record batch")
    }

    /// A numeric PSI profile with four equal-width bins and a fixed threshold.
    fn psi_profile(categorical: Vec<FeatureName>) -> PsiProfile {
        PsiProfile {
            binning_strategy: PsiBinningStrategy::EqualWidth { n_bins: 4 },
            categorical_features: categorical,
            threshold: PsiThreshold::Fixed { value: 0.1 },
        }
    }

    /// Server-side numeric bin counts equal raw-batch scoring, and a window
    /// under the minimum sample is inconclusive rather than a pass.
    ///
    /// # Panics
    /// Panics when the two paths disagree or small input is not inconclusive.
    #[test]
    fn psi_counts_match_raw_scoring_and_small_windows_are_inconclusive() {
        let x = feature("x");
        let profile = psi_profile(Vec::new());
        let base: Vec<f64> = (0..400).map(|value| f64::from(value % 100)).collect();
        let baseline = fit_psi_baseline(
            &batch("x", DataType::Float64, Arc::new(Float64Array::from(base))),
            &profile,
            std::slice::from_ref(&x),
        )
        .expect("baseline fits");
        let target: Vec<f64> = (0..200).map(|value| f64::from(value % 30)).collect();
        let edges = baseline.features[&x].numeric_edges().expect("edges");
        let mut bins = vec![0_u64; edges.len() - 1];
        for value in &target {
            bins[crate::psi::binning::assign_bin(*value, &edges)] += 1;
        }
        let counts = BTreeMap::from([(x.clone(), bins)]);
        let raw = score_psi(
            &baseline,
            &batch("x", DataType::Float64, Arc::new(Float64Array::from(target))),
            &profile,
        )
        .expect("raw scores");
        let aggregate = score_psi_counts(&baseline, &counts, &profile).expect("counts score");
        assert_eq!(raw, aggregate);
        assert_eq!(aggregate.verdict, DriftVerdict::Drift);

        let small = BTreeMap::from([(x.clone(), vec![99, 0, 0, 0])]);
        let report = score_psi_counts(&baseline, &small, &profile).expect("small scores");
        assert_eq!(report.verdict, DriftVerdict::Inconclusive);
        assert!(report.features[&x].score.is_nan());
    }

    /// Unseen categories land in the reserved `other` bin on both paths.
    ///
    /// # Panics
    /// Panics when raw scoring and `other`-bin counts disagree.
    #[test]
    fn psi_categorical_unknowns_land_in_the_other_bin() {
        let c = feature("c");
        let profile = psi_profile(vec![c.clone()]);
        let base: Vec<&str> = (0..200)
            .map(|i| if i % 2 == 0 { "a" } else { "b" })
            .collect();
        let baseline = fit_psi_baseline(
            &batch("c", DataType::Utf8, Arc::new(StringArray::from(base))),
            &profile,
            std::slice::from_ref(&c),
        )
        .expect("baseline fits");
        let target: Vec<&str> = (0..200)
            .map(|i| match i % 4 {
                0 | 1 => "a",
                2 => "b",
                _ => "unseen",
            })
            .collect();
        let raw = score_psi(
            &baseline,
            &batch("c", DataType::Utf8, Arc::new(StringArray::from(target))),
            &profile,
        )
        .expect("raw scores");
        let counts = BTreeMap::from([(c.clone(), vec![100, 50, 50])]);
        assert_eq!(
            raw,
            score_psi_counts(&baseline, &counts, &profile).expect("counts score")
        );
    }

    /// Server subgroup aggregates equal raw-batch SPC scoring, and a target
    /// ending in a partial subgroup is inconclusive.
    ///
    /// # Panics
    /// Panics when the two paths disagree or a partial target scores.
    #[test]
    fn spc_subgroups_match_raw_scoring_and_partial_targets_are_inconclusive() {
        let x = feature("x");
        let profile = SpcProfile { sample_size: 5 };
        let base: Vec<f64> = (0..100).map(|value| f64::from(value % 10)).collect();
        let baseline = fit_spc_baseline(
            &batch("x", DataType::Float64, Arc::new(Float64Array::from(base))),
            &profile,
            std::slice::from_ref(&x),
        )
        .expect("baseline fits");
        let target: Vec<f64> = (0..20).map(|value| 40.0 + f64::from(value)).collect();
        let mut scorer = SpcScorer::new(&baseline);
        for subgroup in target.chunks(5) {
            let (mean, sd) = crate::spc::control_limits::subgroup_stats(subgroup);
            scorer.push(&x, 5, mean, sd).expect("subgroup pushes");
        }
        let aggregate = scorer.finish();
        let raw = score_spc(
            &baseline,
            &batch("x", DataType::Float64, Arc::new(Float64Array::from(target))),
        )
        .expect("raw scores");
        assert_eq!(raw, aggregate);
        assert_eq!(aggregate.verdict, DriftVerdict::Drift);

        let mut partial = SpcScorer::new(&baseline);
        partial.push(&x, 5, 4.5, 2.0).expect("subgroup pushes");
        partial
            .push(&x, 4, f64::NAN, f64::NAN)
            .expect("partial pushes");
        let report = partial.finish();
        assert_eq!(report.verdict, DriftVerdict::Inconclusive);
        assert!(report.features[&x].score.is_nan());
    }

    /// The Custom window mean drifts only strictly above the threshold.
    ///
    /// # Panics
    /// Panics when equality drifts, excess does not, or a non-finite mean scores.
    #[test]
    fn custom_mean_equality_is_no_drift() {
        let profile = CustomProfile {
            metric_name: "latency".to_owned(),
            baseline_value: 10.0,
            alert_threshold: 2.0,
        };
        let equal = score_custom_mean(12.0, &profile).expect("equal scores");
        assert_eq!(equal.verdict, DriftVerdict::NoDrift);
        assert_eq!(equal.features[&feature("latency")].score, 2.0);
        assert_eq!(
            score_custom_mean(12.5, &profile)
                .expect("above scores")
                .verdict,
            DriftVerdict::Drift
        );
        assert!(score_custom_mean(f64::NAN, &profile).is_err());
    }

    /// Fitted baselines, including infinite numeric edges, round-trip through JSON.
    ///
    /// # Panics
    /// Panics when a baseline does not survive serialization unchanged.
    #[test]
    fn fitted_baselines_round_trip_through_json() {
        let x = feature("x");
        let values: Vec<f64> = (0..100).map(f64::from).collect();
        let fitted = FittedBaseline::Psi(
            fit_psi_baseline(
                &batch("x", DataType::Float64, Arc::new(Float64Array::from(values))),
                &psi_profile(Vec::new()),
                std::slice::from_ref(&x),
            )
            .expect("baseline fits"),
        );
        let json = serde_json::to_value(&fitted).expect("baseline serializes");
        assert_eq!(json["Psi"]["features"]["x"]["bins"][0]["lower"], "-inf");
        let restored: FittedBaseline = serde_json::from_value(json).expect("baseline restores");
        assert_eq!(restored, fitted);
    }
}

#[cfg(test)]
mod cancellation {
    //! Cancellation reaching a fit after it starts stops inside a feature.

    use std::cell::Cell;
    use std::sync::Arc;

    use crate::baseline::CANCEL_CHECK_ROWS;
    use crate::{DriftFitError, fit_baseline, fit_baseline_until};
    use arrow::array::{ArrayRef, Float64Array, StringArray};
    use arrow::record_batch::RecordBatch;
    use arrow_schema::{DataType, Field, Schema};
    use wyrd_semver::VersionBlock;
    use wyrd_spec::card::drift::{
        DriftCondition, DriftMethod, DriftProfile, DriftSignal, DriftSpec, PsiBinningStrategy,
        PsiProfile, PsiThreshold, SpcProfile,
    };
    use wyrd_spec::envelope::CardKind;
    use wyrd_spec::ids::{CardName, FeatureName, SpaceName};
    use wyrd_spec::reference::CardRef;

    /// Rows per fixture: three cancellation chunks, so a mid-loop stop leaves work undone.
    const ROWS: usize = CANCEL_CHECK_ROWS * 3;

    /// One-column batch holding `ROWS` numeric values under `num` and labels under `cat`.
    ///
    /// # Panics
    /// Panics when Arrow rejects the fixed two-column schema.
    fn batch() -> RecordBatch {
        let numbers: ArrayRef = Arc::new(Float64Array::from_iter_values(
            (0..ROWS).map(|row| (row % 997) as f64),
        ));
        let labels: ArrayRef = Arc::new(StringArray::from_iter_values(
            (0..ROWS).map(|row| ["a", "b", "c"][row % 3]),
        ));
        RecordBatch::try_new(
            Arc::new(Schema::new(vec![
                Field::new("num", DataType::Float64, false),
                Field::new("cat", DataType::Utf8, false),
            ])),
            vec![numbers, labels],
        )
        .expect("fixture batch")
    }

    /// A Distribution Drift spec over one feature with `profile`.
    ///
    /// # Panics
    /// Panics when the fixed spec fails validation.
    fn spec(method: DriftMethod, feature: &str, profile: DriftProfile) -> DriftSpec {
        let baseline = CardRef {
            kind: CardKind::Data,
            name: CardName::new("baseline").expect("valid name"),
            version: VersionBlock::parse("1.0.0").expect("valid version"),
            space: Some(SpaceName::new("default").expect("valid space")),
            uid: None,
        };
        DriftSpec::new(
            method,
            DriftSignal::Distribution {
                baseline_ref: baseline.into(),
                features: vec![FeatureName::new(feature).expect("valid feature")],
            },
            DriftCondition::Statistical,
            Some(profile),
            None,
        )
        .expect("valid drift spec")
    }

    /// PSI profile using quantile bins, with `categorical` as its categorical features.
    ///
    /// # Panics
    /// Panics when a name is not a valid feature name.
    fn psi(categorical: &[&str]) -> DriftProfile {
        DriftProfile::Psi(PsiProfile {
            binning_strategy: PsiBinningStrategy::Quantile { n_bins: 10 },
            categorical_features: categorical
                .iter()
                .map(|name| FeatureName::new(*name).expect("valid feature"))
                .collect(),
            threshold: PsiThreshold::Fixed { value: 0.25 },
        })
    }

    /// SPC profile whose subgroup size divides `ROWS`.
    fn spc() -> DriftProfile {
        DriftProfile::Spc(SpcProfile { sample_size: 3 })
    }

    /// Each method stops at the first check after cancellation flips mid-feature,
    /// and a never-cancelled fit equals the ordinary fit.
    ///
    /// The probe reports cancelled from its `stop_at`-th call. Earlier calls
    /// include the per-feature check, so the stop lands inside the feature:
    /// PSI numeric mid-binning, PSI categorical mid-count, SPC before limits.
    ///
    /// # Panics
    /// Panics when a fit ignores cancellation, polls past the stop, or an
    /// uncancelled fit differs from [`fit_baseline`].
    #[test]
    fn cancellation_after_fit_starts_stops_inside_the_feature() {
        let batch = batch();
        let cases = [
            (spec(DriftMethod::Psi, "num", psi(&[])), 3),
            (spec(DriftMethod::Psi, "cat", psi(&["cat"])), 2),
            (spec(DriftMethod::Spc, "num", spc()), 2),
        ];
        for (spec, stop_at) in cases {
            let calls = Cell::new(0_u32);
            let probe = || {
                calls.set(calls.get() + 1);
                calls.get() >= stop_at
            };
            let error = fit_baseline_until(&batch, &spec, &probe).expect_err("cancelled fit");
            assert!(matches!(error, DriftFitError::Cancelled), "{error:?}");
            assert_eq!(
                calls.get(),
                stop_at,
                "{:?} polled past the stop",
                spec.method
            );

            let uncancelled = fit_baseline_until(&batch, &spec, &|| false).expect("fit");
            assert_eq!(uncancelled, fit_baseline(&batch, &spec).expect("fit"));
        }
    }
}
