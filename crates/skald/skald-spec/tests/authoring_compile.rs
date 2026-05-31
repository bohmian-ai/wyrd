use skald_spec::authoring::{DraftMessages, PromptDraft};
use skald_spec::{ProviderName, ProviderRequest};

fn draft(provider: &str, model: &str, messages: Vec<&str>) -> PromptDraft {
    PromptDraft {
        provider: provider.to_owned(),
        model: model.to_owned(),
        operation: None,
        system: None,
        messages: DraftMessages::Many(messages.into_iter().map(str::to_owned).collect()),
        model_settings: None,
        version: None,
    }
}

#[test]
fn compile_openai_chat() {
    let prompt = draft("openai", "gpt-4o", vec!["Hello"])
        .compile()
        .expect("openai chat compiles");
    assert_eq!(prompt.model, "gpt-4o");
    assert!(matches!(
        prompt.request,
        ProviderRequest::OpenAiChatCompletion(_)
    ));
}

#[test]
fn compile_openai_responses() {
    let mut d = draft("openai", "gpt-4o", vec!["Hello"]);
    d.operation = Some("responses".to_owned());
    let prompt = d.compile().expect("openai responses compiles");
    assert!(matches!(
        prompt.request,
        ProviderRequest::OpenAiResponses(_)
    ));
}

#[test]
fn compile_anthropic() {
    let prompt = draft("anthropic", "claude-3-5-sonnet-20241022", vec!["Hello"])
        .compile()
        .expect("anthropic compiles");
    assert_eq!(prompt.request.provider(), ProviderName::Anthropic);
}

#[test]
fn compile_google() {
    let prompt = draft("google", "gemini-2.0-flash-exp", vec!["Hello"])
        .compile()
        .expect("google compiles");
    assert!(matches!(
        prompt.request,
        ProviderRequest::GeminiGenerateContent(_)
    ));
}

#[test]
fn compile_vertex() {
    let prompt = draft("vertex", "gemini-2.0-flash-exp", vec!["Hello"])
        .compile()
        .expect("vertex compiles");
    assert!(matches!(prompt.request, ProviderRequest::Vertex(_)));
}

#[test]
fn unknown_provider_returns_draft_invalid() {
    let result = draft("llama", "llama-3.1-8b", vec!["Hello"]).compile();
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert_eq!(err.code(), "SKALD_SPEC_400_PROMPT_DRAFT_INVALID");
}

#[test]
fn single_message_string_deserialized() {
    let yaml = r#"
provider: openai
model: gpt-4o
messages: "Just one message"
"#;
    let draft: PromptDraft = serde_yaml::from_str(yaml).unwrap();
    let prompt = draft.compile().expect("single message string compiles");
    assert!(matches!(
        prompt.request,
        ProviderRequest::OpenAiChatCompletion(_)
    ));
}

#[test]
fn model_settings_temperature_decoded() {
    let yaml = r#"
provider: openai
model: gpt-4o
messages:
  - Hello
model_settings:
  temperature: 0.2
"#;
    let draft: PromptDraft = serde_yaml::from_str(yaml).unwrap();
    let prompt = draft.compile().expect("settings compile");
    let ProviderRequest::OpenAiChatCompletion(req) = &prompt.request else {
        panic!("expected openai chat request");
    };
    assert_eq!(req.settings.temperature, Some(0.2));
}

#[test]
fn variables_extracted_from_template() {
    let yaml = r#"
provider: openai
model: gpt-4o
messages:
  - "Summarize {{doc}} for {{user}}"
"#;
    let draft: PromptDraft = serde_yaml::from_str(yaml).unwrap();
    let prompt = draft.compile().expect("variable extraction works");
    assert!(prompt.variables.contains(&"doc".to_owned()));
    assert!(prompt.variables.contains(&"user".to_owned()));
}

#[test]
fn system_message_openai_chat() {
    let yaml = r#"
provider: openai
model: gpt-4o
system: "You are {{persona}}."
messages:
  - "Summarize {{doc}}."
"#;
    let draft: PromptDraft = serde_yaml::from_str(yaml).unwrap();
    let prompt = draft.compile().expect("system + messages compile");
    let ProviderRequest::OpenAiChatCompletion(req) = &prompt.request else {
        panic!("expected openai chat");
    };
    assert_eq!(req.messages[0].role, "system");
    assert!(prompt.variables.contains(&"persona".to_owned()));
    assert!(prompt.variables.contains(&"doc".to_owned()));
}
