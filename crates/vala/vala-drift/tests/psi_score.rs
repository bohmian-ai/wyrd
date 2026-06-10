use std::sync::Arc;

use arrow::array::{Float64Array, Int64Array, StringArray};
use arrow::record_batch::RecordBatch;
use arrow_schema::{DataType, Field, Schema};
use vala_drift::{DriftScoreError, DriftVerdict, fit_psi_baseline, score_psi};
use wyrd_spec::card::drift::{PsiBinningStrategy, PsiProfile, PsiThreshold};
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

#[test]
fn psi_missing_target_feature_errors() {
    let baseline_batch = numeric_batch("x", (0..1000).map(|value| value as f64).collect());
    let target_batch = numeric_batch("not_x", vec![1.0, 2.0, 3.0]);
    let profile = psi_profile_default();
    let fname = feature("x");
    let baseline = fit_psi_baseline(&baseline_batch, &profile, &[fname]).expect("baseline");

    let err = score_psi(&baseline, &target_batch, &profile).expect_err("score error");

    assert!(matches!(
        err,
        DriftScoreError::FeatureMissingInTarget { feature } if feature == "x"
    ));
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

#[test]
fn psi_new_categorical_values_count_in_denominator_only() {
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
            .chain(std::iter::repeat_n("new", 100))
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
    // Inline PSI formula with epsilon=1e-10 for the two-bin case: baseline=[0.5,0.5], target=[0.5,0.4]
    const EPS: f64 = 1e-10;
    let expected: f64 = [(0.5_f64, 0.5_f64), (0.5, 0.4)]
        .iter()
        .map(|(p, q)| {
            let pe = p + EPS;
            let qe = q + EPS;
            (pe - qe) * (pe / qe).ln()
        })
        .sum();
    assert!((feature_report.score - expected).abs() < 1e-12);
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
