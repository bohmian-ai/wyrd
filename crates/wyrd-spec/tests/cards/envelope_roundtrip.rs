use std::collections::BTreeMap;

use skald_spec::wire::openai_chat::OpenAiMessageContent;
use skald_spec::{
    OpenAiChatMessage, OpenAiChatRequest, OpenAiChatSettings, Prompt, ProviderRequest, ResponseType,
};
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
        spec: Spec::Prompt(prompt_spec()),
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

fn prompt_spec() -> PromptSpec {
    PromptSpec::new(Prompt {
        request: ProviderRequest::OpenAiChatCompletion(OpenAiChatRequest {
            model: "gpt-4o".to_owned(),
            messages: vec![OpenAiChatMessage {
                role: "user".to_owned(),
                content: Some(OpenAiMessageContent::Text("Answer carefully.".to_owned())),
                name: None,
                tool_calls: None,
                tool_call_id: None,
                refusal: None,
            }],
            response_format: None,
            stream: None,
            stream_options: None,
            tools: None,
            tool_choice: None,
            parallel_tool_calls: None,
            settings: OpenAiChatSettings::default(),
        }),
        model: "gpt-4o".to_owned(),
        version: None,
        variables: Vec::new(),
        media_variables: Vec::new(),
        response_type: ResponseType::Text,
    })
    .expect("static prompt spec is valid")
}

#[test]
fn phase_1_addendum_fixtures_round_trip() {
    for fixture in [
        include_str!("../fixtures/trigger-on-drift.yaml"),
        include_str!("../fixtures/operator-remediation.yaml"),
        include_str!("../fixtures/service-with-runtime-policy.yaml"),
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
        format::yaml::from_str(include_str!("../fixtures/trigger-on-drift.yaml")).unwrap();
    assert_eq!(decoded.kind, CardKind::Trigger);
    assert!(matches!(decoded.spec, Spec::Trigger(_)));
}

#[test]
fn operator_fixture_uses_native_operator_kind() {
    let decoded: Card =
        format::yaml::from_str(include_str!("../fixtures/operator-remediation.yaml")).unwrap();
    assert_eq!(decoded.kind, CardKind::Operator);
    assert!(matches!(decoded.spec, Spec::Operator(_)));
}
