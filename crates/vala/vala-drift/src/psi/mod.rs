//! PSI baseline fit and target scoring.
//!
//! Score body lands in Commit 6.

pub mod binning;
pub mod score;
pub mod threshold;

use std::collections::BTreeMap;

use wyrd_spec::card::drift::{PsiBinningStrategy, PsiProfile};
use wyrd_spec::ids::FeatureName;
use wyrd_version::WyrdVersion;

use crate::error::{DriftFitError, DriftScoreError};
use crate::feature::{ColumnRef, resolve_column};
use crate::psi::binning::{assign_bin, compute_edges_equal_width, compute_edges_quantile};
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

/// Body lands in Commit 6.
pub fn score_psi(
    _baseline: &PsiBaseline,
    _target: &arrow::record_batch::RecordBatch,
    _profile: &PsiProfile,
) -> Result<DriftReport, DriftScoreError> {
    unimplemented!("score_psi body lands in Commit 6")
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
        .map_err(|()| DriftFitError::FeatureNotNumeric {
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
            .map_err(|()| DriftFitError::FeatureNotCategorical {
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
