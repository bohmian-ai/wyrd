//! Unit tests for `fit_spc_baseline`.

use std::sync::Arc;

use arrow::array::{Float64Array, Int64Array, StringArray};
use arrow::record_batch::RecordBatch;
use arrow_schema::{DataType, Field, Schema};
use vala_drift::{DriftFitError, fit_spc_baseline};
use wyrd_spec::card::drift::{SpcAlertThreshold, SpcProfile, SpcWecoRule};
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
        Arc::new(Schema::new(vec![Field::new(name, DataType::Int64, true)])),
        vec![Arc::new(Int64Array::from(values))],
    )
    .expect("record batch")
}

fn spc_profile(sample_size: u32) -> SpcProfile {
    SpcProfile {
        sample_size,
        weco_rule: SpcWecoRule::default(),
        alert_threshold: SpcAlertThreshold::Zone4,
    }
}

#[test]
fn fit_adaptive_chunk_size_small_data() {
    let values: Vec<f64> = (0..500).map(|value| (value as f64).sin()).collect();
    let batch = numeric_batch("x", values);
    let profile = spc_profile(0);
    let fname = feature("x");

    let baseline = fit_spc_baseline(&batch, &profile, &[fname.clone()]).expect("baseline");

    assert_eq!(baseline.chunk_size, 25);
    let fitted = baseline.features.get(&fname).expect("feature");
    assert!(fitted.three_lcl < fitted.center && fitted.center < fitted.three_ucl);
}

#[test]
fn fit_explicit_chunk_size() {
    let values: Vec<f64> = (0..1_000).map(|value| (value as f64) * 0.1).collect();
    let batch = numeric_batch("x", values);
    let profile = spc_profile(50);
    let fname = feature("x");

    let baseline = fit_spc_baseline(&batch, &profile, &[fname]).expect("baseline");

    assert_eq!(baseline.chunk_size, 50);
}

#[test]
fn fit_from_int_column() {
    let values: Vec<i64> = (0..500).collect();
    let batch = int_batch("x", values);
    let profile = spc_profile(0);
    let fname = feature("x");

    let baseline = fit_spc_baseline(&batch, &profile, &[fname.clone()]).expect("baseline");

    let fitted = baseline.features.get(&fname).expect("feature");
    assert!(fitted.center > 0.0);
}

#[test]
fn fit_includes_trailing_partial_chunk_for_center() {
    let batch = numeric_batch("x", vec![0.0, 2.0, 2.0, 4.0, 100.0, 104.0]);
    let profile = spc_profile(4);
    let fname = feature("x");

    let baseline = fit_spc_baseline(&batch, &profile, &[fname.clone()]).expect("baseline");

    let fitted = baseline.features.get(&fname).expect("feature");
    assert!((fitted.center - 52.0).abs() < 1e-12);
}

#[test]
fn rejects_non_numeric_column() {
    let batch = RecordBatch::try_new(
        Arc::new(Schema::new(vec![Field::new("name", DataType::Utf8, true)])),
        vec![Arc::new(StringArray::from(vec!["a", "b", "c"]))],
    )
    .expect("record batch");
    let profile = spc_profile(0);
    let fname = feature("name");

    let err = fit_spc_baseline(&batch, &profile, &[fname]).expect_err("fit error");

    assert!(matches!(err, DriftFitError::FeatureNotNumeric { .. }));
}

#[test]
fn rejects_empty_column() {
    let batch = numeric_batch("x", Vec::new());
    let profile = spc_profile(0);
    let fname = feature("x");

    let err = fit_spc_baseline(&batch, &profile, &[fname]).expect_err("fit error");

    assert!(matches!(err, DriftFitError::FeatureEmpty { .. }));
}

#[test]
fn rejects_insufficient_chunks() {
    let values: Vec<f64> = (0..25).map(|value| value as f64).collect();
    let batch = numeric_batch("x", values);
    let profile = spc_profile(25);
    let fname = feature("x");

    let err = fit_spc_baseline(&batch, &profile, &[fname]).expect_err("fit error");

    assert!(matches!(
        err,
        DriftFitError::InsufficientSamplesForChunk { .. }
    ));
}

#[test]
fn rejects_missing_feature() {
    let batch = numeric_batch("x", (0..500).map(|value| value as f64).collect());
    let profile = spc_profile(0);
    let fname = feature("not_x");

    let err = fit_spc_baseline(&batch, &profile, &[fname]).expect_err("fit error");

    assert!(matches!(err, DriftFitError::FeatureMissing { .. }));
}

#[test]
fn rejects_non_finite_values() {
    let batch = numeric_batch("x", vec![1.0, f64::NAN, 3.0]);
    let profile = spc_profile(2);
    let fname = feature("x");

    let err = fit_spc_baseline(&batch, &profile, &[fname]).expect_err("fit error");

    assert!(matches!(err, DriftFitError::NonFiniteValuesInColumn { .. }));
}
