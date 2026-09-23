use thiserror::Error;

/// Errors raised while constructing a fitted baseline from a baseline RecordBatch.
#[derive(Debug, Error)]
pub enum DriftFitError {
    #[error("feature {feature} not present in RecordBatch schema")]
    FeatureMissing { feature: String },

    #[error("feature {feature} expected numeric, got {arrow_type}")]
    FeatureNotNumeric { feature: String, arrow_type: String },

    #[error("feature {feature} expected categorical (Utf8/LargeUtf8/Dictionary), got {arrow_type}")]
    FeatureNotCategorical { feature: String, arrow_type: String },

    #[error("feature {feature} has zero non-null rows")]
    FeatureEmpty { feature: String },

    #[error("feature {feature} contains non-finite values (NaN or +/-inf)")]
    NonFiniteValuesInColumn { feature: String },

    #[error("feature {feature} has {rows} rows; SPC chunk_size is {chunk_size}")]
    InsufficientSamplesForChunk {
        feature: String,
        rows: usize,
        chunk_size: usize,
    },

    #[error("Distribution signal expected for PSI/SPC fit, got {got}")]
    SignalShapeMismatch { got: &'static str },

    #[error("PSI fit internal error: {message}")]
    PsiInternal { message: String },

    #[error("SPC fit internal error: {message}")]
    SpcInternal { message: String },
}

/// Errors raised while scoring a target RecordBatch against a fitted baseline.
#[derive(Debug, Error)]
pub enum DriftScoreError {
    #[error("feature {feature} present in baseline but missing in target")]
    FeatureMissingInTarget { feature: String },

    #[error("feature {feature} dtype mismatch between baseline and target")]
    FeatureTypeMismatch { feature: String },

    #[error("target feature {feature} has {rows} rows; SPC requires at least {chunk_size}")]
    TargetTooSmall {
        feature: String,
        rows: usize,
        chunk_size: usize,
    },

    #[error("PSI threshold computation failed: {message}")]
    ThresholdFailure { message: String },

    #[error("SPC WECO rule malformed: {got}")]
    WecoMalformed { got: String },

    #[error("Custom metric_name {name} not a valid column reference")]
    CustomMetricNameInvalid { name: String },

    #[error("feature {feature} has zero non-null rows in target")]
    FeatureEmpty { feature: String },

    #[error("PSI score internal error: {message}")]
    PsiInternal { message: String },

    #[error("SPC score internal error: {message}")]
    SpcInternal { message: String },

    #[error("Custom score internal error: {message}")]
    CustomInternal { message: String },
}
