use std::sync::Arc;

use arrow::array::{Float64Array, Int64Array, StringArray};
use arrow::datatypes::{DataType, Field, Schema};
use arrow::record_batch::RecordBatch;
use vala_drift::DriftFitError;
use vala_drift::psi::binning::{assign_bin, compute_edges_quantile};
use vala_drift::psi::{BinType, fit_psi_baseline};
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
fn quantile_edges_match_r7_reference_vector() {
    let values: Vec<f64> = (1..=8).map(f64::from).collect();
    let edges = compute_edges_quantile(&values, 4).expect("edges");

    assert_eq!(edges.edges.len(), 5);
    assert!((edges.edges[1] - 2.75).abs() < 1e-10);
    assert!((edges.edges[2] - 4.5).abs() < 1e-10);
    assert!((edges.edges[3] - 6.25).abs() < 1e-10);
}

#[test]
fn assign_bin_matches_left_open_right_closed_parity() {
    let edges = vec![f64::NEG_INFINITY, 10.0, 20.0, 30.0, f64::INFINITY];

    assert_eq!(assign_bin(10.0, &edges), 0);
    assert_eq!(assign_bin(10.0001, &edges), 1);
    assert_eq!(assign_bin(20.0, &edges), 1);
    assert_eq!(assign_bin(30.0, &edges), 2);
    assert_eq!(assign_bin(30.0001, &edges), 3);
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
