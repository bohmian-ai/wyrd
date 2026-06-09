//! End-to-end tests for SPC scoring.

use std::sync::Arc;

use arrow::array::{Float64Array, StringArray};
use arrow::record_batch::RecordBatch;
use arrow_schema::{DataType, Field, Schema};

use vala_drift::{DriftScoreError, DriftVerdict, fit_spc_baseline, score_spc};
use wyrd_spec::card::drift::{SpcAlertThreshold, SpcProfile, SpcWecoRule};
use wyrd_spec::ids::FeatureName;

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

fn profile_default() -> SpcProfile {
    SpcProfile {
        sample_size: 0,
        weco_rule: SpcWecoRule::default(),
        alert_threshold: SpcAlertThreshold::Zone4,
    }
}

#[test]
fn spc_in_control_target_no_drift() {
    let baseline_batch = numeric_batch("x", vec![0.0; 500]);
    let target = numeric_batch("x", vec![0.0; 500]);
    let profile = profile_default();
    let feature = FeatureName::new("x").expect("valid feature name");
    let baseline =
        fit_spc_baseline(&baseline_batch, &profile, &[feature]).expect("SPC fit should pass");
    let report = score_spc(&baseline, &target, &profile).expect("SPC score should pass");
    assert_eq!(report.verdict, DriftVerdict::NoDrift);
}

#[test]
fn spc_out_of_bounds_target_drift_zone4() {
    let baseline_values: Vec<f64> = (0..500).map(|idx| ((idx as f64) * 0.01).sin()).collect();
    let baseline_batch = numeric_batch("x", baseline_values);
    let target = numeric_batch("x", vec![100.0; 200]);
    let profile = profile_default();
    let feature = FeatureName::new("x").expect("valid feature name");
    let baseline =
        fit_spc_baseline(&baseline_batch, &profile, &[feature]).expect("SPC fit should pass");
    let report = score_spc(&baseline, &target, &profile).expect("SPC score should pass");
    assert_eq!(report.verdict, DriftVerdict::Drift);
}

#[test]
fn spc_drift_with_zone1_threshold_detects_run() {
    let baseline_values: Vec<f64> = (0..500).map(|idx| ((idx as f64) * 0.1).sin()).collect();
    let baseline_batch = numeric_batch("x", baseline_values);
    let target = numeric_batch("x", vec![0.05; 200]);
    let profile = SpcProfile {
        sample_size: 0,
        weco_rule: SpcWecoRule::default(),
        alert_threshold: SpcAlertThreshold::Zone1,
    };
    let feature = FeatureName::new("x").expect("valid feature name");
    let baseline =
        fit_spc_baseline(&baseline_batch, &profile, &[feature]).expect("SPC fit should pass");
    let report = score_spc(&baseline, &target, &profile).expect("SPC score should pass");
    assert_eq!(report.verdict, DriftVerdict::Drift);
}

#[test]
fn spc_target_smaller_than_chunk_size_errors() {
    let baseline_batch = numeric_batch("x", (0..500).map(|idx| idx as f64).collect());
    let target = numeric_batch("x", vec![1.0, 2.0, 3.0]);
    let profile = profile_default();
    let feature = FeatureName::new("x").expect("valid feature name");
    let baseline =
        fit_spc_baseline(&baseline_batch, &profile, &[feature]).expect("SPC fit should pass");
    let err = score_spc(&baseline, &target, &profile).expect_err("target should be too small");
    assert!(matches!(err, DriftScoreError::TargetTooSmall { .. }));
}

#[test]
fn spc_missing_feature_in_target() {
    let baseline_batch = numeric_batch("x", (0..500).map(|idx| idx as f64).collect());
    let target = numeric_batch("y", vec![1.0; 100]);
    let profile = profile_default();
    let feature = FeatureName::new("x").expect("valid feature name");
    let baseline =
        fit_spc_baseline(&baseline_batch, &profile, &[feature]).expect("SPC fit should pass");
    let err = score_spc(&baseline, &target, &profile).expect_err("target feature should be absent");
    assert!(matches!(
        err,
        DriftScoreError::FeatureMissingInTarget { .. }
    ));
}

#[test]
fn spc_non_numeric_target_errors() {
    let baseline_batch = numeric_batch("x", (0..500).map(|idx| idx as f64).collect());
    let target = string_batch("x", vec!["1", "2", "3"]);
    let profile = profile_default();
    let feature = FeatureName::new("x").expect("valid feature name");
    let baseline =
        fit_spc_baseline(&baseline_batch, &profile, &[feature]).expect("SPC fit should pass");
    let err = score_spc(&baseline, &target, &profile).expect_err("target feature is not numeric");
    assert!(matches!(err, DriftScoreError::FeatureTypeMismatch { .. }));
}
