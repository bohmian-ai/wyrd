use serde_json::json;
use skald_prompt::{OpenAiChatOptions, Prompt, ResponseFormat, openai_chat};
use skald_runtime::{MockProvider, ProviderRegistry};
use skald_spec::ProviderName;
use skald_spec::ProviderResponse;
use skald_spec::wire::openai_chat::{
    OpenAiChatChoice, OpenAiChatMessage, OpenAiChatResponse, OpenAiMessageContent,
};

pub fn plan_prompt() -> Prompt {
    openai_chat(
        "gpt-4o-mini",
        OpenAiChatOptions {
            messages: vec!["Plan research on: ${topic}".to_owned()],
            output: Some(
                ResponseFormat::json_schema(
                    "plan",
                    json!({
                        "type": "object",
                        "properties": {
                            "summary": {"type": "string"},
                            "steps": {"type": "array", "items": {"type": "string"}}
                        },
                        "required": ["summary", "steps"],
                        "additionalProperties": false
                    }),
                )
                .expect("static schema is valid"),
            ),
            ..OpenAiChatOptions::default()
        },
    )
    .expect("static prompt is valid")
}

pub fn write_prompt() -> Prompt {
    openai_chat(
        "gpt-4o-mini",
        OpenAiChatOptions {
            messages: vec!["Draft a brief on: ${summary}".to_owned()],
            ..OpenAiChatOptions::default()
        },
    )
    .expect("static prompt is valid")
}

pub fn mock_registry(responses: &[&str]) -> ProviderRegistry {
    let provider = MockProvider::new(ProviderName::OpenAi);
    for response in responses {
        provider.push_response(openai_text_response(response));
    }
    let mut registry = ProviderRegistry::new();
    registry.register(std::sync::Arc::new(provider));
    registry
}

fn openai_text_response(text: &str) -> ProviderResponse {
    ProviderResponse::OpenAiChatCompletion(OpenAiChatResponse {
        id: "resp_1".to_owned(),
        object: "chat.completion".to_owned(),
        created: 0,
        model: "gpt-4o-mini".to_owned(),
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
