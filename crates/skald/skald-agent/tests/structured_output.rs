use std::sync::Arc;

use serde_json::{Value, json};
use skald_agent::Agent;
use skald_prompt::{OpenAiChatOptions, ResponseFormat, openai_chat};
use skald_runtime::{MockProvider, ProviderRegistry};
use skald_spec::ProviderResponse;
use skald_spec::wire::openai_chat::{
    OpenAiChatChoice, OpenAiChatMessage, OpenAiChatResponse, OpenAiMessageContent,
};

fn registry_returning_text(text: &str) -> ProviderRegistry {
    let mock = MockProvider::new(skald_spec::ProviderName::OpenAi);
    mock.push_response(openai_text_response(text));
    let mut providers = ProviderRegistry::new();
    providers.register(Arc::new(mock));
    providers
}

fn openai_text_response(text: &str) -> ProviderResponse {
    ProviderResponse::OpenAiChatCompletion(OpenAiChatResponse {
        id: "resp_1".to_owned(),
        object: "chat.completion".to_owned(),
        created: 0,
        model: "gpt-test".to_owned(),
        choices: vec![OpenAiChatChoice {
            index: 0,
            message: OpenAiChatMessage {
                role: "assistant".to_owned(),
                content: Some(OpenAiMessageContent::Text(text.to_owned())),
                ..Default::default()
            },
            finish_reason: Some("stop".to_owned()),
            logprobs: None,
        }],
        usage: None,
        system_fingerprint: None,
        service_tier: None,
    })
}

fn structured_prompt() -> skald_prompt::Prompt {
    let schema = json!({
        "type": "object",
        "properties": {"foo": {"type": "string"}, "bar": {"type": "string"}},
        "required": ["foo", "bar"],
        "additionalProperties": false
    });
    openai_chat(
        "gpt-test",
        OpenAiChatOptions {
            messages: vec!["Say hi.".into()],
            output: Some(ResponseFormat::json_schema("test", schema).unwrap()),
            ..OpenAiChatOptions::default()
        },
    )
    .unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn structured_output_parses_when_schema_declared() {
    let prompt = structured_prompt();
    let agent = Agent::new(prompt);
    let providers = registry_returning_text(r#"{"foo":"hello","bar":"world"}"#);

    let run = agent
        .run_prompt(&providers, &agent.prompt, &[], None)
        .await
        .unwrap();

    let map = run.structured_output.expect("structured output present");
    assert_eq!(map.get("foo"), Some(&Value::String("hello".to_owned())));
    assert_eq!(map.get("bar"), Some(&Value::String("world".to_owned())));
}

#[tokio::test(flavor = "multi_thread")]
async fn structured_output_decode_failure_surfaces_code() {
    let prompt = structured_prompt();
    let agent = Agent::new(prompt);
    let providers = registry_returning_text("not json");

    let err = agent
        .run_prompt(&providers, &agent.prompt, &[], None)
        .await
        .expect_err("non-JSON response must fail");

    assert_eq!(err.code(), "SKALD_AGENT_422_STRUCTURED_DECODE");
}

#[tokio::test(flavor = "multi_thread")]
async fn text_prompt_leaves_structured_output_none() {
    let prompt = openai_chat(
        "gpt-test",
        OpenAiChatOptions {
            messages: vec!["Hi".into()],
            ..OpenAiChatOptions::default()
        },
    )
    .unwrap();
    let agent = Agent::new(prompt);
    let providers = registry_returning_text("hello");

    let run = agent
        .run_prompt(&providers, &agent.prompt, &[], None)
        .await
        .unwrap();

    assert!(run.structured_output.is_none());
    assert_eq!(run.output, "hello");
}
