//! SPC baseline fit and target scoring: a two-sided, three-sigma Shewhart
//! X-bar/S chart over fixed rational subgroups.
//!
//! Consecutive rows in observation order form subgroups of exactly the
//! authored `sample_size`. The author is responsible for supplying baseline
//! rows in process order from a stable process and a size whose consecutive
//! rows form meaningful subgroups; this module infers neither. A fit needs at
//! least [`MIN_BASELINE_SUBGROUPS`] complete subgroups and refuses leftover
//! rows. A target scores only complete subgroups: an empty target, or one
//! ending in a partial subgroup, is inconclusive. A subgroup mean or standard
//! deviation strictly outside its frozen limits is a signal, and the feature's
//! score is the total signal count of both charts with threshold zero.

pub(crate) mod control_limits;

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use wyrd_spec::card::drift::{DriftMethod, SpcProfile};
use wyrd_spec::ids::FeatureName;
use wyrd_version::WyrdVersion;

use crate::baseline::{FITTED_FORMAT, ensure_live};
use crate::error::{DriftFitError, DriftScoreError};
use crate::feature::{TargetColumn, required_values, resolve_column, target_complete};
use crate::report::{DriftReport, DriftVerdict, FeatureDriftReport, FeatureEvidence};
pub use crate::spc::control_limits::ChartLimits;
use crate::spc::control_limits::{fit_x_bar_s, subgroup_stats};

/// Complete baseline subgroups an SPC fit requires.
pub const MIN_BASELINE_SUBGROUPS: usize = 20;

/// SPC fitted baseline: frozen chart limits per feature and the subgroup size.
///
/// Serializable so the server's baseline store can persist it as the fitted
/// profile of one Verifier version.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpcBaseline {
    /// Frozen limits per feature.
    pub features: BTreeMap<FeatureName, FittedSpcFeature>,
    /// Rows per rational subgroup.
    pub subgroup_size: u32,
    /// Fitted-profile format; [`FITTED_FORMAT`] marks X-bar/S limits.
    pub format: u32,
    /// Vala version at fit time; used by the persistence layer for forward-compatibility checks.
    pub wyrd_version: WyrdVersion,
}

/// Per-feature frozen X-bar and S chart limits.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct FittedSpcFeature {
    /// Limits on subgroup means.
    pub x_bar: ChartLimits,
    /// Limits on subgroup sample standard deviations.
    pub s: ChartLimits,
}

/// The chart evidence behind one scored SPC feature.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpcEvidence {
    /// Rows per subgroup.
    pub subgroup_size: u32,
    /// Complete target subgroups scored.
    pub subgroups: u64,
    /// The X-bar chart and its signals.
    pub x_bar: SpcChartEvidence,
    /// The S chart and its signals.
    pub s: SpcChartEvidence,
}

/// One chart's frozen limits and the target subgroups outside them.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SpcChartEvidence {
    /// Center line and limits.
    #[serde(flatten)]
    pub limits: ChartLimits,
    /// Subgroups strictly outside the limits.
    pub signals: u64,
}

/// Fit an SPC baseline for `features` of `batch` under `profile`, uncancelled.
///
/// Runs [`fit_spc_baseline_until`] with a check that never fires.
///
/// # Errors
/// Returns the errors of [`fit_spc_baseline_until`] other than cancellation.
pub fn fit_spc_baseline(
    batch: &arrow::record_batch::RecordBatch,
    profile: &SpcProfile,
    features: &[FeatureName],
) -> Result<SpcBaseline, DriftFitError> {
    fit_spc_baseline_until(batch, profile, features, &|| false)
}

/// Fit an SPC baseline, stopping once `cancelled` reports true.
///
/// For each feature in order: reads every row, splits the rows into
/// consecutive subgroups of `profile.sample_size`, and fits X-bar/S limits from
/// the subgroup means and sample standard deviations. Polls `cancelled`
/// before each feature and between collecting its values and fitting; each
/// phase is one linear pass.
///
/// # Errors
/// Returns [`DriftFitError::Cancelled`] once `cancelled` reports true;
/// [`DriftFitError::FeatureMissing`], [`DriftFitError::FeatureNotNumeric`],
/// [`DriftFitError::FeatureEmpty`], [`DriftFitError::NullValuesInColumn`], or
/// [`DriftFitError::NonFiniteValuesInColumn`] for an unusable column;
/// [`DriftFitError::IncompleteSubgroup`] when rows remain after the last
/// complete subgroup; [`DriftFitError::InsufficientSubgroups`] below
/// [`MIN_BASELINE_SUBGROUPS`]; and [`DriftFitError::SpcInternal`] for a
/// subgroup size below two or a non-finite limit.
pub(crate) fn fit_spc_baseline_until(
    batch: &arrow::record_batch::RecordBatch,
    profile: &SpcProfile,
    features: &[FeatureName],
    cancelled: &dyn Fn() -> bool,
) -> Result<SpcBaseline, DriftFitError> {
    let n = profile.sample_size;
    let size = n as usize;
    let mut fitted = BTreeMap::new();
    for feature in features {
        ensure_live(cancelled)?;
        let name = || feature.as_str().to_string();
        let column = resolve_column(batch, feature)
            .map_err(|_| DriftFitError::FeatureMissing { feature: name() })?;
        let not_numeric = || DriftFitError::FeatureNotNumeric {
            feature: name(),
            arrow_type: column.data_type_string(),
        };
        if !column.is_numeric() {
            return Err(not_numeric());
        }
        let values = column.collect_f64().map_err(|_| not_numeric())?;
        let values = required_values(&column, values)?;
        if values.iter().any(|value| !value.is_finite()) {
            return Err(DriftFitError::NonFiniteValuesInColumn { feature: name() });
        }
        if size < 2 || values.len() % size != 0 {
            return Err(DriftFitError::IncompleteSubgroup {
                feature: name(),
                rows: values.len(),
                subgroup_size: n,
            });
        }
        let subgroups = values.len() / size;
        if subgroups < MIN_BASELINE_SUBGROUPS {
            return Err(DriftFitError::InsufficientSubgroups {
                feature: name(),
                subgroups,
                required: MIN_BASELINE_SUBGROUPS,
            });
        }

        ensure_live(cancelled)?;
        let stats = values
            .chunks_exact(size)
            .map(subgroup_stats)
            .collect::<Vec<_>>();
        let (x_bar, s) = fit_x_bar_s(&stats, n)?;
        fitted.insert(feature.clone(), FittedSpcFeature { x_bar, s });
    }

    Ok(SpcBaseline {
        features: fitted,
        subgroup_size: n,
        format: FITTED_FORMAT,
        wyrd_version: WyrdVersion::current(),
    })
}

/// Score a target `RecordBatch` against a fitted SPC baseline.
///
/// `target` is an already selected batch: each row is one relevant
/// observation and the caller has excluded unrelated ones. When any row
/// misses a feature or holds a null or non-finite value, the target is
/// [`DriftReport::unscored`]. Otherwise
/// each feature's values form consecutive subgroups in row order and stream
/// through one [`SpcScorer`], so raw-batch and server-aggregated inputs share
/// one chart evaluation.
///
/// # Errors
/// Returns [`DriftScoreError::FeatureTypeMismatch`] for a non-numeric target
/// column and the errors of [`SpcScorer::push`].
pub fn score_spc(
    baseline: &SpcBaseline,
    target: &arrow::record_batch::RecordBatch,
) -> Result<DriftReport, DriftScoreError> {
    let mut columns = Vec::with_capacity(baseline.features.len());
    for feature in baseline.features.keys() {
        let mismatch = || DriftScoreError::FeatureTypeMismatch {
            feature: feature.as_str().to_string(),
        };
        columns.push(match resolve_column(target, feature) {
            Err(_) => TargetColumn::Absent,
            Ok(column) if column.is_numeric() => {
                TargetColumn::Numeric(column.collect_f64().map_err(|_| mismatch())?)
            }
            Ok(_) => return Err(mismatch()),
        });
    }
    if !target_complete(target.num_rows(), &columns.iter().collect::<Vec<_>>()) {
        return Ok(DriftReport::unscored(DriftMethod::Spc));
    }
    let size = baseline.subgroup_size as usize;
    let mut scorer = SpcScorer::new(baseline);
    for (feature, column) in baseline.features.keys().zip(&columns) {
        for subgroup in column.numeric_values().chunks(size.max(1)) {
            let (mean, sd) = if subgroup.len() == size {
                subgroup_stats(subgroup)
            } else {
                (f64::NAN, f64::NAN)
            };
            scorer.push(feature, subgroup.len() as u64, mean, sd)?;
        }
    }
    Ok(scorer.finish())
}

/// Streaming X-bar/S scorer over ordered subgroup aggregates of one baseline.
///
/// The caller feeds each feature's subgroups in observation order, as row
/// count, mean, and sample standard deviation, then calls
/// [`SpcScorer::finish`]. State per feature is two signal counters, so it
/// stays constant however many subgroups arrive. A subgroup shorter than the
/// frozen size can only be the trailing one and makes the whole report
/// unscored; its statistics are ignored.
#[derive(Debug, Clone)]
pub struct SpcScorer {
    /// Frozen rows per subgroup.
    subgroup_size: u32,
    /// Chart state per baseline feature.
    features: BTreeMap<FeatureName, SpcFeatureChart>,
}

/// Running chart state of one baseline feature inside an [`SpcScorer`].
#[derive(Debug, Clone)]
struct SpcFeatureChart {
    /// The feature's frozen limits.
    limits: FittedSpcFeature,
    /// Complete subgroups pushed so far.
    subgroups: u64,
    /// Whether a partial subgroup arrived.
    partial: bool,
    /// Subgroup means strictly outside the X-bar limits.
    x_bar_signals: u64,
    /// Subgroup standard deviations strictly outside the S limits.
    s_signals: u64,
}

impl SpcScorer {
    /// Seed empty chart state for every baseline feature.
    #[must_use]
    pub fn new(baseline: &SpcBaseline) -> Self {
        let features = baseline
            .features
            .iter()
            .map(|(name, limits)| {
                let chart = SpcFeatureChart {
                    limits: *limits,
                    subgroups: 0,
                    partial: false,
                    x_bar_signals: 0,
                    s_signals: 0,
                };
                (name.clone(), chart)
            })
            .collect();
        Self {
            subgroup_size: baseline.subgroup_size,
            features,
        }
    }

    /// The frozen rows per complete subgroup.
    #[must_use]
    pub fn subgroup_size(&self) -> u32 {
        self.subgroup_size
    }

    /// Feed the next subgroup of `feature`: `rows` values with `mean` and
    /// sample standard deviation `sd`.
    ///
    /// A complete subgroup is checked against both charts; a shorter one
    /// marks the feature partial and ignores `mean` and `sd`. Nothing changes
    /// on error.
    ///
    /// # Errors
    /// Returns [`DriftScoreError::SpcInternal`] when `feature` is not in the
    /// baseline, `rows` exceeds the subgroup size, a subgroup follows a
    /// partial one, or a complete subgroup's statistics are not finite.
    pub fn push(
        &mut self,
        feature: &FeatureName,
        rows: u64,
        mean: f64,
        sd: f64,
    ) -> Result<(), DriftScoreError> {
        let internal = |message: &str| DriftScoreError::SpcInternal {
            message: format!("feature {}: {message}", feature.as_str()),
        };
        let size = u64::from(self.subgroup_size);
        let chart = self
            .features
            .get_mut(feature)
            .ok_or_else(|| internal("not in the SPC baseline"))?;
        if rows > size || chart.partial {
            return Err(internal(
                "a subgroup follows a partial subgroup or is oversized",
            ));
        }
        if rows < size {
            chart.partial = true;
            return Ok(());
        }
        if !mean.is_finite() || !sd.is_finite() {
            return Err(internal("non-finite subgroup statistics"));
        }
        chart.subgroups += 1;
        chart.x_bar_signals += u64::from(chart.limits.x_bar.signals(mean));
        chart.s_signals += u64::from(chart.limits.s.signals(sd));
        Ok(())
    }

    /// Build the drift report from every pushed subgroup.
    ///
    /// When any feature has no complete subgroup or ends in a partial one,
    /// the whole target is incomplete and the report is
    /// [`DriftReport::unscored`]: no feature is scored, so a signal on another
    /// feature cannot fail an incomplete run. Otherwise each feature's score
    /// is the X-bar plus S signal count, its threshold is zero, any signal is
    /// `Drift`, and it carries [`SpcEvidence`].
    #[must_use]
    pub fn finish(self) -> DriftReport {
        if self
            .features
            .values()
            .any(|chart| chart.partial || chart.subgroups == 0)
        {
            return DriftReport::unscored(DriftMethod::Spc);
        }
        let subgroup_size = self.subgroup_size;
        let features: BTreeMap<FeatureName, FeatureDriftReport> = self
            .features
            .into_iter()
            .map(|(feature, chart)| {
                let signals = chart.x_bar_signals + chart.s_signals;
                let report = FeatureDriftReport {
                    feature: feature.clone(),
                    score: signals as f64,
                    threshold: 0.0,
                    verdict: if signals > 0 {
                        DriftVerdict::Drift
                    } else {
                        DriftVerdict::NoDrift
                    },
                    evidence: Some(FeatureEvidence::Spc(SpcEvidence {
                        subgroup_size,
                        subgroups: chart.subgroups,
                        x_bar: SpcChartEvidence {
                            limits: chart.limits.x_bar,
                            signals: chart.x_bar_signals,
                        },
                        s: SpcChartEvidence {
                            limits: chart.limits.s,
                            signals: chart.s_signals,
                        },
                    })),
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
}

#[cfg(test)]
mod spc_fit {
    //! Baseline fitting: complete subgroups only, at least twenty of them,
    //! and no null or non-finite value.

    use std::sync::Arc;

    use crate::{DriftFitError, SpcBaseline, fit_spc_baseline};
    use arrow::array::{Float64Array, Int64Array, StringArray};
    use arrow::record_batch::RecordBatch;
    use arrow_schema::{DataType, Field, Schema};
    use wyrd_spec::card::drift::SpcProfile;
    use wyrd_spec::ids::FeatureName;

    /// Parse a fixture feature name.
    ///
    /// # Panics
    /// Panics when `name` is not a valid feature name.
    fn feature(name: &str) -> FeatureName {
        FeatureName::new(name).expect("valid feature name")
    }

    /// One nullable `Float64` column named `name`.
    ///
    /// # Panics
    /// Panics when Arrow rejects the batch.
    fn numeric_batch(name: &str, values: Vec<Option<f64>>) -> RecordBatch {
        RecordBatch::try_new(
            Arc::new(Schema::new(vec![Field::new(name, DataType::Float64, true)])),
            vec![Arc::new(Float64Array::from(values))],
        )
        .expect("record batch")
    }

    /// Fit `values` of feature `x` with subgroups of `n` through the public
    /// SPC fitter, so each test sees exactly what a Verifier fit would.
    ///
    /// # Errors
    /// Returns the fitter's [`DriftFitError`] for a size below two, an
    /// incomplete or trailing subgroup, too few subgroups, or a null or
    /// non-finite value.
    ///
    /// # Panics
    /// Panics when Arrow rejects the fixture batch.
    fn fit(values: Vec<Option<f64>>, n: u32) -> Result<SpcBaseline, DriftFitError> {
        fit_spc_baseline(
            &numeric_batch("x", values),
            &SpcProfile { sample_size: n },
            &[feature("x")],
        )
    }

    /// `subgroups` subgroups of five rows alternating around 10.
    fn stable(subgroups: usize) -> Vec<Option<f64>> {
        (0..subgroups * 5)
            .map(|row| Some(10.0 + [-2.0, -1.0, 0.0, 1.0, 2.0][row % 5]))
            .collect()
    }

    /// Twenty complete subgroups fit limits around the grand mean, integer
    /// columns fit too, and the fitted profile records its format.
    ///
    /// # Panics
    /// Panics when a valid baseline does not fit.
    #[test]
    fn twenty_complete_subgroups_fit() {
        let baseline = fit(stable(20), 5).expect("baseline");
        assert_eq!(baseline.subgroup_size, 5);
        assert_eq!(baseline.format, crate::baseline::FITTED_FORMAT);
        let x = baseline.features[&feature("x")];
        assert!((x.x_bar.center - 10.0).abs() < 1e-12);
        assert!(x.x_bar.lower < 10.0 && 10.0 < x.x_bar.upper);
        assert!(x.s.lower >= 0.0 && x.s.center < x.s.upper);

        let ints = RecordBatch::try_new(
            Arc::new(Schema::new(vec![Field::new("x", DataType::Int64, false)])),
            vec![Arc::new(Int64Array::from_iter_values(
                (0..100).map(|v| v % 7),
            ))],
        )
        .expect("record batch");
        fit_spc_baseline(&ints, &SpcProfile { sample_size: 5 }, &[feature("x")])
            .expect("integer baseline");
    }

    /// Nineteen subgroups, leftover rows, and a subgroup size below two fail.
    ///
    /// # Panics
    /// Panics when an incomplete or short baseline fits.
    #[test]
    fn short_or_ragged_baselines_fail_visibly() {
        assert!(matches!(
            fit(stable(19), 5),
            Err(DriftFitError::InsufficientSubgroups {
                subgroups: 19,
                required: 20,
                ..
            })
        ));
        let mut ragged = stable(20);
        ragged.push(Some(10.0));
        assert!(matches!(
            fit(ragged, 5),
            Err(DriftFitError::IncompleteSubgroup { rows: 101, .. })
        ));
        assert!(matches!(
            fit(stable(20), 1),
            Err(DriftFitError::IncompleteSubgroup { .. })
        ));
    }

    /// Null, NaN, and infinite baseline values fail without dropping rows.
    ///
    /// # Panics
    /// Panics when a malformed baseline fits.
    #[test]
    fn null_and_non_finite_baseline_values_fail() {
        for (bad, is_null) in [
            (None, true),
            (Some(f64::NAN), false),
            (Some(f64::INFINITY), false),
        ] {
            let mut values = stable(20);
            values[7] = bad;
            let error = fit(values, 5).expect_err("malformed baseline");
            if is_null {
                assert!(
                    matches!(error, DriftFitError::NullValuesInColumn { .. }),
                    "{error:?}"
                );
            } else {
                assert!(
                    matches!(error, DriftFitError::NonFiniteValuesInColumn { .. }),
                    "{error:?}"
                );
            }
        }
    }

    /// Missing, text, and empty columns keep their typed errors.
    ///
    /// # Panics
    /// Panics when a column error maps to the wrong variant.
    #[test]
    fn column_errors_are_typed() {
        let profile = SpcProfile { sample_size: 5 };
        let text = RecordBatch::try_new(
            Arc::new(Schema::new(vec![Field::new("x", DataType::Utf8, true)])),
            vec![Arc::new(StringArray::from(vec!["a", "b"]))],
        )
        .expect("record batch");
        assert!(matches!(
            fit_spc_baseline(&text, &profile, &[feature("x")]),
            Err(DriftFitError::FeatureNotNumeric { .. })
        ));
        assert!(matches!(
            fit(Vec::new(), 5),
            Err(DriftFitError::FeatureEmpty { .. })
        ));
        assert!(matches!(
            fit_spc_baseline(&numeric_batch("y", stable(20)), &profile, &[feature("x")]),
            Err(DriftFitError::FeatureMissing { .. })
        ));
    }
}

#[cfg(test)]
mod spc_score {
    //! Target scoring against independently computed limits: signals on
    //! either chart, equality at the limit, complete-subgroup gating, and
    //! the missing-value rules shared with the server path.

    use std::sync::Arc;

    use arrow::array::{ArrayRef, Float64Array, StringArray};
    use arrow::record_batch::RecordBatch;
    use arrow_schema::{DataType, Field, Schema};
    use wyrd_spec::card::drift::DriftMethod;
    use wyrd_spec::ids::FeatureName;
    use wyrd_version::WyrdVersion;

    use super::{ChartLimits, FittedSpcFeature, SpcBaseline, SpcScorer};
    use crate::report::FeatureEvidence;
    use crate::{DriftReport, DriftScoreError, DriftVerdict, score_spc};

    /// Parse a fixture feature name.
    ///
    /// # Panics
    /// Panics when `name` is not a valid feature name.
    fn feature(name: &str) -> FeatureName {
        FeatureName::new(name).expect("valid feature name")
    }

    /// Subgroups of two with X-bar limits `[9, 11]` and S limits `[0, sqrt(2)]`.
    fn baseline(names: &[&str]) -> SpcBaseline {
        let limits = FittedSpcFeature {
            x_bar: ChartLimits {
                center: 10.0,
                lower: 9.0,
                upper: 11.0,
            },
            s: ChartLimits {
                center: 1.0,
                lower: 0.0,
                upper: std::f64::consts::SQRT_2,
            },
        };
        SpcBaseline {
            features: names.iter().map(|name| (feature(name), limits)).collect(),
            subgroup_size: 2,
            format: crate::baseline::FITTED_FORMAT,
            wyrd_version: WyrdVersion::current(),
        }
    }

    /// A batch of nullable numeric columns.
    ///
    /// # Panics
    /// Panics when Arrow rejects the batch.
    fn batch(columns: &[(&str, Vec<Option<f64>>)]) -> RecordBatch {
        let fields = columns
            .iter()
            .map(|(name, _)| Field::new(*name, DataType::Float64, true))
            .collect::<Vec<_>>();
        let arrays = columns
            .iter()
            .map(|(_, values)| Arc::new(Float64Array::from(values.clone())) as ArrayRef)
            .collect();
        RecordBatch::try_new(Arc::new(Schema::new(fields)), arrays).expect("record batch")
    }

    /// Score `values` of `x` directly.
    ///
    /// # Panics
    /// Panics when scoring errors.
    fn score(values: &[f64]) -> DriftReport {
        let column = values.iter().copied().map(Some).collect();
        score_spc(&baseline(&["x"]), &batch(&[("x", column)])).expect("scores")
    }

    /// The X-bar and S signal counts of feature `x`.
    ///
    /// # Panics
    /// Panics when `x` carries no SPC evidence.
    fn signals(report: &DriftReport) -> (u64, u64) {
        match &report.features[&feature("x")].evidence {
            Some(FeatureEvidence::Spc(evidence)) => (evidence.x_bar.signals, evidence.s.signals),
            other => panic!("no SPC evidence: {other:?}"),
        }
    }

    /// In-control subgroups pass with score and threshold zero and evidence.
    ///
    /// # Panics
    /// Panics when a calm target signals.
    #[test]
    fn in_control_target_passes_with_evidence() {
        let report = score(&[9.5, 10.5, 10.0, 10.0]);
        let x = &report.features[&feature("x")];
        assert_eq!(
            (report.verdict, x.score, x.threshold),
            (DriftVerdict::NoDrift, 0.0, 0.0)
        );
        let Some(FeatureEvidence::Spc(evidence)) = &x.evidence else {
            panic!("SPC evidence");
        };
        assert_eq!((evidence.subgroup_size, evidence.subgroups), (2, 2));
        assert_eq!(evidence.x_bar.limits.upper, 11.0);
    }

    /// A mean beyond the X-bar limit and a spread beyond the S limit each
    /// signal; the score sums both charts.
    ///
    /// # Panics
    /// Panics when either chart misses its signal.
    #[test]
    fn each_chart_signals_independently() {
        // Mean 12 (X-bar signal, sd 0); then mean 10 with sd 2.83 (S signal).
        let report = score(&[12.0, 12.0, 8.0, 12.0]);
        assert_eq!(signals(&report), (1, 1));
        assert_eq!(report.features[&feature("x")].score, 2.0);
        assert_eq!(report.verdict, DriftVerdict::Drift);
    }

    /// A subgroup mean exactly at a limit and a standard deviation exactly at
    /// the S limit do not signal.
    ///
    /// # Panics
    /// Panics when equality signals.
    #[test]
    fn equality_at_the_limits_is_in_control() {
        // Mean 11 (= upper), sd 0 (= lower); mean 9 (= lower); sd sqrt(2) (= upper).
        let report = score(&[11.0, 11.0, 9.0, 9.0, 9.0, 11.0]);
        assert_eq!(signals(&report), (0, 0));
        assert_eq!(report.verdict, DriftVerdict::NoDrift);
    }

    /// An empty target and a trailing partial subgroup are unscored, never
    /// scored on shifted or dropped rows.
    ///
    /// # Panics
    /// Panics when either target scores.
    #[test]
    fn empty_and_partial_targets_are_inconclusive() {
        for values in [&[][..], &[12.0, 12.0, 10.0][..]] {
            assert_eq!(
                score(values),
                DriftReport::unscored(DriftMethod::Spc),
                "{values:?}"
            );
        }
    }

    /// A selected row missing a feature or holding null, NaN, or infinity is
    /// unscored with no feature rows. A direct batch is already selected, so
    /// a row null in every feature is a selected observation that makes an
    /// otherwise sufficient target unscored rather than shifting subgroups.
    ///
    /// # Panics
    /// Panics when an incomplete target scores.
    #[test]
    fn incomplete_targets_are_unscored() {
        let base = baseline(&["x", "y"]);
        let full = |x: Vec<Option<f64>>, y: Vec<Option<f64>>| {
            score_spc(&base, &batch(&[("x", x), ("y", y)])).expect("scores")
        };
        let unscored = DriftReport::unscored(DriftMethod::Spc);
        let calm = vec![Some(10.0), Some(10.0)];
        for bad in [None, Some(f64::NAN), Some(f64::NEG_INFINITY)] {
            assert_eq!(full(calm.clone(), vec![Some(10.0), bad]), unscored);
        }
        let only_x = score_spc(&base, &batch(&[("x", calm.clone())])).expect("scores");
        assert_eq!(only_x, unscored, "an omitted feature is unscored");

        let four = vec![Some(10.0); 4];
        assert_eq!(full(four.clone(), four).verdict, DriftVerdict::NoDrift);
        let with_null_row = full(
            vec![Some(10.0), None, Some(10.0), Some(10.0), Some(10.0)],
            vec![Some(10.0), None, Some(10.0), Some(10.0), Some(10.0)],
        );
        assert_eq!(with_null_row, unscored, "a null-only selected row");
    }

    /// One signaled complete feature beside an empty or trailing-partial
    /// feature leaves the whole report unscored, so the signal cannot fail
    /// an incomplete run.
    ///
    /// # Panics
    /// Panics when the signal escapes an incomplete report.
    #[test]
    fn a_signal_beside_an_incomplete_feature_is_unscored() {
        let base = baseline(&["x", "y"]);
        let (x, y) = (feature("x"), feature("y"));
        let signaled = || {
            let mut scorer = SpcScorer::new(&base);
            scorer.push(&x, 2, 12.0, 0.0).expect("signaled subgroup");
            scorer
        };
        let mut complete = signaled();
        complete.push(&y, 2, 10.0, 1.0).expect("calm subgroup");
        assert_eq!(complete.finish().verdict, DriftVerdict::Drift);

        let empty = signaled();
        let mut partial = signaled();
        partial.push(&y, 1, f64::NAN, f64::NAN).expect("partial");
        for scorer in [empty, partial] {
            assert_eq!(scorer.finish(), DriftReport::unscored(DriftMethod::Spc));
        }
    }

    /// The streaming scorer refuses unknown features, oversized or
    /// post-partial subgroups, and non-finite statistics, and keeps no
    /// per-subgroup history.
    ///
    /// # Panics
    /// Panics when an invalid push is accepted.
    #[test]
    fn scorer_rejects_invalid_pushes() {
        let x = feature("x");
        let mut scorer = SpcScorer::new(&baseline(&["x"]));
        assert!(matches!(
            scorer.push(&feature("nope"), 2, 10.0, 1.0),
            Err(DriftScoreError::SpcInternal { .. })
        ));
        assert!(scorer.push(&x, 3, 10.0, 1.0).is_err());
        assert!(scorer.push(&x, 2, f64::NAN, 1.0).is_err());
        for _ in 0..100_000 {
            scorer.push(&x, 2, 10.0, 1.0).expect("calm subgroup");
        }
        scorer
            .push(&x, 1, f64::NAN, f64::NAN)
            .expect("trailing partial");
        assert!(scorer.push(&x, 2, 10.0, 1.0).is_err());
        assert_eq!(scorer.finish().verdict, DriftVerdict::Inconclusive);
    }

    /// A non-numeric target column is a type mismatch.
    ///
    /// # Panics
    /// Panics when a text column scores.
    #[test]
    fn non_numeric_target_errors() {
        let text = RecordBatch::try_new(
            Arc::new(Schema::new(vec![Field::new("x", DataType::Utf8, true)])),
            vec![Arc::new(StringArray::from(vec!["1", "2"]))],
        )
        .expect("record batch");
        assert!(matches!(
            score_spc(&baseline(&["x"]), &text),
            Err(DriftScoreError::FeatureTypeMismatch { .. })
        ));
    }
}
