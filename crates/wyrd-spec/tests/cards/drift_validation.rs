use std::collections::BTreeMap;

use wyrd_spec::card::drift::{
    CustomProfile, DriftCondition, DriftMethod, DriftProfile, DriftSignal, DriftSpec,
    DriftValidationError, PsiBinningStrategy, PsiProfile, PsiThreshold, SpcAlertThreshold,
    SpcProfile, SpcWecoRule,
};
use wyrd_spec::envelope::CardKind;
use wyrd_spec::error::WyrdError;
use wyrd_spec::ids::{CardName, FeatureName};
use wyrd_spec::reference::CardRef;
use wyrd_spec::version::VersionBlock;

fn card_ref(kind: CardKind, name: &str, version: &str) -> CardRef {
    CardRef {
        kind,
        name: CardName::new(name).expect("valid card name"),
        version: VersionBlock::parse(version).expect("valid version"),
        space: None,
        uid: None,
    }
}

fn data_ref(name: &str) -> CardRef {
    card_ref(CardKind::Data, name, "1.0.0")
}

fn model_ref(name: &str) -> CardRef {
    card_ref(CardKind::Model, name, "1.0.0")
}

fn psi_profile() -> PsiProfile {
    PsiProfile {
        binning_strategy: PsiBinningStrategy::EqualWidth { n_bins: 10 },
        categorical_features: vec![],
        threshold: PsiThreshold::ChiSquare { alpha: 0.05 },
    }
}

fn spc_profile() -> SpcProfile {
    SpcProfile {
        sample_size: 0,
        weco_rule: SpcWecoRule::default(),
        alert_threshold: SpcAlertThreshold::Zone4,
    }
}

fn custom_profile() -> CustomProfile {
    CustomProfile {
        metric_name: "latency".to_string(),
        baseline_value: 100.0,
        alert_threshold: 10.0,
    }
}

fn distribution_signal() -> DriftSignal {
    DriftSignal::Distribution {
        baseline_ref: data_ref("baseline-data"),
        features: vec![FeatureName::new("feature_a").expect("valid feature")],
    }
}

fn metric_signal() -> DriftSignal {
    DriftSignal::Metric {
        name: "latency".to_string(),
    }
}

fn external_signal() -> DriftSignal {
    DriftSignal::External {
        source_ref: data_ref("source-data"),
    }
}

#[test]
fn happy_path_psi_distribution_statistical() {
    let spec = DriftSpec::new(
        DriftMethod::Psi,
        model_ref("subject-model"),
        distribution_signal(),
        DriftCondition::Statistical,
        Some(DriftProfile::Psi(psi_profile())),
        None,
        BTreeMap::new(),
    );

    assert!(spec.is_ok(), "{spec:?}");
}

#[test]
fn happy_path_external_method_with_comparator_condition() {
    let spec = DriftSpec::new(
        DriftMethod::External,
        model_ref("subject-model"),
        external_signal(),
        DriftCondition::Above { limit: 1.0 },
        None,
        None,
        BTreeMap::new(),
    );

    assert!(spec.is_ok(), "{spec:?}");
}

#[test]
fn rejects_subject_ref_not_in_allowed_set() {
    let err = DriftSpec::new(
        DriftMethod::Psi,
        card_ref(CardKind::Trigger, "subject-trigger", "1.0.0"),
        distribution_signal(),
        DriftCondition::Statistical,
        Some(DriftProfile::Psi(psi_profile())),
        None,
        BTreeMap::new(),
    )
    .unwrap_err();

    assert!(matches!(
        err,
        DriftValidationError::InvalidSubjectKind { .. }
    ));
}

#[test]
fn rejects_psi_with_metric_signal() {
    let err = DriftSpec::new(
        DriftMethod::Psi,
        model_ref("subject-model"),
        metric_signal(),
        DriftCondition::Statistical,
        Some(DriftProfile::Psi(psi_profile())),
        None,
        BTreeMap::new(),
    )
    .unwrap_err();

    assert!(matches!(
        err,
        DriftValidationError::SignalMethodMismatch { .. }
    ));
}

#[test]
fn rejects_custom_with_external_signal() {
    let err = DriftSpec::new(
        DriftMethod::Custom,
        model_ref("subject-model"),
        external_signal(),
        DriftCondition::Statistical,
        Some(DriftProfile::Custom(custom_profile())),
        None,
        BTreeMap::new(),
    )
    .unwrap_err();

    assert!(matches!(
        err,
        DriftValidationError::SignalMethodMismatch { .. }
    ));
}

#[test]
fn rejects_distribution_with_non_data_baseline_ref() {
    let signal = DriftSignal::Distribution {
        baseline_ref: model_ref("baseline-model"),
        features: vec![FeatureName::new("feature_a").expect("valid feature")],
    };

    let err = DriftSpec::new(
        DriftMethod::Psi,
        model_ref("subject-model"),
        signal,
        DriftCondition::Statistical,
        Some(DriftProfile::Psi(psi_profile())),
        None,
        BTreeMap::new(),
    )
    .unwrap_err();

    assert!(matches!(
        err,
        DriftValidationError::BaselineRefMustBeData { .. }
    ));
}

#[test]
fn rejects_distribution_with_no_features() {
    let signal = DriftSignal::Distribution {
        baseline_ref: data_ref("baseline-data"),
        features: vec![],
    };

    let err = DriftSpec::new(
        DriftMethod::Psi,
        model_ref("subject-model"),
        signal,
        DriftCondition::Statistical,
        Some(DriftProfile::Psi(psi_profile())),
        None,
        BTreeMap::new(),
    )
    .unwrap_err();

    assert!(matches!(
        err,
        DriftValidationError::DistributionMissingFeatures
    ));
}

#[test]
fn rejects_distribution_with_duplicate_features() {
    let signal = DriftSignal::Distribution {
        baseline_ref: data_ref("baseline-data"),
        features: vec![
            FeatureName::new("duplicate").expect("valid feature"),
            FeatureName::new("duplicate").expect("valid feature"),
        ],
    };

    let err = DriftSpec::new(
        DriftMethod::Psi,
        model_ref("subject-model"),
        signal,
        DriftCondition::Statistical,
        Some(DriftProfile::Psi(psi_profile())),
        None,
        BTreeMap::new(),
    )
    .unwrap_err();

    assert!(matches!(
        err,
        DriftValidationError::DistributionDuplicateFeatures { .. }
    ));
}

#[test]
fn rejects_eval_score_with_non_eval_ref() {
    let signal = DriftSignal::EvalScore {
        eval_ref: data_ref("not-eval"),
    };

    let err = DriftSpec::new(
        DriftMethod::Spc,
        model_ref("subject-model"),
        signal,
        DriftCondition::Statistical,
        Some(DriftProfile::Spc(spc_profile())),
        None,
        BTreeMap::new(),
    )
    .unwrap_err();

    assert!(matches!(
        err,
        DriftValidationError::EvalRefMustBeEval { .. }
    ));
}

#[test]
fn rejects_external_source_ref_with_drift_kind() {
    let signal = DriftSignal::External {
        source_ref: card_ref(CardKind::Drift, "bad-source", "1.0.0"),
    };

    let err = DriftSpec::new(
        DriftMethod::External,
        model_ref("subject-model"),
        signal,
        DriftCondition::Above { limit: 1.0 },
        None,
        None,
        BTreeMap::new(),
    )
    .unwrap_err();

    assert!(matches!(err, DriftValidationError::SourceRefInvalidKind));
}

#[test]
fn rejects_profile_required_for_psi() {
    let err = DriftSpec::new(
        DriftMethod::Psi,
        model_ref("subject-model"),
        distribution_signal(),
        DriftCondition::Above { limit: 1.0 },
        None,
        None,
        BTreeMap::new(),
    )
    .unwrap_err();

    assert!(matches!(err, DriftValidationError::ProfileRequired { .. }));
}

#[test]
fn rejects_profile_variant_that_does_not_match_method() {
    let err = DriftSpec::new(
        DriftMethod::Psi,
        model_ref("subject-model"),
        distribution_signal(),
        DriftCondition::Statistical,
        Some(DriftProfile::Spc(spc_profile())),
        None,
        BTreeMap::new(),
    )
    .unwrap_err();

    assert!(matches!(
        err,
        DriftValidationError::ProfileMethodMismatch { .. }
    ));
}

#[test]
fn rejects_profile_for_external_method() {
    let err = DriftSpec::new(
        DriftMethod::External,
        model_ref("subject-model"),
        external_signal(),
        DriftCondition::Above { limit: 1.0 },
        Some(DriftProfile::Psi(psi_profile())),
        None,
        BTreeMap::new(),
    )
    .unwrap_err();

    assert!(matches!(
        err,
        DriftValidationError::ProfileForbiddenForExternal
    ));
}

#[test]
fn rejects_external_method_with_statistical_condition() {
    let err = DriftSpec::new(
        DriftMethod::External,
        model_ref("subject-model"),
        external_signal(),
        DriftCondition::Statistical,
        None,
        None,
        BTreeMap::new(),
    )
    .unwrap_err();

    assert!(matches!(
        err,
        DriftValidationError::StatisticalRequiresProfile
    ));
}

#[test]
fn rejects_above_with_non_finite_limit() {
    let err = DriftSpec::new(
        DriftMethod::Custom,
        model_ref("subject-model"),
        metric_signal(),
        DriftCondition::Above { limit: f64::NAN },
        Some(DriftProfile::Custom(custom_profile())),
        None,
        BTreeMap::new(),
    )
    .unwrap_err();

    assert!(matches!(err, DriftValidationError::NonFiniteLimit { .. }));
}

#[test]
fn rejects_outside_with_inverted_bounds() {
    let err = DriftSpec::new(
        DriftMethod::Custom,
        model_ref("subject-model"),
        metric_signal(),
        DriftCondition::Outside {
            lower: 10.0,
            upper: 1.0,
        },
        Some(DriftProfile::Custom(custom_profile())),
        None,
        BTreeMap::new(),
    )
    .unwrap_err();

    assert!(matches!(err, DriftValidationError::OutsideBoundsInverted));
}

#[test]
fn rejects_psi_alpha_out_of_range() {
    let mut profile = psi_profile();
    profile.threshold = PsiThreshold::Normal { alpha: 1.0 };

    let err = DriftSpec::new(
        DriftMethod::Psi,
        model_ref("subject-model"),
        distribution_signal(),
        DriftCondition::Statistical,
        Some(DriftProfile::Psi(profile)),
        None,
        BTreeMap::new(),
    )
    .unwrap_err();

    assert!(matches!(
        err,
        DriftValidationError::PsiAlphaOutOfRange { .. }
    ));
}

#[test]
fn rejects_psi_fixed_threshold_when_not_positive() {
    let mut profile = psi_profile();
    profile.threshold = PsiThreshold::Fixed { value: 0.0 };

    let err = DriftSpec::new(
        DriftMethod::Psi,
        model_ref("subject-model"),
        distribution_signal(),
        DriftCondition::Statistical,
        Some(DriftProfile::Psi(profile)),
        None,
        BTreeMap::new(),
    )
    .unwrap_err();

    assert!(matches!(err, DriftValidationError::PsiFixedInvalid));
}

#[test]
fn rejects_psi_bin_count_out_of_range() {
    let mut profile = psi_profile();
    profile.binning_strategy = PsiBinningStrategy::Quantile { n_bins: 1 };

    let err = DriftSpec::new(
        DriftMethod::Psi,
        model_ref("subject-model"),
        distribution_signal(),
        DriftCondition::Statistical,
        Some(DriftProfile::Psi(profile)),
        None,
        BTreeMap::new(),
    )
    .unwrap_err();

    assert!(matches!(err, DriftValidationError::PsiBinCountOutOfRange));
}

#[test]
fn rejects_spc_sample_size_one() {
    let mut profile = spc_profile();
    profile.sample_size = 1;

    let err = DriftSpec::new(
        DriftMethod::Spc,
        model_ref("subject-model"),
        metric_signal(),
        DriftCondition::Statistical,
        Some(DriftProfile::Spc(profile)),
        None,
        BTreeMap::new(),
    )
    .unwrap_err();

    assert!(matches!(err, DriftValidationError::SpcSampleSizeOutOfRange));
}

#[test]
fn rejects_malformed_spc_weco_rule() {
    let mut profile = spc_profile();
    profile.weco_rule.rule_string = "8 16 0 8 2 4 1 1".to_string();

    let err = DriftSpec::new(
        DriftMethod::Spc,
        model_ref("subject-model"),
        metric_signal(),
        DriftCondition::Statistical,
        Some(DriftProfile::Spc(profile)),
        None,
        BTreeMap::new(),
    )
    .unwrap_err();

    assert!(matches!(err, DriftValidationError::SpcWecoMalformed));
}

#[test]
fn rejects_custom_profile_with_empty_metric_name() {
    let mut profile = custom_profile();
    profile.metric_name = "   ".to_string();

    let err = DriftSpec::new(
        DriftMethod::Custom,
        model_ref("subject-model"),
        metric_signal(),
        DriftCondition::Statistical,
        Some(DriftProfile::Custom(profile)),
        None,
        BTreeMap::new(),
    )
    .unwrap_err();

    assert!(matches!(err, DriftValidationError::CustomMetricNameEmpty));
}

#[test]
fn rejects_custom_profile_with_invalid_numbers() {
    let mut profile = custom_profile();
    profile.alert_threshold = -1.0;

    let err = DriftSpec::new(
        DriftMethod::Custom,
        model_ref("subject-model"),
        metric_signal(),
        DriftCondition::Statistical,
        Some(DriftProfile::Custom(profile)),
        None,
        BTreeMap::new(),
    )
    .unwrap_err();

    assert!(matches!(err, DriftValidationError::CustomInvalidNumber));
}

#[test]
fn signal_method_mismatch_routes_to_dedicated_catalog_code() {
    let err: WyrdError = DriftValidationError::SignalMethodMismatch {
        signal: "Metric".to_string(),
        method: "Psi".to_string(),
    }
    .into();

    assert_eq!(err.code(), "WYRD_DRIFT_400_SIGNAL_METHOD_MISMATCH");
    assert_eq!(err.status(), 400);
    let details = match &err {
        WyrdError::DriftSignalMethodMismatch { details, .. } => details,
        other => panic!("wrong variant: {other:?}"),
    };
    assert_eq!(
        details.get("signal").and_then(|value| value.as_str()),
        Some("Metric")
    );
    assert_eq!(
        details.get("method").and_then(|value| value.as_str()),
        Some("Psi")
    );
}

#[test]
fn profile_required_routes_to_dedicated_catalog_code() {
    let err: WyrdError = DriftValidationError::ProfileRequired {
        method: "Spc".to_string(),
    }
    .into();

    assert_eq!(err.code(), "WYRD_DRIFT_400_PROFILE_REQUIRED");
    assert_eq!(err.status(), 400);
    let details = match &err {
        WyrdError::DriftProfileRequired { details, .. } => details,
        other => panic!("wrong variant: {other:?}"),
    };
    assert_eq!(
        details.get("method").and_then(|value| value.as_str()),
        Some("Spc")
    );
}

#[test]
fn catch_all_routes_to_drift_validation_catalog_code() {
    let err: WyrdError = DriftValidationError::OutsideBoundsInverted.into();

    assert_eq!(err.code(), "WYRD_DRIFT_400_VALIDATION");
    assert_eq!(err.status(), 400);
    let details = match &err {
        WyrdError::DriftValidation { details, .. } => details,
        other => panic!("wrong variant: {other:?}"),
    };
    assert_eq!(
        details.get("field").and_then(|value| value.as_str()),
        Some("condition")
    );
    assert_eq!(
        details.get("reason").and_then(|value| value.as_str()),
        Some("lower_gte_upper")
    );
}
