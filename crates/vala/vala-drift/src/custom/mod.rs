//! Custom drift scoring.
//!
//! Custom drift carries no fitted baseline. The author supplies
//! `baseline_value` and `alert_threshold` directly in [`CustomProfile`].
//! `score_custom` resolves the column named `metric_name` in the target
//! `RecordBatch`, computes its mean, and compares the absolute deviation
//! against `alert_threshold`.

use std::collections::BTreeMap;

use wyrd_spec::card::drift::{CustomProfile, DriftMethod};
use wyrd_spec::ids::FeatureName;

use crate::error::DriftScoreError;
use crate::feature::resolve_column;
use crate::report::{DriftReport, DriftVerdict, FeatureDriftReport};

/// Score a target `RecordBatch` against a Custom profile.
///
/// # Errors
/// - [`DriftScoreError::CustomMetricNameInvalid`] when `profile.metric_name`
///   is not a valid `FeatureName`.
/// - [`DriftScoreError::FeatureMissingInTarget`] when no target column matches
///   `profile.metric_name`.
/// - [`DriftScoreError::FeatureTypeMismatch`] when the target column is not
///   logically numeric or cannot be cast to `Float64`.
/// - [`DriftScoreError::FeatureEmpty`] when the target column has zero
///   non-null rows.
pub fn score_custom(
    target: &arrow::record_batch::RecordBatch,
    profile: &CustomProfile,
) -> Result<DriftReport, DriftScoreError> {
    let feature_name = FeatureName::new(profile.metric_name.as_str()).map_err(|_| {
        DriftScoreError::CustomMetricNameInvalid {
            name: profile.metric_name.clone(),
        }
    })?;
    let column = resolve_column(target, &feature_name).map_err(|_| {
        DriftScoreError::FeatureMissingInTarget {
            feature: profile.metric_name.clone(),
        }
    })?;
    if !column.is_numeric() {
        return Err(DriftScoreError::FeatureTypeMismatch {
            feature: profile.metric_name.clone(),
        });
    }

    let values =
        column
            .collect_f64_non_null()
            .map_err(|_| DriftScoreError::FeatureTypeMismatch {
                feature: profile.metric_name.clone(),
            })?;
    if values.is_empty() {
        return Err(DriftScoreError::FeatureEmpty {
            feature: profile.metric_name.clone(),
        });
    }
    if values.iter().any(|value| !value.is_finite()) {
        return Err(DriftScoreError::CustomInternal {
            message: "non-finite value in target column".into(),
        });
    }

    let mean = values.iter().sum::<f64>() / values.len() as f64;
    let score = (mean - profile.baseline_value).abs();
    let verdict = if score > profile.alert_threshold {
        DriftVerdict::Drift
    } else {
        DriftVerdict::NoDrift
    };

    let mut features = BTreeMap::new();
    features.insert(
        feature_name.clone(),
        FeatureDriftReport {
            feature: feature_name,
            score,
            threshold: profile.alert_threshold,
            verdict,
        },
    );

    Ok(DriftReport {
        method: DriftMethod::Custom,
        features,
        verdict,
    })
}
