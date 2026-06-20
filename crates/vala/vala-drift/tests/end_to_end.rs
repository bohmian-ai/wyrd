//! In-memory end-to-end tests for top-level drift dispatch.

use std::collections::BTreeMap;
use std::error::Error;
use std::sync::Arc;

use arrow::array::Float64Array;
use arrow::record_batch::RecordBatch;
use arrow_schema::{DataType, Field, Schema};
use vala_drift::{DriftReport, DriftVerdict, FittedBaseline, fit_baseline, score_drift};
use wyrd_semver::VersionBlock;
use wyrd_spec::card::drift::{
    CustomProfile, DriftCondition, DriftMethod, DriftProfile, DriftSignal, DriftSpec,
    PsiBinningStrategy, PsiProfile, PsiThreshold, SpcAlertThreshold, SpcProfile, SpcWecoRule,
};
use wyrd_spec::envelope::CardKind;
use wyrd_spec::ids::{CardName, FeatureName, SpaceName};
use wyrd_spec::reference::CardRef;

fn numeric_batch(name: &str, values: Vec<f64>) -> Result<RecordBatch, Box<dyn Error>> {
    let schema = Schema::new(vec![Field::new(name, DataType::Float64, true)]);
    let array = Float64Array::from(values);
    Ok(RecordBatch::try_new(
        Arc::new(schema),
        vec![Arc::new(array)],
    )?)
}

fn card_ref(kind: CardKind, name: &str) -> Result<CardRef, Box<dyn Error>> {
    Ok(CardRef {
        kind,
        name: CardName::new(name)?,
        version: VersionBlock::parse("1.0.0")?,
        space: SpaceName::new("default")?,
        uid: None,
    })
}

fn data_ref(name: &str) -> Result<CardRef, Box<dyn Error>> {
    card_ref(CardKind::Data, name)
}

fn model_ref(name: &str) -> Result<CardRef, Box<dyn Error>> {
    card_ref(CardKind::Model, name)
}

fn psi_spec(feature: &FeatureName) -> Result<DriftSpec, Box<dyn Error>> {
    Ok(DriftSpec::new(
        DriftMethod::Psi,
        model_ref("subject-model")?,
        DriftSignal::Distribution {
            baseline_ref: data_ref("baseline-data")?,
            features: vec![feature.clone()],
        },
        DriftCondition::Statistical,
        Some(DriftProfile::Psi(PsiProfile {
            binning_strategy: PsiBinningStrategy::EqualWidth { n_bins: 10 },
            categorical_features: vec![],
            threshold: PsiThreshold::Fixed { value: 0.25 },
        })),
        None,
        BTreeMap::new(),
    )?)
}

fn spc_distribution_spec(feature: &FeatureName) -> Result<DriftSpec, Box<dyn Error>> {
    Ok(DriftSpec::new(
        DriftMethod::Spc,
        model_ref("subject-model")?,
        DriftSignal::Distribution {
            baseline_ref: data_ref("baseline-data")?,
            features: vec![feature.clone()],
        },
        DriftCondition::Statistical,
        Some(DriftProfile::Spc(spc_profile())),
        None,
        BTreeMap::new(),
    )?)
}

fn spc_metric_spec(name: &str) -> Result<DriftSpec, Box<dyn Error>> {
    Ok(DriftSpec::new(
        DriftMethod::Spc,
        model_ref("subject-model")?,
        DriftSignal::Metric {
            name: name.to_string(),
        },
        DriftCondition::Statistical,
        Some(DriftProfile::Spc(spc_profile())),
        None,
        BTreeMap::new(),
    )?)
}

fn custom_spec(name: &str) -> Result<DriftSpec, Box<dyn Error>> {
    Ok(DriftSpec::new(
        DriftMethod::Custom,
        model_ref("subject-model")?,
        DriftSignal::Metric {
            name: name.to_string(),
        },
        DriftCondition::Statistical,
        Some(DriftProfile::Custom(CustomProfile {
            metric_name: name.to_string(),
            baseline_value: 100.0,
            alert_threshold: 5.0,
        })),
        None,
        BTreeMap::new(),
    )?)
}

fn spc_profile() -> SpcProfile {
    SpcProfile {
        sample_size: 0,
        weco_rule: SpcWecoRule::default(),
        alert_threshold: SpcAlertThreshold::Zone4,
    }
}

fn assert_report_shape(
    report: &DriftReport,
    method: DriftMethod,
    feature: &FeatureName,
    verdict: DriftVerdict,
) {
    assert_eq!(report.method, method);
    assert_eq!(report.verdict, verdict);
    assert_eq!(report.features.len(), 1);
    let feature_report = report
        .features
        .get(feature)
        .unwrap_or_else(|| panic!("missing feature report for {feature}"));
    assert_eq!(feature_report.feature, *feature);
    assert_eq!(feature_report.verdict, verdict);
    assert!(feature_report.score.is_finite());
    if method == DriftMethod::Spc {
        assert!(feature_report.threshold.is_nan());
    } else {
        assert!(feature_report.threshold.is_finite());
    }
}

#[test]
fn psi_dispatch_scores_report_shape() -> Result<(), Box<dyn Error>> {
    let feature = FeatureName::new("score")?;
    let spec = psi_spec(&feature)?;
    let baseline_batch = numeric_batch("score", (0..1_000).map(f64::from).collect())?;
    let baseline = fit_baseline(&baseline_batch, &spec)?;
    assert!(matches!(baseline, FittedBaseline::Psi(_)));

    let target = numeric_batch("score", (5_000..6_000).map(f64::from).collect())?;
    let report = score_drift(&baseline, &target, &spec)?;

    assert_report_shape(&report, DriftMethod::Psi, &feature, DriftVerdict::Drift);
    Ok(())
}

#[test]
fn spc_distribution_dispatch_scores_report_shape() -> Result<(), Box<dyn Error>> {
    let feature = FeatureName::new("latency")?;
    let spec = spc_distribution_spec(&feature)?;
    let baseline_values = (0..500).map(|idx| (f64::from(idx) * 0.01).sin()).collect();
    let baseline_batch = numeric_batch("latency", baseline_values)?;
    let baseline = fit_baseline(&baseline_batch, &spec)?;
    assert!(matches!(baseline, FittedBaseline::Spc(_)));

    let target = numeric_batch("latency", vec![100.0; 200])?;
    let report = score_drift(&baseline, &target, &spec)?;

    assert_report_shape(&report, DriftMethod::Spc, &feature, DriftVerdict::Drift);
    Ok(())
}

#[test]
fn spc_metric_dispatch_scores_single_feature_report() -> Result<(), Box<dyn Error>> {
    let feature = FeatureName::new("latency_ms")?;
    let spec = spc_metric_spec(feature.as_str())?;
    let baseline_values = (0..500).map(|idx| (f64::from(idx) * 0.01).sin()).collect();
    let baseline_batch = numeric_batch(feature.as_str(), baseline_values)?;
    let baseline = fit_baseline(&baseline_batch, &spec)?;
    assert!(matches!(baseline, FittedBaseline::Spc(_)));

    let target = numeric_batch(feature.as_str(), vec![100.0; 200])?;
    let report = score_drift(&baseline, &target, &spec)?;

    assert_report_shape(&report, DriftMethod::Spc, &feature, DriftVerdict::Drift);
    Ok(())
}

#[test]
fn custom_dispatch_scores_report_shape() -> Result<(), Box<dyn Error>> {
    let feature = FeatureName::new("latency_ms")?;
    let spec = custom_spec(feature.as_str())?;
    let baseline_batch = numeric_batch(feature.as_str(), vec![100.0])?;
    let baseline = fit_baseline(&baseline_batch, &spec)?;
    assert!(matches!(baseline, FittedBaseline::Custom));

    let target = numeric_batch(feature.as_str(), vec![200.0, 210.0, 220.0])?;
    let report = score_drift(&baseline, &target, &spec)?;

    assert_report_shape(&report, DriftMethod::Custom, &feature, DriftVerdict::Drift);
    Ok(())
}
