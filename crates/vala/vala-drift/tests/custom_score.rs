//! End-to-end tests for Custom scoring.

use std::sync::Arc;

use arrow::array::{Float64Array, StringArray};
use arrow::record_batch::RecordBatch;
use arrow_schema::{DataType, Field, Schema};

use vala_drift::{DriftScoreError, DriftVerdict, score_custom};
use wyrd_spec::card::drift::CustomProfile;

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

fn custom(name: &str, baseline_value: f64, alert_threshold: f64) -> CustomProfile {
    CustomProfile {
        metric_name: name.into(),
        baseline_value,
        alert_threshold,
    }
}

#[test]
fn custom_within_threshold_is_no_drift() {
    let batch = numeric_batch("latency_ms", vec![100.0, 102.0, 98.0, 101.0]);
    let profile = custom("latency_ms", 100.0, 5.0);

    let report = score_custom(&batch, &profile).expect("Custom score should pass");

    assert_eq!(report.verdict, DriftVerdict::NoDrift);
    let feature = report
        .features
        .values()
        .next()
        .expect("feature report should exist");
    assert!((feature.score - 0.25).abs() < 1e-9);
    assert_eq!(feature.threshold, 5.0);
    assert_eq!(feature.verdict, DriftVerdict::NoDrift);
}

#[test]
fn custom_outside_threshold_is_drift() {
    let batch = numeric_batch("latency_ms", vec![200.0, 210.0, 220.0]);
    let profile = custom("latency_ms", 100.0, 50.0);

    let report = score_custom(&batch, &profile).expect("Custom score should pass");

    assert_eq!(report.verdict, DriftVerdict::Drift);
    let feature = report
        .features
        .values()
        .next()
        .expect("feature report should exist");
    assert_eq!(feature.score, 110.0);
    assert_eq!(feature.threshold, 50.0);
    assert_eq!(feature.verdict, DriftVerdict::Drift);
}

#[test]
fn custom_missing_column_errors() {
    let batch = numeric_batch("other", vec![1.0, 2.0, 3.0]);
    let profile = custom("latency_ms", 100.0, 5.0);

    let err = score_custom(&batch, &profile).expect_err("target feature should be absent");

    assert!(matches!(
        err,
        DriftScoreError::FeatureMissingInTarget { .. }
    ));
}

#[test]
fn custom_non_numeric_column_errors() {
    let batch = string_batch("latency_ms", vec!["a", "b"]);
    let profile = custom("latency_ms", 100.0, 5.0);

    let err = score_custom(&batch, &profile).expect_err("target feature should be non-numeric");

    assert!(matches!(err, DriftScoreError::FeatureTypeMismatch { .. }));
}

#[test]
fn custom_empty_column_errors() {
    let batch = numeric_batch("latency_ms", vec![]);
    let profile = custom("latency_ms", 100.0, 5.0);

    let err = score_custom(&batch, &profile).expect_err("target feature should be empty");

    assert!(matches!(err, DriftScoreError::FeatureEmpty { .. }));
}

#[test]
fn custom_non_finite_column_errors() {
    let batch = numeric_batch("latency_ms", vec![1.0, f64::NAN]);
    let profile = custom("latency_ms", 100.0, 5.0);

    let err = score_custom(&batch, &profile).expect_err("target feature should be finite");

    assert!(matches!(err, DriftScoreError::CustomInternal { .. }));
}

#[test]
fn custom_invalid_metric_name_errors() {
    let batch = numeric_batch("latency_ms", vec![1.0, 2.0]);
    let profile = custom("", 100.0, 5.0);

    let err = score_custom(&batch, &profile).expect_err("metric name should be invalid");

    assert!(matches!(
        err,
        DriftScoreError::CustomMetricNameInvalid { .. }
    ));
}
