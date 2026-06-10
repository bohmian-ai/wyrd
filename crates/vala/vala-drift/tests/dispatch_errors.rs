//! Tests for the four untested error arms in the fit_baseline / score_drift dispatch layer.

use std::collections::BTreeMap;
use std::sync::Arc;

use arrow::array::Float64Array;
use arrow::record_batch::RecordBatch;
use arrow_schema::{DataType, Field, Schema};
use vala_drift::{DriftFitError, DriftScoreError};
use vala_drift::{FittedBaseline, SpcBaseline, fit_baseline, score_drift};
use wyrd_spec::card::drift::{
    DriftCondition, DriftMethod, DriftProfile, DriftSignal, DriftSpec, PsiBinningStrategy,
    PsiProfile, PsiThreshold, SpcAlertThreshold, SpcProfile, SpcWecoRule,
};
use wyrd_spec::envelope::CardKind;
use wyrd_spec::ids::{CardName, FeatureName};
use wyrd_spec::reference::CardRef;
use wyrd_spec::version::VersionBlock;

fn data_ref(name: &str) -> CardRef {
    CardRef {
        kind: CardKind::Data,
        name: CardName::new(name).expect("valid name"),
        version: VersionBlock::parse("1.0.0").expect("valid version"),
        space: None,
        uid: None,
    }
}

fn model_ref(name: &str) -> CardRef {
    CardRef {
        kind: CardKind::Model,
        name: CardName::new(name).expect("valid name"),
        version: VersionBlock::parse("1.0.0").expect("valid version"),
        space: None,
        uid: None,
    }
}

fn empty_batch() -> RecordBatch {
    RecordBatch::try_new(
        Arc::new(Schema::new(vec![Field::new("x", DataType::Float64, true)])),
        vec![Arc::new(Float64Array::from(vec![1.0_f64]))],
    )
    .expect("record batch")
}

fn external_spec() -> DriftSpec {
    DriftSpec::new(
        DriftMethod::External,
        model_ref("subject"),
        DriftSignal::External {
            source_ref: data_ref("source"),
        },
        DriftCondition::Above { limit: 1.0 },
        None,
        None,
        BTreeMap::new(),
    )
    .expect("valid external spec")
}

fn spc_eval_score_spec() -> DriftSpec {
    // SPC + EvalScore passes spec-level validation (EvalScore is in the allowed
    // set for SPC) but hits the SignalShapeMismatch arm in fit_baseline because
    // fit_spc_baseline only handles Distribution and Metric signals.
    DriftSpec::new(
        DriftMethod::Spc,
        model_ref("subject"),
        DriftSignal::EvalScore {
            eval_ref: CardRef {
                kind: CardKind::Eval,
                name: CardName::new("eval-card").expect("valid name"),
                version: VersionBlock::parse("1.0.0").expect("valid version"),
                space: None,
                uid: None,
            },
        },
        DriftCondition::Statistical,
        Some(DriftProfile::Spc(SpcProfile {
            sample_size: 0,
            weco_rule: SpcWecoRule::default(),
            alert_threshold: SpcAlertThreshold::Zone4,
        })),
        None,
        BTreeMap::new(),
    )
    .expect("valid spc+evalscore spec")
}

fn psi_spec() -> DriftSpec {
    let feature = FeatureName::new("x").expect("valid feature");
    DriftSpec::new(
        DriftMethod::Psi,
        model_ref("subject"),
        DriftSignal::Distribution {
            baseline_ref: data_ref("baseline"),
            features: vec![feature],
        },
        DriftCondition::Statistical,
        Some(DriftProfile::Psi(PsiProfile {
            binning_strategy: PsiBinningStrategy::EqualWidth { n_bins: 10 },
            categorical_features: vec![],
            threshold: PsiThreshold::Fixed { value: 0.25 },
        })),
        None,
        BTreeMap::new(),
    )
    .expect("valid psi spec")
}

fn spc_baseline_stub() -> FittedBaseline {
    use wyrd_version::WyrdVersion;
    FittedBaseline::Spc(SpcBaseline {
        features: BTreeMap::new(),
        chunk_size: 25,
        wyrd_version: WyrdVersion::current(),
    })
}

#[test]
fn fit_baseline_external_errors() {
    let spec = external_spec();
    let batch = empty_batch();
    let err = fit_baseline(&batch, &spec).expect_err("should error");
    assert!(
        matches!(err, DriftFitError::ExternalMethodHasNoBaseline),
        "unexpected error: {err:?}"
    );
}

#[test]
fn score_drift_external_errors() {
    let spec = external_spec();
    let baseline = FittedBaseline::Custom;
    let batch = empty_batch();
    let err = score_drift(&baseline, &batch, &spec).expect_err("should error");
    assert!(
        matches!(err, DriftScoreError::ExternalMethodNotInPhase),
        "unexpected error: {err:?}"
    );
}

#[test]
fn fit_baseline_signal_shape_mismatch() {
    let spec = spc_eval_score_spec();
    let batch = empty_batch();
    let err = fit_baseline(&batch, &spec).expect_err("should error");
    assert!(
        matches!(err, DriftFitError::SignalShapeMismatch { .. }),
        "unexpected error: {err:?}"
    );
}

#[test]
fn score_drift_cross_method_mismatch() {
    let spec = psi_spec();
    let baseline = spc_baseline_stub();
    let batch = empty_batch();
    let err = score_drift(&baseline, &batch, &spec).expect_err("should error");
    assert!(
        matches!(err, DriftScoreError::PsiInternal { .. }),
        "unexpected error: {err:?}"
    );
}
