use std::collections::BTreeMap;

use wyrd_spec::card::prompt::PromptSpec;
use wyrd_spec::envelope::{Card, CardKind, Metadata, Relationships, Spec};
use wyrd_spec::format;
use wyrd_spec::ids::CardName;
use wyrd_spec::version::{ApiVersion, VersionBlock};

#[test]
fn card_yaml_round_trip() {
    let card = Card {
        api_version: ApiVersion::v1(),
        kind: CardKind::Prompt,
        metadata: Metadata {
            name: CardName::new("support_prompt").unwrap(),
            version: VersionBlock::parse("1.0.0").unwrap(),
            space: None,
            uid: None,
            labels: BTreeMap::new(),
            annotations: BTreeMap::new(),
            spec_hash: None,
            artifact_hash: None,
        },
        spec: Spec::Prompt(PromptSpec {
            template: "Answer carefully.".to_string(),
            ..PromptSpec::default()
        }),
        relationships: Relationships::default(),
        status: None,
    };

    let yaml = format::yaml::to_string(&card).unwrap();
    assert!(yaml.contains("apiVersion: wyrd/v1"));
    assert!(yaml.contains("type: Prompt"));
    assert!(!yaml.contains("api_version:"));
    let decoded = format::yaml::from_str(&yaml).unwrap();
    assert_eq!(decoded, card);
}

#[test]
fn phase_1_addendum_fixtures_round_trip() {
    for fixture in [
        include_str!("fixtures/trigger-on-drift.yaml"),
        include_str!("fixtures/operator-remediation.yaml"),
        include_str!("fixtures/service-with-runtime-policy.yaml"),
    ] {
        let decoded: Card = format::yaml::from_str(fixture).unwrap();
        let encoded = format::yaml::to_string(&decoded).unwrap();
        let reparsed: Card = format::yaml::from_str(&encoded).unwrap();
        assert_eq!(reparsed, decoded);
    }
}

#[test]
fn trigger_fixture_uses_native_trigger_kind() {
    let decoded: Card =
        format::yaml::from_str(include_str!("fixtures/trigger-on-drift.yaml")).unwrap();
    assert_eq!(decoded.kind, CardKind::Trigger);
    assert!(matches!(decoded.spec, Spec::Trigger(_)));
}

#[test]
fn operator_fixture_uses_native_operator_kind() {
    let decoded: Card =
        format::yaml::from_str(include_str!("fixtures/operator-remediation.yaml")).unwrap();
    assert_eq!(decoded.kind, CardKind::Operator);
    assert!(matches!(decoded.spec, Spec::Operator(_)));
}
