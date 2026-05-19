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
