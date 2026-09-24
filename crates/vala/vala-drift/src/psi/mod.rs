//! PSI baseline fit and target scoring.

pub(crate) mod binning;
mod edge;
pub(crate) mod score;
pub(crate) mod threshold;

use std::collections::BTreeMap;
use std::collections::HashMap;

use arrow_schema::DataType;
use serde::{Deserialize, Serialize};
use wyrd_spec::card::drift::{DriftMethod, PsiBinningStrategy, PsiProfile};
use wyrd_spec::ids::FeatureName;
use wyrd_version::WyrdVersion;

use crate::baseline::{CANCEL_CHECK_ROWS, ensure_live};
use crate::error::{DriftFitError, DriftScoreError};
use crate::feature::{ColumnRef, resolve_column};
use crate::psi::binning::{assign_bin, compute_edges_equal_width, compute_edges_quantile};
pub use crate::psi::score::PSI_MIN_TARGET_SAMPLE;
use crate::psi::score::psi;
use crate::psi::threshold::compute_psi_threshold;
use crate::report::{DriftReport, DriftVerdict, FeatureDriftReport};

/// PSI fitted baseline, one entry per feature.
///
/// Serializable so the server's baseline store can persist it as the fitted
/// profile of one Verifier version.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PsiBaseline {
    pub features: BTreeMap<FeatureName, FittedPsiFeature>,
    /// Vala version at fit time; used by the persistence layer for forward-compatibility checks.
    pub wyrd_version: WyrdVersion,
}

/// Per-feature fitted PSI state.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FittedPsiFeature {
    pub feature: FeatureName,
    pub bin_type: BinType,
    pub bins: Vec<Bin>,
    pub total_count: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BinType {
    Numeric,
    Categorical,
}

/// One bin.
///
/// Numeric edges may be infinite (the outer bins are open), which JSON cannot
/// represent as a number, so edges serialize through [`edge`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Bin {
    pub id: i32,
    #[serde(with = "edge")]
    pub lower: Option<f64>,
    #[serde(with = "edge")]
    pub upper: Option<f64>,
    pub categorical_value: Option<String>,
    pub proportion: f64,
}

/// Fit a PSI baseline for `features` of `batch` under `profile`, uncancelled.
///
/// The ordinary entry point for callers with no cancellation: it runs the same
/// per-feature fit as the cancellable fitter with a check that never fires,
/// so its output is identical. Categorical features in `profile` get one bin
/// per observed value; every other feature is binned numerically by the
/// profile's binning strategy.
///
/// # Errors
/// Returns [`DriftFitError::FeatureMissing`] when a feature is not a column
/// of `batch`; [`DriftFitError::FeatureNotNumeric`] or
/// [`DriftFitError::FeatureNotCategorical`] when a column has the wrong Arrow
/// type; [`DriftFitError::FeatureEmpty`] when a column has no non-null rows;
/// [`DriftFitError::NonFiniteValuesInColumn`] when a numeric column holds NaN
/// or infinity; and [`DriftFitError::PsiInternal`] when bin edges or bin
/// indices cannot be computed.
pub fn fit_psi_baseline(
    batch: &arrow::record_batch::RecordBatch,
    profile: &PsiProfile,
    features: &[FeatureName],
) -> Result<PsiBaseline, DriftFitError> {
    fit_psi_baseline_until(batch, profile, features, &|| false)
}

/// Fit a PSI baseline, stopping once `cancelled` reports true.
///
/// Fits each feature in order exactly as [`fit_psi_baseline`] does, polling
/// `cancelled` before each feature and inside its fit loops.
///
/// # Errors
/// Returns [`DriftFitError::Cancelled`] once `cancelled` reports true, and
/// otherwise the errors of [`fit_psi_baseline`].
pub(crate) fn fit_psi_baseline_until(
    batch: &arrow::record_batch::RecordBatch,
    profile: &PsiProfile,
    features: &[FeatureName],
    cancelled: &dyn Fn() -> bool,
) -> Result<PsiBaseline, DriftFitError> {
    let mut fitted_features = BTreeMap::new();

    for feature in features {
        ensure_live(cancelled)?;
        let column = resolve_column(batch, feature).map_err(|_| DriftFitError::FeatureMissing {
            feature: feature.as_str().to_string(),
        })?;
        let fitted = if profile.categorical_features.contains(feature) {
            fit_categorical(feature, &column, cancelled)?
        } else {
            fit_numeric(feature, &column, &profile.binning_strategy, cancelled)?
        };
        fitted_features.insert(feature.clone(), fitted);
    }

    Ok(PsiBaseline {
        features: fitted_features,
        wyrd_version: WyrdVersion::current(),
    })
}

/// Score a target `RecordBatch` against a fitted PSI baseline.
///
/// Counts each baseline feature's non-null target values into the fitted bins
/// and scores those counts through [`score_psi_counts`], so raw-batch and
/// server-aggregated inputs share one formula and report construction.
///
/// # Errors
/// Returns [`DriftScoreError`] when a feature is missing, mistyped, empty, or
/// non-finite in the target, or when threshold computation fails.
pub fn score_psi(
    baseline: &PsiBaseline,
    target: &arrow::record_batch::RecordBatch,
    profile: &PsiProfile,
) -> Result<DriftReport, DriftScoreError> {
    let mut counts = BTreeMap::new();
    for (feature_name, fitted) in &baseline.features {
        let column = resolve_column(target, feature_name).map_err(|_| {
            DriftScoreError::FeatureMissingInTarget {
                feature: feature_name.as_str().to_string(),
            }
        })?;
        let target_counts = match fitted.bin_type {
            BinType::Numeric => target_numeric_counts(fitted, &column)?,
            BinType::Categorical => target_categorical_counts(fitted, &column)?,
        };
        counts.insert(feature_name.clone(), target_counts);
    }
    score_psi_counts(baseline, &counts, profile)
}

/// Target observations of one PSI feature, already assigned to fitted bins.
///
/// `bins[i]` counts target values in fitted bin `i`; `total` counts every
/// non-null target value, including categories absent from the baseline,
/// which join no bin but still dilute every bin's proportion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PsiTargetCounts {
    /// Count per fitted bin, in fitted bin order.
    pub bins: Vec<u64>,
    /// Non-null target values, including unmatched categories.
    pub total: u64,
}

/// Score per-feature target bin counts against a fitted PSI baseline.
///
/// This is the aggregate-input entry point: the server counts target values
/// into the fitted bins and passes only the counts. A feature below
/// [`PSI_MIN_TARGET_SAMPLE`] values (including zero) is
/// `Inconclusive` with NaN score and threshold; otherwise the PSI of fitted
/// against target proportions is compared with the profile threshold, and
/// equality is `NoDrift`.
///
/// # Errors
/// Returns [`DriftScoreError::PsiInternal`] when a baseline feature has no
/// counts or a count vector does not match its fitted bins, and
/// [`DriftScoreError::ThresholdFailure`] when the threshold cannot be computed.
pub fn score_psi_counts(
    baseline: &PsiBaseline,
    counts: &BTreeMap<FeatureName, PsiTargetCounts>,
    profile: &PsiProfile,
) -> Result<DriftReport, DriftScoreError> {
    let mut feature_reports = BTreeMap::new();
    for (feature_name, fitted) in &baseline.features {
        let target = counts
            .get(feature_name)
            .filter(|target| target.bins.len() == fitted.bins.len())
            .ok_or_else(|| DriftScoreError::PsiInternal {
                message: format!(
                    "feature {} has no target counts matching its fitted bins",
                    feature_name.as_str()
                ),
            })?;
        let report = if target.total < PSI_MIN_TARGET_SAMPLE {
            FeatureDriftReport {
                feature: feature_name.clone(),
                score: f64::NAN,
                threshold: f64::NAN,
                verdict: DriftVerdict::Inconclusive,
            }
        } else {
            let baseline_proportions = fitted
                .bins
                .iter()
                .map(|bin| bin.proportion)
                .collect::<Vec<_>>();
            let target_proportions = target
                .bins
                .iter()
                .map(|count| *count as f64 / target.total as f64)
                .collect::<Vec<_>>();
            let score = psi(&baseline_proportions, &target_proportions);
            let threshold =
                compute_psi_threshold(&profile.threshold, fitted.bins.len(), target.total)?;
            let verdict = if score > threshold {
                DriftVerdict::Drift
            } else {
                DriftVerdict::NoDrift
            };
            FeatureDriftReport {
                feature: feature_name.clone(),
                score,
                threshold,
                verdict,
            }
        };
        feature_reports.insert(feature_name.clone(), report);
    }

    let verdict = DriftReport::aggregate_verdict(&feature_reports);
    Ok(DriftReport {
        method: DriftMethod::Psi,
        features: feature_reports,
        verdict,
    })
}

/// Count a numeric target column into the fitted `(lower, upper]` bins.
fn target_numeric_counts(
    fitted: &FittedPsiFeature,
    column: &ColumnRef<'_>,
) -> Result<PsiTargetCounts, DriftScoreError> {
    if !column.is_numeric() {
        return Err(DriftScoreError::FeatureTypeMismatch {
            feature: fitted.feature.as_str().to_string(),
        });
    }

    let values =
        column
            .collect_f64_non_null()
            .map_err(|_| DriftScoreError::FeatureTypeMismatch {
                feature: fitted.feature.as_str().to_string(),
            })?;
    let total = values.len() as u64;
    if total == 0 {
        return Err(DriftScoreError::FeatureEmpty {
            feature: fitted.feature.as_str().to_string(),
        });
    }

    let edges = fitted.numeric_edges()?;
    let mut bins = vec![0_u64; fitted.bins.len()];
    for value in &values {
        if !value.is_finite() {
            return Err(DriftScoreError::PsiInternal {
                message: "non-finite value in target column".to_string(),
            });
        }
        bins[assign_bin(*value, &edges)] += 1;
    }
    Ok(PsiTargetCounts { bins, total })
}

/// Count a categorical target column into the fitted category bins.
fn target_categorical_counts(
    fitted: &FittedPsiFeature,
    column: &ColumnRef<'_>,
) -> Result<PsiTargetCounts, DriftScoreError> {
    if !is_categorical_dtype(column.array.data_type()) {
        return Err(DriftScoreError::FeatureTypeMismatch {
            feature: fitted.feature.as_str().to_string(),
        });
    }

    let values =
        column
            .collect_string_non_null()
            .map_err(|_| DriftScoreError::FeatureTypeMismatch {
                feature: fitted.feature.as_str().to_string(),
            })?;
    let total = values.len() as u64;
    if total == 0 {
        return Err(DriftScoreError::FeatureEmpty {
            feature: fitted.feature.as_str().to_string(),
        });
    }

    let mut bins = vec![0_u64; fitted.bins.len()];
    let mut index_by_category = HashMap::with_capacity(fitted.bins.len());
    for (index, bin) in fitted.bins.iter().enumerate() {
        if let Some(category) = &bin.categorical_value {
            index_by_category.insert(category.as_str(), index);
        }
    }
    for value in &values {
        if let Some(index) = index_by_category.get(value.as_str()) {
            bins[*index] += 1;
        }
    }
    Ok(PsiTargetCounts { bins, total })
}

impl FittedPsiFeature {
    /// Ordered numeric bin edges, from the first bin's lower edge through
    /// every bin's upper edge; `(edges[i], edges[i + 1]]` is bin `i`.
    ///
    /// # Errors
    /// Returns [`DriftScoreError::PsiInternal`] when the feature has no bins
    /// or a numeric bin lacks an edge.
    pub fn numeric_edges(&self) -> Result<Vec<f64>, DriftScoreError> {
        let mut edges = Vec::with_capacity(self.bins.len() + 1);
        let first = self
            .bins
            .first()
            .ok_or_else(|| DriftScoreError::PsiInternal {
                message: format!("feature {} has no fitted bins", self.feature.as_str()),
            })?;
        edges.push(first.lower.ok_or_else(|| DriftScoreError::PsiInternal {
            message: format!(
                "numeric feature {} has a first bin without lower edge",
                self.feature.as_str()
            ),
        })?);

        for bin in &self.bins {
            edges.push(bin.upper.ok_or_else(|| DriftScoreError::PsiInternal {
                message: format!(
                    "numeric feature {} has a bin without upper edge",
                    self.feature.as_str()
                ),
            })?);
        }

        Ok(edges)
    }
}

fn is_categorical_dtype(data_type: &DataType) -> bool {
    match data_type {
        DataType::Utf8 | DataType::LargeUtf8 => true,
        DataType::Dictionary(_, value_type) => is_categorical_dtype(value_type),
        _ => false,
    }
}

/// Fit one numeric feature's bin edges and baseline proportions.
///
/// Polls `cancelled` after collecting values, after computing edges, and
/// every [`CANCEL_CHECK_ROWS`] values while binning.
///
/// # Errors
/// Returns [`DriftFitError`] for a non-numeric, empty, or non-finite column,
/// an edge computation failure, or cancellation.
fn fit_numeric(
    feature: &FeatureName,
    column: &ColumnRef<'_>,
    strategy: &PsiBinningStrategy,
    cancelled: &dyn Fn() -> bool,
) -> Result<FittedPsiFeature, DriftFitError> {
    if !column.is_numeric() {
        return Err(DriftFitError::FeatureNotNumeric {
            feature: column.name.to_string(),
            arrow_type: column.data_type_string(),
        });
    }

    let values = column
        .collect_f64_non_null()
        .map_err(|_| DriftFitError::FeatureNotNumeric {
            feature: column.name.to_string(),
            arrow_type: column.data_type_string(),
        })?;
    if values.is_empty() {
        return Err(DriftFitError::FeatureEmpty {
            feature: column.name.to_string(),
        });
    }
    if values.iter().any(|value| !value.is_finite()) {
        return Err(DriftFitError::NonFiniteValuesInColumn {
            feature: column.name.to_string(),
        });
    }

    ensure_live(cancelled)?;
    let edges = match strategy {
        PsiBinningStrategy::EqualWidth { n_bins } => compute_edges_equal_width(&values, *n_bins)?,
        PsiBinningStrategy::Quantile { n_bins } => compute_edges_quantile(&values, *n_bins)?,
    };

    let mut counts = vec![0_u64; edges.edges.len() - 1];
    for chunk in values.chunks(CANCEL_CHECK_ROWS) {
        ensure_live(cancelled)?;
        for value in chunk {
            counts[assign_bin(*value, &edges.edges)] += 1;
        }
    }

    let total_count = values.len() as u64;
    let mut bins = Vec::with_capacity(counts.len());
    for (index, count) in counts.into_iter().enumerate() {
        bins.push(Bin {
            id: bin_id(index)?,
            lower: Some(edges.edges[index]),
            upper: Some(edges.edges[index + 1]),
            categorical_value: None,
            proportion: count as f64 / total_count as f64,
        });
    }

    Ok(FittedPsiFeature {
        feature: feature.clone(),
        bin_type: BinType::Numeric,
        bins,
        total_count,
    })
}

/// Fit one categorical feature's observed categories and proportions.
///
/// Polls `cancelled` every [`CANCEL_CHECK_ROWS`] values while counting.
///
/// # Errors
/// Returns [`DriftFitError`] for a non-categorical or empty column, a bin id
/// overflow, or cancellation.
fn fit_categorical(
    feature: &FeatureName,
    column: &ColumnRef<'_>,
    cancelled: &dyn Fn() -> bool,
) -> Result<FittedPsiFeature, DriftFitError> {
    let values =
        column
            .collect_string_non_null()
            .map_err(|_| DriftFitError::FeatureNotCategorical {
                feature: column.name.to_string(),
                arrow_type: column.data_type_string(),
            })?;
    if values.is_empty() {
        return Err(DriftFitError::FeatureEmpty {
            feature: column.name.to_string(),
        });
    }

    let mut counts: BTreeMap<&str, u64> = BTreeMap::new();
    for chunk in values.chunks(CANCEL_CHECK_ROWS) {
        ensure_live(cancelled)?;
        for value in chunk {
            *counts.entry(value.as_str()).or_default() += 1;
        }
    }

    let total_count = counts.values().sum::<u64>();
    let mut bins = Vec::with_capacity(counts.len());
    for (index, (value, count)) in counts.into_iter().enumerate() {
        bins.push(Bin {
            id: bin_id(index)?,
            lower: None,
            upper: None,
            categorical_value: Some(value.to_owned()),
            proportion: count as f64 / total_count as f64,
        });
    }

    Ok(FittedPsiFeature {
        feature: feature.clone(),
        bin_type: BinType::Categorical,
        bins,
        total_count,
    })
}

fn bin_id(index: usize) -> Result<i32, DriftFitError> {
    i32::try_from(index).map_err(|_| DriftFitError::PsiInternal {
        message: format!("bin index {index} exceeds i32 range"),
    })
}

#[cfg(test)]
mod psi_fit {
    use std::sync::Arc;

    use crate::DriftFitError;
    use crate::psi::{BinType, fit_psi_baseline};
    use arrow::array::{Float64Array, Int64Array, StringArray};
    use arrow::datatypes::{DataType, Field, Schema};
    use arrow::record_batch::RecordBatch;
    use wyrd_spec::card::drift::{PsiBinningStrategy, PsiProfile, PsiThreshold};
    use wyrd_spec::ids::FeatureName;

    fn feature(name: &str) -> FeatureName {
        FeatureName::new(name).expect("valid feature name")
    }

    fn fixed_profile(binning_strategy: PsiBinningStrategy) -> PsiProfile {
        profile_with_categorical(binning_strategy, Vec::new())
    }

    fn profile_with_categorical(
        binning_strategy: PsiBinningStrategy,
        categorical_features: Vec<FeatureName>,
    ) -> PsiProfile {
        PsiProfile {
            binning_strategy,
            categorical_features,
            threshold: PsiThreshold::Fixed { value: 0.25 },
        }
    }

    fn numeric_batch(name: &str, values: Vec<Option<f64>>) -> RecordBatch {
        RecordBatch::try_new(
            Arc::new(Schema::new(vec![Field::new(name, DataType::Float64, true)])),
            vec![Arc::new(Float64Array::from(values))],
        )
        .expect("record batch")
    }

    fn int_batch(name: &str, values: Vec<i64>) -> RecordBatch {
        RecordBatch::try_new(
            Arc::new(Schema::new(vec![Field::new(name, DataType::Int64, false)])),
            vec![Arc::new(Int64Array::from(values))],
        )
        .expect("record batch")
    }

    #[test]
    fn equal_width_numeric_fit_computes_bin_proportions() {
        let values = (0..100).map(|value| Some(value as f64)).collect();
        let batch = numeric_batch("score", values);
        let profile = fixed_profile(PsiBinningStrategy::EqualWidth { n_bins: 10 });
        let baseline = fit_psi_baseline(&batch, &profile, &[feature("score")]).expect("baseline");
        let fitted = baseline.features.get(&feature("score")).expect("feature");

        assert_eq!(fitted.bin_type, BinType::Numeric);
        assert_eq!(fitted.total_count, 100);
        assert_eq!(fitted.bins.len(), 10);
        assert_eq!(fitted.bins[0].lower, Some(f64::NEG_INFINITY));
        assert!((fitted.bins[0].upper.expect("upper") - 9.9).abs() < 1e-6);
        assert_eq!(fitted.bins[9].upper, Some(f64::INFINITY));
        for bin in &fitted.bins {
            assert!((bin.proportion - 0.1).abs() < 1e-12);
        }
    }

    #[test]
    fn all_equal_numeric_fit_uses_single_infinite_bin() {
        let batch = numeric_batch("score", vec![Some(5.0); 12]);
        let profile = fixed_profile(PsiBinningStrategy::EqualWidth { n_bins: 10 });
        let baseline = fit_psi_baseline(&batch, &profile, &[feature("score")]).expect("baseline");
        let fitted = baseline.features.get(&feature("score")).expect("feature");

        assert_eq!(fitted.bins.len(), 1);
        assert_eq!(fitted.bins[0].lower, Some(f64::NEG_INFINITY));
        assert_eq!(fitted.bins[0].upper, Some(f64::INFINITY));
        assert_eq!(fitted.bins[0].proportion, 1.0);
    }

    #[test]
    fn categorical_fit_sorts_bins_and_computes_proportions() {
        let batch = RecordBatch::try_new(
            Arc::new(Schema::new(vec![Field::new("kind", DataType::Utf8, true)])),
            vec![Arc::new(StringArray::from(vec![
                Some("beta"),
                Some("alpha"),
                None,
                Some("alpha"),
                Some("gamma"),
            ]))],
        )
        .expect("record batch");
        let profile = profile_with_categorical(
            PsiBinningStrategy::EqualWidth { n_bins: 10 },
            vec![feature("kind")],
        );
        let baseline = fit_psi_baseline(&batch, &profile, &[feature("kind")]).expect("baseline");
        let fitted = baseline.features.get(&feature("kind")).expect("feature");

        assert_eq!(fitted.bin_type, BinType::Categorical);
        assert_eq!(fitted.total_count, 4);
        let values: Vec<&str> = fitted
            .bins
            .iter()
            .map(|bin| bin.categorical_value.as_deref().expect("category"))
            .collect();
        assert_eq!(values, vec!["alpha", "beta", "gamma"]);
        assert!((fitted.bins[0].proportion - 0.5).abs() < 1e-12);
        assert!((fitted.bins[1].proportion - 0.25).abs() < 1e-12);
        assert!((fitted.bins[2].proportion - 0.25).abs() < 1e-12);
    }

    #[test]
    fn integer_numeric_fit_casts_to_float_values() {
        let batch = int_batch("score", (0..100).collect());
        let profile = fixed_profile(PsiBinningStrategy::EqualWidth { n_bins: 5 });
        let baseline = fit_psi_baseline(&batch, &profile, &[feature("score")]).expect("baseline");
        let fitted = baseline.features.get(&feature("score")).expect("feature");

        assert_eq!(fitted.bin_type, BinType::Numeric);
        assert_eq!(fitted.bins.len(), 5);
        assert_eq!(fitted.total_count, 100);
    }

    #[test]
    fn missing_feature_returns_typed_error() {
        let batch = numeric_batch("score", vec![Some(1.0), Some(2.0)]);
        let profile = fixed_profile(PsiBinningStrategy::EqualWidth { n_bins: 2 });
        let err = fit_psi_baseline(&batch, &profile, &[feature("missing")]).expect_err("fit error");

        assert!(matches!(
            err,
            DriftFitError::FeatureMissing { feature } if feature == "missing"
        ));
    }

    #[test]
    fn string_feature_not_declared_categorical_returns_numeric_error() {
        let batch = RecordBatch::try_new(
            Arc::new(Schema::new(vec![Field::new("kind", DataType::Utf8, false)])),
            vec![Arc::new(StringArray::from(vec!["alpha", "beta"]))],
        )
        .expect("record batch");
        let profile = fixed_profile(PsiBinningStrategy::EqualWidth { n_bins: 2 });
        let err = fit_psi_baseline(&batch, &profile, &[feature("kind")]).expect_err("fit error");

        assert!(matches!(
            err,
            DriftFitError::FeatureNotNumeric { feature, .. } if feature == "kind"
        ));
    }

    #[test]
    fn empty_numeric_column_returns_typed_error() {
        let batch = numeric_batch("score", Vec::new());
        let profile = fixed_profile(PsiBinningStrategy::EqualWidth { n_bins: 2 });
        let err = fit_psi_baseline(&batch, &profile, &[feature("score")]).expect_err("fit error");

        assert!(matches!(
            err,
            DriftFitError::FeatureEmpty { feature } if feature == "score"
        ));
    }

    #[test]
    fn numeric_fit_rejects_non_finite_values() {
        let batch = numeric_batch("score", vec![Some(1.0), Some(f64::NAN), Some(3.0)]);
        let profile = fixed_profile(PsiBinningStrategy::EqualWidth { n_bins: 3 });
        let err = fit_psi_baseline(&batch, &profile, &[feature("score")]).expect_err("fit error");

        assert!(matches!(
            err,
            DriftFitError::NonFiniteValuesInColumn { feature } if feature == "score"
        ));
    }
}

#[cfg(test)]
mod psi_score {
    use std::sync::Arc;

    use crate::{DriftScoreError, DriftVerdict, fit_psi_baseline, score_psi};
    use arrow::array::{Float64Array, Int64Array, StringArray};
    use arrow::record_batch::RecordBatch;
    use arrow_schema::{DataType, Field, Schema};
    use wyrd_spec::card::drift::{PsiBinningStrategy, PsiProfile, PsiThreshold};
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
            Arc::new(Schema::new(vec![Field::new(name, DataType::Int64, false)])),
            vec![Arc::new(Int64Array::from(values))],
        )
        .expect("record batch")
    }

    fn cat_batch(name: &str, values: Vec<&str>) -> RecordBatch {
        RecordBatch::try_new(
            Arc::new(Schema::new(vec![Field::new(name, DataType::Utf8, true)])),
            vec![Arc::new(StringArray::from(values))],
        )
        .expect("record batch")
    }

    fn psi_profile_default() -> PsiProfile {
        PsiProfile {
            binning_strategy: PsiBinningStrategy::EqualWidth { n_bins: 10 },
            categorical_features: Vec::new(),
            threshold: PsiThreshold::ChiSquare { alpha: 0.05 },
        }
    }

    #[test]
    fn psi_identical_distributions_no_drift() {
        let baseline_batch = numeric_batch("x", (0..1000).map(|value| value as f64).collect());
        let target_batch = numeric_batch("x", (0..1000).map(|value| value as f64).collect());
        let profile = psi_profile_default();
        let fname = feature("x");
        let baseline = fit_psi_baseline(&baseline_batch, &profile, std::slice::from_ref(&fname))
            .expect("baseline");

        let report = score_psi(&baseline, &target_batch, &profile).expect("report");

        assert_eq!(report.verdict, DriftVerdict::NoDrift);
        let feature_report = report.features.get(&fname).expect("feature report");
        assert!(feature_report.score < 0.01);
        assert!(feature_report.threshold.is_finite());
    }

    #[test]
    fn psi_shifted_distribution_drift() {
        let baseline_batch = numeric_batch("x", (0..1000).map(|value| value as f64).collect());
        let target_batch = numeric_batch("x", (1000..2000).map(|value| value as f64).collect());
        let profile = psi_profile_default();
        let fname = feature("x");
        let baseline = fit_psi_baseline(&baseline_batch, &profile, &[fname]).expect("baseline");

        let report = score_psi(&baseline, &target_batch, &profile).expect("report");

        assert_eq!(report.verdict, DriftVerdict::Drift);
    }

    #[test]
    fn psi_target_too_small_is_inconclusive_with_nan_score_and_threshold() {
        let baseline_batch = numeric_batch("x", (0..1000).map(|value| value as f64).collect());
        let target_batch = numeric_batch("x", (0..50).map(|value| value as f64).collect());
        let profile = psi_profile_default();
        let fname = feature("x");
        let baseline = fit_psi_baseline(&baseline_batch, &profile, std::slice::from_ref(&fname))
            .expect("baseline");

        let report = score_psi(&baseline, &target_batch, &profile).expect("report");

        assert_eq!(report.verdict, DriftVerdict::Inconclusive);
        let feature_report = report.features.get(&fname).expect("feature report");
        assert_eq!(feature_report.verdict, DriftVerdict::Inconclusive);
        assert!(feature_report.score.is_nan());
        assert!(feature_report.threshold.is_nan());
    }

    #[test]
    fn psi_missing_target_feature_errors() {
        let baseline_batch = numeric_batch("x", (0..1000).map(|value| value as f64).collect());
        let target_batch = numeric_batch("not_x", vec![1.0, 2.0, 3.0]);
        let profile = psi_profile_default();
        let fname = feature("x");
        let baseline = fit_psi_baseline(&baseline_batch, &profile, &[fname]).expect("baseline");

        let err = score_psi(&baseline, &target_batch, &profile).expect_err("score error");

        assert!(matches!(
            err,
            DriftScoreError::FeatureMissingInTarget { feature } if feature == "x"
        ));
    }

    #[test]
    fn psi_categorical_drift() {
        let baseline_batch = cat_batch(
            "city",
            std::iter::repeat_n("sea", 500)
                .chain(std::iter::repeat_n("pdx", 500))
                .collect(),
        );
        let target_batch = cat_batch(
            "city",
            std::iter::repeat_n("sea", 100)
                .chain(std::iter::repeat_n("pdx", 900))
                .collect(),
        );
        let fname = feature("city");
        let profile = PsiProfile {
            binning_strategy: PsiBinningStrategy::EqualWidth { n_bins: 2 },
            categorical_features: vec![fname.clone()],
            threshold: PsiThreshold::Fixed { value: 0.1 },
        };
        let baseline = fit_psi_baseline(&baseline_batch, &profile, &[fname]).expect("baseline");

        let report = score_psi(&baseline, &target_batch, &profile).expect("report");

        assert_eq!(report.verdict, DriftVerdict::Drift);
    }

    #[test]
    fn psi_new_categorical_values_count_in_denominator_only() {
        let baseline_batch = cat_batch(
            "city",
            std::iter::repeat_n("sea", 500)
                .chain(std::iter::repeat_n("pdx", 500))
                .collect(),
        );
        let target_batch = cat_batch(
            "city",
            std::iter::repeat_n("sea", 500)
                .chain(std::iter::repeat_n("pdx", 400))
                .chain(std::iter::repeat_n("new", 100))
                .collect(),
        );
        let fname = feature("city");
        let profile = PsiProfile {
            binning_strategy: PsiBinningStrategy::EqualWidth { n_bins: 2 },
            categorical_features: vec![fname.clone()],
            threshold: PsiThreshold::Fixed { value: 100.0 },
        };
        let baseline = fit_psi_baseline(&baseline_batch, &profile, std::slice::from_ref(&fname))
            .expect("baseline");

        let report = score_psi(&baseline, &target_batch, &profile).expect("report");

        let feature_report = report.features.get(&fname).expect("feature report");
        // Inline PSI formula with epsilon=1e-10 for the two-bin case: baseline=[0.5,0.5], target=[0.5,0.4]
        const EPS: f64 = 1e-10;
        let expected: f64 = [(0.5_f64, 0.5_f64), (0.5, 0.4)]
            .iter()
            .map(|(p, q)| {
                let pe = p + EPS;
                let qe = q + EPS;
                (pe - qe) * (pe / qe).ln()
            })
            .sum();
        assert!((feature_report.score - expected).abs() < 1e-12);
    }

    #[test]
    fn psi_fixed_threshold_is_used() {
        let baseline_batch = numeric_batch("x", (0..1000).map(|value| value as f64).collect());
        let target_batch = numeric_batch("x", (500..1500).map(|value| value as f64).collect());
        let fname = feature("x");
        let profile_low = PsiProfile {
            binning_strategy: PsiBinningStrategy::EqualWidth { n_bins: 10 },
            categorical_features: Vec::new(),
            threshold: PsiThreshold::Fixed { value: 0.0001 },
        };
        let baseline = fit_psi_baseline(&baseline_batch, &profile_low, &[fname]).expect("baseline");

        let report_low = score_psi(&baseline, &target_batch, &profile_low).expect("report");

        assert_eq!(report_low.verdict, DriftVerdict::Drift);

        let profile_high = PsiProfile {
            binning_strategy: PsiBinningStrategy::EqualWidth { n_bins: 10 },
            categorical_features: Vec::new(),
            threshold: PsiThreshold::Fixed { value: 100.0 },
        };

        let report_high = score_psi(&baseline, &target_batch, &profile_high).expect("report");

        assert_eq!(report_high.verdict, DriftVerdict::NoDrift);
    }

    #[test]
    fn psi_numeric_target_type_mismatch_errors() {
        let baseline_batch = numeric_batch("x", (0..1000).map(|value| value as f64).collect());
        let target_batch = cat_batch("x", vec!["1", "2", "3"]);
        let profile = psi_profile_default();
        let fname = feature("x");
        let baseline = fit_psi_baseline(&baseline_batch, &profile, &[fname]).expect("baseline");

        let err = score_psi(&baseline, &target_batch, &profile).expect_err("score error");

        assert!(matches!(
            err,
            DriftScoreError::FeatureTypeMismatch { feature } if feature == "x"
        ));
    }

    #[test]
    fn psi_categorical_target_type_mismatch_errors() {
        let baseline_batch = cat_batch(
            "city",
            std::iter::repeat_n("sea", 500)
                .chain(std::iter::repeat_n("pdx", 500))
                .collect(),
        );
        let target_batch = int_batch("city", (0..1000).collect());
        let fname = feature("city");
        let profile = PsiProfile {
            binning_strategy: PsiBinningStrategy::EqualWidth { n_bins: 2 },
            categorical_features: vec![fname.clone()],
            threshold: PsiThreshold::Fixed { value: 0.1 },
        };
        let baseline = fit_psi_baseline(&baseline_batch, &profile, &[fname]).expect("baseline");

        let err = score_psi(&baseline, &target_batch, &profile).expect_err("score error");

        assert!(matches!(
            err,
            DriftScoreError::FeatureTypeMismatch { feature } if feature == "city"
        ));
    }
}
