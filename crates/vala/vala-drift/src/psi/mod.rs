//! PSI baseline fit and target scoring.
//!
//! A fit freezes an exhaustive set of bins per feature: numeric bins cover the
//! whole real line through open outer edges, and a categorical fit holds one
//! bin per baseline label plus one reserved `other` bin with zero baseline
//! mass. Every scored target value therefore lands in exactly one bin, so the
//! baseline and target proportions each sum to one before the smoothed PSI is
//! compared with the authored threshold. PSI measures distribution shift; it is
//! not a significance test and does not prove model degradation.

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

use crate::baseline::{CANCEL_CHECK_ROWS, FITTED_FORMAT, ensure_live};
use crate::error::{DriftFitError, DriftScoreError};
use crate::feature::{ColumnRef, TargetColumn, required_values, resolve_column, target_complete};
use crate::psi::binning::{assign_bin, compute_edges_equal_width, compute_edges_quantile};
pub use crate::psi::score::PSI_MIN_TARGET_SAMPLE;
use crate::psi::score::psi;
use crate::psi::threshold::compute_psi_threshold;
use crate::report::{DriftReport, DriftVerdict, FeatureDriftReport, FeatureEvidence};

/// PSI fitted baseline, one entry per feature.
///
/// Serializable so the server's baseline store can persist it as the fitted
/// profile of one Verifier version.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PsiBaseline {
    /// Fitted state per feature.
    pub features: BTreeMap<FeatureName, FittedPsiFeature>,
    /// Fitted-profile format; [`FITTED_FORMAT`] marks exhaustive bins.
    pub format: u32,
    /// Vala version at fit time; used by the persistence layer for forward-compatibility checks.
    pub wyrd_version: WyrdVersion,
}

/// Per-feature fitted PSI state.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FittedPsiFeature {
    /// Feature this state belongs to.
    pub feature: FeatureName,
    /// Whether the bins are numeric intervals or categorical labels.
    pub bin_type: BinType,
    /// The frozen, exhaustive bins in scoring order.
    pub bins: Vec<Bin>,
    /// Baseline values the proportions were computed from.
    pub total_count: u64,
}

/// Kind of a fitted feature's bins.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BinType {
    /// `(lower, upper]` intervals over the real line.
    Numeric,
    /// One bin per baseline label plus the reserved `other` bin.
    Categorical,
}

/// One bin.
///
/// Numeric edges may be infinite (the outer bins are open), which JSON cannot
/// represent as a number, so edges serialize through [`edge`]. A categorical
/// bin names its label; the reserved `other` bin is the categorical bin with
/// no label, so it never collides with an authored label spelled `other`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Bin {
    /// Position of the bin in scoring order.
    pub id: i32,
    /// Exclusive lower edge of a numeric bin.
    #[serde(with = "edge")]
    pub lower: Option<f64>,
    /// Inclusive upper edge of a numeric bin.
    #[serde(with = "edge")]
    pub upper: Option<f64>,
    /// Label of a categorical bin; `None` on the reserved `other` bin.
    pub categorical_value: Option<String>,
    /// Baseline proportion of the bin.
    pub proportion: f64,
}

/// The per-bin comparison behind one scored PSI feature.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PsiEvidence {
    /// Target values compared; every one landed in exactly one bin.
    pub sample: u64,
    /// Every fitted bin with its target count, in scoring order.
    pub bins: Vec<PsiBinEvidence>,
}

/// One fitted bin with its target count and proportion.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PsiBinEvidence {
    /// The frozen bin, including its baseline proportion.
    pub bin: Bin,
    /// Target values in the bin.
    pub target_count: u64,
    /// `target_count / sample`.
    pub target_proportion: f64,
}

/// Fit a PSI baseline for `features` of `batch` under `profile`, uncancelled.
///
/// The ordinary entry point for callers with no cancellation: it runs the same
/// per-feature fit as the cancellable fitter with a check that never fires,
/// so its output is identical. Categorical features in `profile` get one bin
/// per observed value plus the reserved `other` bin; every other feature is
/// binned numerically by the profile's binning strategy.
///
/// # Errors
/// Returns [`DriftFitError::FeatureMissing`] when a feature is not a column
/// of `batch`; [`DriftFitError::FeatureNotNumeric`] or
/// [`DriftFitError::FeatureNotCategorical`] when a column has the wrong Arrow
/// type; [`DriftFitError::FeatureEmpty`] when a column has no rows;
/// [`DriftFitError::NullValuesInColumn`] when a column holds a null;
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
        format: FITTED_FORMAT,
        wyrd_version: WyrdVersion::current(),
    })
}

/// Score a target `RecordBatch` against a fitted PSI baseline.
///
/// `target` is an already selected batch: each row is one relevant
/// observation and the caller has excluded unrelated ones. When any row
/// misses a feature or holds a null or non-finite value, the target is
/// [`DriftReport::unscored`]; nothing is dropped or imputed. Otherwise every value is counted into its fitted
/// bin (an unseen category into `other`) and scored through
/// [`score_psi_counts`], so raw-batch and server-aggregated inputs share one
/// formula and report construction.
///
/// # Errors
/// Returns [`DriftScoreError::FeatureTypeMismatch`] when a target column has
/// the wrong type for its fitted bins, and the errors of [`score_psi_counts`].
pub fn score_psi(
    baseline: &PsiBaseline,
    target: &arrow::record_batch::RecordBatch,
    profile: &PsiProfile,
) -> Result<DriftReport, DriftScoreError> {
    let mut columns = Vec::with_capacity(baseline.features.len());
    for (feature_name, fitted) in &baseline.features {
        columns.push(target_column(fitted, target, feature_name)?);
    }
    if !target_complete(target.num_rows(), &columns.iter().collect::<Vec<_>>()) {
        return Ok(DriftReport::unscored(DriftMethod::Psi));
    }
    let mut counts = BTreeMap::new();
    for ((feature_name, fitted), column) in baseline.features.iter().zip(&columns) {
        counts.insert(feature_name.clone(), fitted.count(column)?);
    }
    score_psi_counts(baseline, &counts, profile)
}

/// Read `feature`'s target column with the type its fitted bins require.
///
/// # Errors
/// Returns [`DriftScoreError::FeatureTypeMismatch`] for a numeric feature
/// whose column is not numeric or a categorical one whose column is not text.
fn target_column(
    fitted: &FittedPsiFeature,
    target: &arrow::record_batch::RecordBatch,
    feature: &FeatureName,
) -> Result<TargetColumn, DriftScoreError> {
    let mismatch = || DriftScoreError::FeatureTypeMismatch {
        feature: feature.as_str().to_string(),
    };
    let Ok(column) = resolve_column(target, feature) else {
        return Ok(TargetColumn::Absent);
    };
    match fitted.bin_type {
        BinType::Numeric if column.is_numeric() => column
            .collect_f64()
            .map(TargetColumn::Numeric)
            .map_err(|_| mismatch()),
        BinType::Categorical if is_categorical_dtype(column.array.data_type()) => column
            .collect_string()
            .map(TargetColumn::Categorical)
            .map_err(|_| mismatch()),
        BinType::Numeric | BinType::Categorical => Err(mismatch()),
    }
}

/// Score per-feature target bin counts against a fitted PSI baseline.
///
/// This is the aggregate-input entry point: the server counts target values
/// into the fitted bins and passes only the counts. `counts[f][i]` is the
/// number of target values in fitted bin `i`, and the feature's sample is
/// their sum. A feature below [`PSI_MIN_TARGET_SAMPLE`] values (including
/// zero) is `Inconclusive` with NaN score and threshold; otherwise the PSI of
/// baseline against target proportions is compared with the profile threshold
/// over every bin, `other` included, equality is `NoDrift`, and the feature
/// carries [`PsiEvidence`].
///
/// # Errors
/// Returns [`DriftScoreError::PsiInternal`] when a baseline feature has no
/// counts or a count vector does not match its fitted bins, and
/// [`DriftScoreError::ThresholdFailure`] when the threshold cannot be computed.
pub fn score_psi_counts(
    baseline: &PsiBaseline,
    counts: &BTreeMap<FeatureName, Vec<u64>>,
    profile: &PsiProfile,
) -> Result<DriftReport, DriftScoreError> {
    let mut feature_reports = BTreeMap::new();
    for (feature_name, fitted) in &baseline.features {
        let target = counts
            .get(feature_name)
            .filter(|target| target.len() == fitted.bins.len())
            .ok_or_else(|| DriftScoreError::PsiInternal {
                message: format!(
                    "feature {} has no target counts matching its fitted bins",
                    feature_name.as_str()
                ),
            })?;
        let sample = target.iter().sum::<u64>();
        let report = if sample < PSI_MIN_TARGET_SAMPLE {
            FeatureDriftReport {
                feature: feature_name.clone(),
                score: f64::NAN,
                threshold: f64::NAN,
                verdict: DriftVerdict::Inconclusive,
                evidence: None,
            }
        } else {
            let baseline_proportions = fitted
                .bins
                .iter()
                .map(|bin| bin.proportion)
                .collect::<Vec<_>>();
            let target_proportions = target
                .iter()
                .map(|count| *count as f64 / sample as f64)
                .collect::<Vec<_>>();
            let score = psi(&baseline_proportions, &target_proportions);
            let threshold = compute_psi_threshold(&profile.threshold, fitted.bins.len(), sample)?;
            let verdict = if score > threshold {
                DriftVerdict::Drift
            } else {
                DriftVerdict::NoDrift
            };
            let bins = fitted
                .bins
                .iter()
                .zip(target.iter().zip(target_proportions))
                .map(|(bin, (count, proportion))| PsiBinEvidence {
                    bin: bin.clone(),
                    target_count: *count,
                    target_proportion: proportion,
                })
                .collect();
            FeatureDriftReport {
                feature: feature_name.clone(),
                score,
                threshold,
                verdict,
                evidence: Some(FeatureEvidence::Psi(PsiEvidence { sample, bins })),
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

    /// The fitted labels in bin order and the index of the reserved `other`
    /// bin that receives every unseen category.
    ///
    /// # Errors
    /// Returns [`DriftScoreError::PsiInternal`] when the feature is not
    /// categorical or its last bin is not the unlabeled `other` bin.
    pub fn categorical_labels(&self) -> Result<(Vec<&str>, usize), DriftScoreError> {
        let internal = || DriftScoreError::PsiInternal {
            message: format!(
                "feature {} has no reserved other bin",
                self.feature.as_str()
            ),
        };
        let (other, labeled) = self.bins.split_last().ok_or_else(internal)?;
        if self.bin_type != BinType::Categorical || other.categorical_value.is_some() {
            return Err(internal());
        }
        let labels = labeled
            .iter()
            .map(|bin| bin.categorical_value.as_deref().ok_or_else(internal))
            .collect::<Result<Vec<_>, _>>()?;
        Ok((labels, labeled.len()))
    }

    /// Count a complete target column into the fitted bins.
    ///
    /// Every carried value lands in exactly one bin; an unseen category lands
    /// in `other`. An absent column counts nothing.
    ///
    /// # Errors
    /// Returns [`DriftScoreError::PsiInternal`] when the fitted bins are
    /// malformed or `column` does not match the bin type.
    fn count(&self, column: &TargetColumn) -> Result<Vec<u64>, DriftScoreError> {
        let mut bins = vec![0_u64; self.bins.len()];
        match (self.bin_type, column) {
            (_, TargetColumn::Absent) => {}
            (BinType::Numeric, TargetColumn::Numeric(_)) => {
                let edges = self.numeric_edges()?;
                for value in column.numeric_values() {
                    bins[assign_bin(value, &edges)] += 1;
                }
            }
            (BinType::Categorical, TargetColumn::Categorical(values)) => {
                let (labels, other) = self.categorical_labels()?;
                let index_by_label: HashMap<&str, usize> = labels
                    .into_iter()
                    .enumerate()
                    .map(|(index, label)| (label, index))
                    .collect();
                for value in values.iter().flatten() {
                    bins[index_by_label.get(value.as_str()).copied().unwrap_or(other)] += 1;
                }
            }
            (BinType::Numeric | BinType::Categorical, _) => {
                return Err(DriftScoreError::PsiInternal {
                    message: format!(
                        "feature {} target does not match its bins",
                        self.feature.as_str()
                    ),
                });
            }
        }
        Ok(bins)
    }
}

/// Whether `data_type` is a text or text-dictionary column.
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
/// Returns [`DriftFitError`] for a non-numeric, empty, null-bearing, or
/// non-finite column, an edge computation failure, or cancellation.
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
        .collect_f64()
        .map_err(|_| DriftFitError::FeatureNotNumeric {
            feature: column.name.to_string(),
            arrow_type: column.data_type_string(),
        })?;
    let values = required_values(column, values)?;
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

/// Fit one categorical feature's observed labels, proportions, and `other` bin.
///
/// Labels are sorted; the reserved `other` bin follows them with zero
/// baseline mass. Polls `cancelled` every [`CANCEL_CHECK_ROWS`] values while
/// counting.
///
/// # Errors
/// Returns [`DriftFitError`] for a non-categorical, empty, or null-bearing
/// column, a bin id overflow, or cancellation.
fn fit_categorical(
    feature: &FeatureName,
    column: &ColumnRef<'_>,
    cancelled: &dyn Fn() -> bool,
) -> Result<FittedPsiFeature, DriftFitError> {
    let values = column
        .collect_string()
        .map_err(|_| DriftFitError::FeatureNotCategorical {
            feature: column.name.to_string(),
            arrow_type: column.data_type_string(),
        })?;
    let values = required_values(column, values)?;

    let mut counts: BTreeMap<&str, u64> = BTreeMap::new();
    for chunk in values.chunks(CANCEL_CHECK_ROWS) {
        ensure_live(cancelled)?;
        for value in chunk {
            *counts.entry(value.as_str()).or_default() += 1;
        }
    }

    let total_count = values.len() as u64;
    let mut bins = Vec::with_capacity(counts.len() + 1);
    for (index, (value, count)) in counts.into_iter().enumerate() {
        bins.push(Bin {
            id: bin_id(index)?,
            lower: None,
            upper: None,
            categorical_value: Some(value.to_owned()),
            proportion: count as f64 / total_count as f64,
        });
    }
    bins.push(Bin {
        id: bin_id(bins.len())?,
        lower: None,
        upper: None,
        categorical_value: None,
        proportion: 0.0,
    });

    Ok(FittedPsiFeature {
        feature: feature.clone(),
        bin_type: BinType::Categorical,
        bins,
        total_count,
    })
}

/// Convert a bin position to its stored `i32` id.
///
/// # Errors
/// Returns [`DriftFitError::PsiInternal`] beyond the `i32` range.
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

    /// Categorical bins are the sorted labels plus a trailing, unlabeled
    /// `other` bin with zero baseline proportion.
    ///
    /// # Panics
    /// Panics when the bins, order, or proportions differ.
    #[test]
    fn categorical_fit_sorts_bins_and_reserves_other() {
        let batch = RecordBatch::try_new(
            Arc::new(Schema::new(vec![Field::new("kind", DataType::Utf8, true)])),
            vec![Arc::new(StringArray::from(vec![
                "beta", "alpha", "alpha", "gamma",
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
        let values: Vec<Option<&str>> = fitted
            .bins
            .iter()
            .map(|bin| bin.categorical_value.as_deref())
            .collect();
        assert_eq!(
            values,
            vec![Some("alpha"), Some("beta"), Some("gamma"), None]
        );
        let proportions: Vec<f64> = fitted.bins.iter().map(|bin| bin.proportion).collect();
        assert_eq!(proportions, vec![0.5, 0.25, 0.25, 0.0]);
    }

    /// A null baseline value fails the fit for numeric and categorical
    /// features instead of being dropped.
    ///
    /// # Panics
    /// Panics when a baseline with a null fits.
    #[test]
    fn null_baseline_values_fail_the_fit() {
        let numeric = numeric_batch("score", vec![Some(1.0), None, Some(3.0)]);
        let profile = fixed_profile(PsiBinningStrategy::EqualWidth { n_bins: 2 });
        assert!(matches!(
            fit_psi_baseline(&numeric, &profile, &[feature("score")]),
            Err(DriftFitError::NullValuesInColumn { feature }) if feature == "score"
        ));

        let categorical = RecordBatch::try_new(
            Arc::new(Schema::new(vec![Field::new("kind", DataType::Utf8, true)])),
            vec![Arc::new(StringArray::from(vec![Some("a"), None]))],
        )
        .expect("record batch");
        let profile = profile_with_categorical(
            PsiBinningStrategy::EqualWidth { n_bins: 2 },
            vec![feature("kind")],
        );
        assert!(matches!(
            fit_psi_baseline(&categorical, &profile, &[feature("kind")]),
            Err(DriftFitError::NullValuesInColumn { feature }) if feature == "kind"
        ));
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
    //! Target scoring: exhaustive bins, completeness, evidence, and thresholds.

    use std::sync::Arc;

    use crate::report::FeatureEvidence;
    use crate::{DriftReport, DriftScoreError, DriftVerdict, fit_psi_baseline, score_psi};
    use arrow::array::{ArrayRef, Float64Array, Int64Array, StringArray};
    use arrow::record_batch::RecordBatch;
    use arrow_schema::{DataType, Field, Schema};
    use wyrd_spec::card::drift::{DriftMethod, PsiBinningStrategy, PsiProfile, PsiThreshold};
    use wyrd_spec::ids::FeatureName;

    fn feature(name: &str) -> FeatureName {
        FeatureName::new(name).expect("valid feature name")
    }

    /// A nullable numeric `x` column beside a nullable text `c` column.
    ///
    /// # Panics
    /// Panics when Arrow rejects the batch.
    fn two_column_batch(x: Vec<Option<f64>>, c: Vec<Option<&str>>) -> RecordBatch {
        RecordBatch::try_new(
            Arc::new(Schema::new(vec![
                Field::new("x", DataType::Float64, true),
                Field::new("c", DataType::Utf8, true),
            ])),
            vec![
                Arc::new(Float64Array::from(x)) as ArrayRef,
                Arc::new(StringArray::from(c)),
            ],
        )
        .expect("record batch")
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

    /// A direct target batch is already selected, so a row whose configured
    /// features are all null is a selected observation, not an unrelated one:
    /// it makes an otherwise sufficient target wholly unscored instead of
    /// vanishing from the counts.
    ///
    /// # Panics
    /// Panics when the null-only row is dropped and the target scores.
    #[test]
    fn psi_selected_null_only_row_makes_the_target_unscored() {
        let profile = PsiProfile {
            categorical_features: vec![feature("c")],
            ..psi_profile_default()
        };
        let baseline_batch = two_column_batch(
            (0..1000).map(|value| Some(value as f64)).collect(),
            (0..1000).map(|value| Some(["a", "b"][value % 2])).collect(),
        );
        let baseline = fit_psi_baseline(&baseline_batch, &profile, &[feature("x"), feature("c")])
            .expect("baseline");
        let mut x = (0..200).map(|value| Some(value as f64)).collect::<Vec<_>>();
        let mut c = (0..200).map(|_| Some("a")).collect::<Vec<_>>();
        let selected = score_psi(&baseline, &two_column_batch(x.clone(), c.clone()), &profile)
            .expect("report");
        assert_ne!(selected.verdict, DriftVerdict::Inconclusive);

        x.push(None);
        c.push(None);
        let report = score_psi(&baseline, &two_column_batch(x, c), &profile).expect("report");

        assert_eq!(report, DriftReport::unscored(DriftMethod::Psi));
    }

    /// A selected observation omitting a configured feature, or holding a
    /// null or non-finite value, makes the whole target unscored: no feature
    /// rows, nothing dropped or imputed.
    ///
    /// # Panics
    /// Panics when an incomplete target scores.
    #[test]
    fn psi_incomplete_targets_are_unscored() {
        let profile = PsiProfile {
            categorical_features: vec![feature("c")],
            ..psi_profile_default()
        };
        let baseline_batch = two_column_batch(
            (0..1000).map(|value| Some(value as f64)).collect(),
            (0..1000).map(|value| Some(["a", "b"][value % 2])).collect(),
        );
        let baseline = fit_psi_baseline(&baseline_batch, &profile, &[feature("x"), feature("c")])
            .expect("baseline");
        let calm = || (0..200).map(|value| Some(value as f64)).collect::<Vec<_>>();
        let labels = || (0..200).map(|_| Some("a")).collect::<Vec<_>>();

        let complete =
            score_psi(&baseline, &two_column_batch(calm(), labels()), &profile).expect("report");
        assert_ne!(complete.verdict, DriftVerdict::Inconclusive);

        let unscored = DriftReport::unscored(DriftMethod::Psi);
        for bad in [None, Some(f64::NAN), Some(f64::INFINITY)] {
            let mut x = calm();
            x[3] = bad;
            let report =
                score_psi(&baseline, &two_column_batch(x, labels()), &profile).expect("report");
            assert_eq!(report, unscored, "{bad:?}");
        }
        let mut c = labels();
        c[3] = None;
        assert_eq!(
            score_psi(&baseline, &two_column_batch(calm(), c), &profile).expect("report"),
            unscored,
            "omitted categorical feature"
        );
        assert_eq!(
            score_psi(&baseline, &numeric_batch("x", vec![1.0; 200]), &profile).expect("report"),
            unscored,
            "omitted column"
        );
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

    /// Unseen categories land in the `other` bin: target proportions sum to
    /// one, the zero-baseline bin is epsilon-smoothed, and the evidence
    /// exposes every bin.
    ///
    /// # Panics
    /// Panics when the score or evidence differs from the closed form.
    #[test]
    fn psi_unseen_categories_land_in_the_other_bin() {
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
                .chain(std::iter::repeat_n("new", 60))
                .chain(std::iter::repeat_n("newer", 40))
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
        // Bins pdx, sea, other: baseline [0.5, 0.5, 0], target [0.4, 0.5, 0.1].
        const EPS: f64 = 1e-10;
        let expected: f64 = [(0.5_f64, 0.4_f64), (0.5, 0.5), (0.0, 0.1)]
            .iter()
            .map(|(p, q)| {
                let pe = p + EPS;
                let qe = q + EPS;
                (pe - qe) * (pe / qe).ln()
            })
            .sum();
        assert!((feature_report.score - expected).abs() < 1e-9);
        let Some(FeatureEvidence::Psi(evidence)) = &feature_report.evidence else {
            panic!("PSI evidence");
        };
        assert_eq!(evidence.sample, 1000);
        let counts: Vec<u64> = evidence.bins.iter().map(|bin| bin.target_count).collect();
        assert_eq!(counts, vec![400, 500, 100]);
        let total: f64 = evidence.bins.iter().map(|bin| bin.target_proportion).sum();
        assert!((total - 1.0).abs() < 1e-12);
        assert_eq!(evidence.bins[2].bin.categorical_value, None);
    }

    /// Empty numeric bins are smoothed rather than dividing by zero, and the
    /// `other` bin counts toward a chi-square threshold's bin count.
    ///
    /// # Panics
    /// Panics when a zero bin breaks the score or the threshold ignores `other`.
    #[test]
    fn psi_zero_bins_and_other_count_toward_the_threshold() {
        let baseline_batch = numeric_batch("x", (0..1000).map(|value| value as f64).collect());
        let target_batch = numeric_batch("x", vec![5.0; 200]);
        let profile = psi_profile_default();
        let fname = feature("x");
        let baseline = fit_psi_baseline(&baseline_batch, &profile, std::slice::from_ref(&fname))
            .expect("baseline");
        let report = score_psi(&baseline, &target_batch, &profile).expect("report");
        let x = &report.features[&fname];
        assert!(x.score.is_finite() && x.score > 0.0);
        assert_eq!(x.verdict, DriftVerdict::Drift);

        let cfeature = feature("c");
        let profile = PsiProfile {
            binning_strategy: PsiBinningStrategy::EqualWidth { n_bins: 2 },
            categorical_features: vec![cfeature.clone()],
            threshold: PsiThreshold::ChiSquare { alpha: 0.05 },
        };
        let baseline = fit_psi_baseline(
            &cat_batch("c", (0..200).map(|i| ["a", "b"][i % 2]).collect()),
            &profile,
            std::slice::from_ref(&cfeature),
        )
        .expect("baseline");
        let report = score_psi(
            &baseline,
            &cat_batch("c", (0..200).map(|i| ["a", "b"][i % 2]).collect()),
            &profile,
        )
        .expect("report");
        let expected = crate::psi::threshold::compute_psi_threshold(&profile.threshold, 3, 200)
            .expect("threshold");
        assert_eq!(report.features[&cfeature].threshold, expected);
    }

    /// A score exactly equal to a fixed threshold is `NoDrift`; just above
    /// it is `Drift`.
    ///
    /// # Panics
    /// Panics when the boundary is not strict.
    #[test]
    fn psi_threshold_boundary_is_strict() {
        let baseline_batch = numeric_batch("x", (0..1000).map(|value| value as f64).collect());
        let target_batch = numeric_batch("x", (300..1300).map(|value| value as f64).collect());
        let fname = feature("x");
        let at = |value: f64| PsiProfile {
            binning_strategy: PsiBinningStrategy::EqualWidth { n_bins: 10 },
            categorical_features: Vec::new(),
            threshold: PsiThreshold::Fixed { value },
        };
        let baseline = fit_psi_baseline(&baseline_batch, &at(1.0), std::slice::from_ref(&fname))
            .expect("baseline");
        let score = score_psi(&baseline, &target_batch, &at(1.0))
            .expect("report")
            .features[&fname]
            .score;
        let equal = score_psi(&baseline, &target_batch, &at(score)).expect("report");
        assert_eq!(equal.verdict, DriftVerdict::NoDrift);
        let below = score_psi(&baseline, &target_batch, &at(score * (1.0 - 1e-9))).expect("report");
        assert_eq!(below.verdict, DriftVerdict::Drift);
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
