use skald_runtime::{MockProvider, Provider, default_registry};
use skald_spec::wire::openai_chat::{
    OpenAiChatChoice, OpenAiChatMessage, OpenAiChatRequest, OpenAiChatResponse, OpenAiChatSettings,
    OpenAiMessageContent,
};
use skald_spec::{ProviderRequest, ProviderResponse};

#[tokio::test]
async fn test_echo_returns_user_input_verbatim() {
    let response = MockProvider::echo()
        .send(openai_request("hello world"))
        .await
        .expect("mock echoes");

    assert_eq!(response.adapter().text().as_deref(), Some("hello world"));
}

#[tokio::test]
async fn test_default_registry_has_mock_provider() {
    let providers = default_registry();
    let provider = providers
        .get(&skald_spec::ProviderName::Custom("mock".to_owned()))
        .expect("mock provider registered");

    let response = provider
        .send(openai_request("hello world"))
        .await
        .expect("mock echoes");

    assert_eq!(response.adapter().text().as_deref(), Some("hello world"));
}

#[tokio::test]
async fn test_scripted_expectation_overrides_echo() {
    let expected = openai_request("hello world");
    let provider = MockProvider::echo()
        .expect_request(expected.clone())
        .respond_with(openai_text_response("custom"));

    let scripted = provider
        .send(expected)
        .await
        .expect("scripted response wins");
    let echo = provider
        .send(openai_request("fallback"))
        .await
        .expect("echo fallback works");

    assert_eq!(scripted.adapter().text().as_deref(), Some("custom"));
    assert_eq!(echo.adapter().text().as_deref(), Some("fallback"));
}

fn openai_request(text: &str) -> ProviderRequest {
    ProviderRequest::OpenAiChatCompletion(OpenAiChatRequest {
        model: "mock-model".to_owned(),
        messages: vec![OpenAiChatMessage {
            role: "user".to_owned(),
            content: Some(OpenAiMessageContent::Text(text.to_owned())),
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
    })
}

fn openai_text_response(text: &str) -> ProviderResponse {
    ProviderResponse::OpenAiChatCompletion(OpenAiChatResponse {
        id: "mock_response".to_owned(),
        object: "chat.completion".to_owned(),
        created: 0,
        model: "mock-model".to_owned(),
        choices: vec![OpenAiChatChoice {
            index: 0,
            message: OpenAiChatMessage {
                role: "assistant".to_owned(),
                content: Some(OpenAiMessageContent::Text(text.to_owned())),
                name: None,
                tool_calls: None,
                tool_call_id: None,
                refusal: None,
            },
            finish_reason: Some("stop".to_owned()),
            logprobs: None,
        }],
        usage: None,
        system_fingerprint: None,
        service_tier: None,
    })
}
