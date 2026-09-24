//! Phase-specific drift errors: fitting a baseline and scoring a target.

use thiserror::Error;

/// Errors raised while constructing a fitted baseline from a baseline RecordBatch.
///
/// Every variant names the feature it concerns where one applies; the server
/// persists the rendered message as the visible fit failure.
#[derive(Debug, Error)]
pub enum DriftFitError {
    /// A configured feature has no column in the baseline.
    #[error("feature {feature} not present in RecordBatch schema")]
    FeatureMissing {
        /// The configured feature name.
        feature: String,
    },

    /// A numeric feature's column is not numeric.
    #[error("feature {feature} expected numeric, got {arrow_type}")]
    FeatureNotNumeric {
        /// The configured feature name.
        feature: String,
        /// The column's Arrow type.
        arrow_type: String,
    },

    /// A categorical feature's column is not text.
    #[error("feature {feature} expected categorical (Utf8/LargeUtf8/Dictionary), got {arrow_type}")]
    FeatureNotCategorical {
        /// The configured feature name.
        feature: String,
        /// The column's Arrow type.
        arrow_type: String,
    },

    /// A configured feature's column has no rows.
    #[error("feature {feature} has zero rows")]
    FeatureEmpty {
        /// The configured feature name.
        feature: String,
    },

    /// A numeric baseline value is NaN or infinite; rows are never dropped.
    #[error("feature {feature} contains non-finite values (NaN or +/-inf)")]
    NonFiniteValuesInColumn {
        /// The configured feature name.
        feature: String,
    },

    /// A required baseline value is null; baseline rows are never dropped.
    #[error("feature {feature} contains null values")]
    NullValuesInColumn {
        /// The configured feature name.
        feature: String,
    },

    /// The baseline does not divide into complete SPC subgroups.
    #[error(
        "feature {feature} has {rows} rows, not a whole number of SPC subgroups of {subgroup_size}"
    )]
    IncompleteSubgroup {
        /// The configured feature name.
        feature: String,
        /// Baseline rows of the feature.
        rows: usize,
        /// The authored subgroup size.
        subgroup_size: u32,
    },

    /// The baseline holds fewer complete SPC subgroups than a fit requires.
    #[error("feature {feature} has {subgroups} SPC subgroups; a fit requires at least {required}")]
    InsufficientSubgroups {
        /// The configured feature name.
        feature: String,
        /// Complete subgroups in the baseline.
        subgroups: usize,
        /// The minimum a fit requires.
        required: usize,
    },

    /// A PSI or SPC spec carries a Metric signal instead of features.
    #[error("Distribution signal expected for PSI/SPC fit, got {got}")]
    SignalShapeMismatch {
        /// The signal variant that was supplied.
        got: &'static str,
    },

    /// A PSI fit reached an inconsistent internal state.
    #[error("PSI fit internal error: {message}")]
    PsiInternal {
        /// What was inconsistent.
        message: String,
    },

    /// An SPC fit reached an inconsistent internal state.
    #[error("SPC fit internal error: {message}")]
    SpcInternal {
        /// What was inconsistent.
        message: String,
    },

    /// The caller cancelled the fit before it finished; no baseline exists.
    #[error("the baseline fit was cancelled")]
    Cancelled,
}

/// Errors raised while scoring a target RecordBatch against a fitted baseline.
///
/// Incomplete or insufficient target data is not an error: it scores as an
/// inconclusive report. These variants mark malformed inputs or state.
#[derive(Debug, Error)]
pub enum DriftScoreError {
    /// The Custom metric column is absent from the target.
    #[error("feature {feature} present in baseline but missing in target")]
    FeatureMissingInTarget {
        /// The missing feature name.
        feature: String,
    },

    /// A target column's type does not match its fitted feature.
    #[error("feature {feature} dtype mismatch between baseline and target")]
    FeatureTypeMismatch {
        /// The mismatched feature name.
        feature: String,
    },

    /// The PSI threshold could not be computed for the bin count and sample.
    #[error("PSI threshold computation failed: {message}")]
    ThresholdFailure {
        /// Why the threshold failed.
        message: String,
    },

    /// The Custom `metric_name` is not a valid column reference.
    #[error("Custom metric_name {name} not a valid column reference")]
    CustomMetricNameInvalid {
        /// The authored metric name.
        name: String,
    },

    /// The Custom metric column holds no non-null rows.
    #[error("feature {feature} has zero non-null rows in target")]
    FeatureEmpty {
        /// The metric feature name.
        feature: String,
    },

    /// PSI scoring reached an inconsistent internal state.
    #[error("PSI score internal error: {message}")]
    PsiInternal {
        /// What was inconsistent.
        message: String,
    },

    /// SPC scoring reached an inconsistent internal state.
    #[error("SPC score internal error: {message}")]
    SpcInternal {
        /// What was inconsistent.
        message: String,
    },

    /// Custom scoring reached an inconsistent internal state.
    #[error("Custom score internal error: {message}")]
    CustomInternal {
        /// What was inconsistent.
        message: String,
    },
}
