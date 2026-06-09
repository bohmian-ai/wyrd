//! PSI baseline fit and target scoring.

pub mod binning;
pub mod score;
pub mod threshold;

use std::collections::BTreeMap;
use std::collections::HashMap;

use arrow_schema::DataType;
use wyrd_spec::card::drift::{PsiBinningStrategy, PsiProfile};
use wyrd_spec::ids::FeatureName;
use wyrd_version::WyrdVersion;

use crate::error::{DriftFitError, DriftScoreError};
use crate::feature::{ColumnRef, resolve_column};
use crate::psi::binning::{assign_bin, compute_edges_equal_width, compute_edges_quantile};
use crate::psi::score::{PSI_MIN_TARGET_SAMPLE, psi};
use crate::psi::threshold::compute_psi_threshold;
use crate::report::{DriftReport, DriftVerdict, FeatureDriftReport};

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

pub fn fit_psi_baseline(
    batch: &arrow::record_batch::RecordBatch,
    profile: &PsiProfile,
    features: &[FeatureName],
) -> Result<PsiBaseline, DriftFitError> {
    let mut fitted_features = BTreeMap::new();

    for feature in features {
        let column = resolve_column(batch, feature)?;
        let fitted = if profile.categorical_features.contains(feature) {
            fit_categorical(feature, &column)?
        } else {
            fit_numeric(feature, &column, &profile.binning_strategy)?
        };
        fitted_features.insert(feature.clone(), fitted);
    }

    Ok(PsiBaseline {
        features: fitted_features,
        wyrd_version: WyrdVersion::current(),
    })
}

pub fn score_psi(
    baseline: &PsiBaseline,
    target: &arrow::record_batch::RecordBatch,
    profile: &PsiProfile,
) -> Result<DriftReport, DriftScoreError> {
    use wyrd_spec::card::drift::DriftMethod;

    let mut feature_reports = BTreeMap::new();
    for (feature_name, fitted) in &baseline.features {
        let column = resolve_column(target, feature_name).map_err(|_| {
            DriftScoreError::FeatureMissingInTarget {
                feature: feature_name.as_str().to_string(),
            }
        })?;
        let (target_proportions, target_total) = match fitted.bin_type {
            BinType::Numeric => target_numeric_proportions(fitted, &column)?,
            BinType::Categorical => target_categorical_proportions(fitted, &column)?,
        };

        if target_total < PSI_MIN_TARGET_SAMPLE {
            feature_reports.insert(
                feature_name.clone(),
                FeatureDriftReport {
                    feature: feature_name.clone(),
                    score: f64::NAN,
                    threshold: f64::NAN,
                    verdict: DriftVerdict::Inconclusive,
                },
            );
            continue;
        }

        let baseline_proportions = fitted
            .bins
            .iter()
            .map(|bin| bin.proportion)
            .collect::<Vec<_>>();
        let score = psi(&baseline_proportions, &target_proportions);
        let threshold = compute_psi_threshold(&profile.threshold, fitted.bins.len(), target_total)?;
        let verdict = if score > threshold {
            DriftVerdict::Drift
        } else {
            DriftVerdict::NoDrift
        };

        feature_reports.insert(
            feature_name.clone(),
            FeatureDriftReport {
                feature: feature_name.clone(),
                score,
                threshold,
                verdict,
            },
        );
    }

    let verdict = DriftReport::aggregate_verdict(&feature_reports);
    Ok(DriftReport {
        method: DriftMethod::Psi,
        features: feature_reports,
        verdict,
    })
}

fn target_numeric_proportions(
    fitted: &FittedPsiFeature,
    column: &ColumnRef<'_>,
) -> Result<(Vec<f64>, u64), DriftScoreError> {
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

    let edges = numeric_edges(fitted)?;
    let mut counts = vec![0_u64; fitted.bins.len()];
    for value in &values {
        if !value.is_finite() {
            return Err(DriftScoreError::PsiInternal {
                message: "non-finite value in target column".to_string(),
            });
        }
        let bin = assign_bin(*value, &edges);
        counts[bin] += 1;
    }

    let proportions = counts
        .iter()
        .map(|count| *count as f64 / total as f64)
        .collect();
    Ok((proportions, total))
}

fn target_categorical_proportions(
    fitted: &FittedPsiFeature,
    column: &ColumnRef<'_>,
) -> Result<(Vec<f64>, u64), DriftScoreError> {
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

    let mut counts = vec![0_u64; fitted.bins.len()];
    let mut index_by_category = HashMap::with_capacity(fitted.bins.len());
    for (index, bin) in fitted.bins.iter().enumerate() {
        if let Some(category) = &bin.categorical_value {
            index_by_category.insert(category.as_str(), index);
        }
    }

    for value in &values {
        if let Some(index) = index_by_category.get(value.as_str()) {
            counts[*index] += 1;
        }
    }

    let proportions = counts
        .iter()
        .map(|count| *count as f64 / total as f64)
        .collect();
    Ok((proportions, total))
}

fn numeric_edges(fitted: &FittedPsiFeature) -> Result<Vec<f64>, DriftScoreError> {
    let mut edges = Vec::with_capacity(fitted.bins.len() + 1);
    let first = fitted
        .bins
        .first()
        .ok_or_else(|| DriftScoreError::PsiInternal {
            message: format!("feature {} has no fitted bins", fitted.feature.as_str()),
        })?;
    edges.push(first.lower.ok_or_else(|| DriftScoreError::PsiInternal {
        message: format!(
            "numeric feature {} has a first bin without lower edge",
            fitted.feature.as_str()
        ),
    })?);

    for bin in &fitted.bins {
        edges.push(bin.upper.ok_or_else(|| DriftScoreError::PsiInternal {
            message: format!(
                "numeric feature {} has a bin without upper edge",
                fitted.feature.as_str()
            ),
        })?);
    }

    Ok(edges)
}

fn is_categorical_dtype(data_type: &DataType) -> bool {
    match data_type {
        DataType::Utf8 | DataType::LargeUtf8 => true,
        DataType::Dictionary(_, value_type) => is_categorical_dtype(value_type),
        _ => false,
    }
}

fn fit_numeric(
    feature: &FeatureName,
    column: &ColumnRef<'_>,
    strategy: &PsiBinningStrategy,
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

    let edges = match strategy {
        PsiBinningStrategy::EqualWidth { n_bins } => compute_edges_equal_width(&values, *n_bins)?,
        PsiBinningStrategy::Quantile { n_bins } => compute_edges_quantile(&values, *n_bins)?,
    };

    let mut counts = vec![0_u64; edges.edges.len() - 1];
    for value in &values {
        let bin = assign_bin(*value, &edges.edges);
        counts[bin] += 1;
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

fn fit_categorical(
    feature: &FeatureName,
    column: &ColumnRef<'_>,
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

    let mut counts: BTreeMap<String, u64> = BTreeMap::new();
    for value in values {
        *counts.entry(value).or_default() += 1;
    }

    let total_count = counts.values().sum::<u64>();
    let mut bins = Vec::with_capacity(counts.len());
    for (index, (value, count)) in counts.into_iter().enumerate() {
        bins.push(Bin {
            id: bin_id(index)?,
            lower: None,
            upper: None,
            categorical_value: Some(value),
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
