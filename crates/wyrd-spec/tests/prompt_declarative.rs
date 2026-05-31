use skald_spec::{ProviderRequest, ResponseType};
use wyrd_spec::card::prompt::PromptSpec;
use wyrd_spec::envelope::{Card, CardKind, Spec};
use wyrd_spec::format;

fn declarative_yaml() -> &'static str {
    r#"
apiVersion: wyrd/v1
kind: Prompt
metadata:
  name: test-prompt
  version: 0.1.0
spec:
  provider: openai
  model: gpt-4o
  system: "You are {{persona}}."
  messages:
    - "Summarize {{doc}}."
  model_settings:
    temperature: 0.2
"#
}

fn native_yaml(provider: &str, model: &str) -> String {
    // Build via the builder and serialize, to get the native round-trip fixture
    let prompt = skald_spec::authoring::PromptDraft {
        provider: provider.to_owned(),
        model: model.to_owned(),
        operation: None,
        system: None,
        messages: skald_spec::authoring::DraftMessages::Many(vec!["Hello".to_owned()]),
        model_settings: None,
        version: None,
    }
    .compile()
    .unwrap();

    let spec = PromptSpec::new(prompt).unwrap();
    let card = Card {
        api_version: wyrd_spec::version::ApiVersion::v1(),
        kind: CardKind::Prompt,
        metadata: wyrd_spec::envelope::Metadata {
            name: wyrd_spec::ids::CardName::new("test").unwrap(),
            version: wyrd_spec::version::VersionBlock::parse("0.1.0").unwrap(),
            space: None,
            uid: None,
            labels: Default::default(),
            annotations: Default::default(),
            spec_hash: None,
            artifact_hash: None,
        },
        spec: Spec::Prompt(spec),
        relationships: Default::default(),
        status: None,
    };
    format::yaml::to_string(&card).unwrap()
}

#[test]
fn declarative_yaml_deserializes_as_card() {
    let card: Card = format::yaml::from_str(declarative_yaml()).unwrap();
    assert_eq!(card.kind, CardKind::Prompt);
    let Spec::Prompt(spec) = &card.spec else {
        panic!("expected Prompt spec");
    };
    assert_eq!(spec.prompt.model, "gpt-4o");
    assert!(matches!(
        spec.prompt.request,
        ProviderRequest::OpenAiChatCompletion(_)
    ));
    assert!(spec.prompt.variables.contains(&"persona".to_owned()));
    assert!(spec.prompt.variables.contains(&"doc".to_owned()));
    assert_eq!(spec.prompt.response_type, ResponseType::Text);
}

#[test]
fn declarative_yaml_has_no_type_field() {
    let card: Card = format::yaml::from_str(declarative_yaml()).unwrap();
    let roundtripped = format::yaml::to_string(&card).unwrap();
    assert!(
        !roundtripped.contains("type: Prompt"),
        "spec.type must not appear in serialized output"
    );
    assert!(roundtripped.contains("kind: Prompt"));
}

#[test]
fn declarative_temperature_setting_applied() {
    let card: Card = format::yaml::from_str(declarative_yaml()).unwrap();
    let Spec::Prompt(spec) = &card.spec else {
        panic!()
    };
    let ProviderRequest::OpenAiChatCompletion(req) = &spec.prompt.request else {
        panic!()
    };
    assert_eq!(req.settings.temperature, Some(0.2));
}

#[test]
fn native_format_round_trips() {
    let yaml_str = native_yaml("openai", "gpt-4o");
    assert!(yaml_str.contains("kind: Prompt"));
    assert!(!yaml_str.contains("type: Prompt"));
    let decoded: Card = format::yaml::from_str(&yaml_str).unwrap();
    let re_encoded = format::yaml::to_string(&decoded).unwrap();
    let redecoded: Card = format::yaml::from_str(&re_encoded).unwrap();
    assert_eq!(decoded, redecoded);
}

#[test]
fn api_version_default_when_omitted() {
    let yaml = r#"
kind: Prompt
metadata:
  name: test-prompt
  version: 0.1.0
spec:
  provider: openai
  model: gpt-4o
  messages:
    - Hello
"#;
    let card: Card = format::yaml::from_str(yaml).unwrap();
    assert_eq!(card.api_version.as_str(), "wyrd/v1");
}

#[test]
fn malformed_provider_raises_error_not_silent() {
    let yaml = r#"
kind: Prompt
metadata:
  name: test-prompt
  version: 0.1.0
spec:
  provider: badprovider
  model: gpt-4o
  messages:
    - Hello
"#;
    let result: Result<Card, _> = format::yaml::from_str(yaml);
    assert!(result.is_err(), "unknown provider must produce an error");
}

#[test]
fn anthropic_declarative_compiles() {
    let yaml = r#"
kind: Prompt
metadata:
  name: test-anthropic
  version: 0.1.0
spec:
  provider: anthropic
  model: claude-3-5-sonnet-20241022
  system: You are a helpful assistant.
  messages:
    - What is 2+2?
"#;
    let card: Card = format::yaml::from_str(yaml).unwrap();
    let Spec::Prompt(spec) = &card.spec else {
        panic!()
    };
    assert!(matches!(
        spec.prompt.request,
        ProviderRequest::AnthropicMessage(_)
    ));
}
