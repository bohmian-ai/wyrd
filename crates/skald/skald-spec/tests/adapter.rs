mod common;

use serde_json::json;
use skald_spec::wire::anthropic_messages::AnthropicStopReason;
use skald_spec::wire::google_generate::GoogleFinishReason;
use skald_spec::wire::openai_chat::OpenAiMessageContent;
use skald_spec::{FinishReason, ProviderResponse};

#[test]
fn openai_chat_adapter_reads_text_tool_calls_structured_usage_and_finish() {
    let response = ProviderResponse::OpenAiChatCompletion(common::openai_chat_response());
    let adapter = response.adapter();

    assert_eq!(adapter.text().unwrap(), "{\"answer\":\"hello\"}");
    assert_eq!(adapter.tool_calls()[0].name, "lookup");
    assert_eq!(
        adapter.structured_output().unwrap().get(),
        "{\"answer\":\"hello\"}"
    );
    assert_eq!(adapter.usage().unwrap().usage.cache_read_input_tokens, 4);
    assert_eq!(adapter.usage().unwrap().usage.reasoning_tokens, 3);
    assert_eq!(adapter.finish_reason(), FinishReason::ToolCalls);
}

#[test]
fn openai_chat_adapter_borrows_contiguous_text() {
    let mut response = common::openai_chat_response();
    response.choices[0].message.tool_calls = None;
    response.choices[0].message.content = Some(OpenAiMessageContent::Text("plain".to_string()));
    let response = ProviderResponse::OpenAiChatCompletion(response);
    let adapter = response.adapter();
    assert!(matches!(
        adapter.text().unwrap(),
        std::borrow::Cow::Borrowed(_)
    ));
}

#[test]
fn openai_chat_finish_reason_maps() {
    let mut response = common::openai_chat_response();
    response.choices[0].finish_reason = Some("stop".to_string());
    assert_eq!(
        ProviderResponse::OpenAiChatCompletion(response)
            .adapter()
            .finish_reason(),
        FinishReason::Stop
    );
}

#[test]
fn openai_responses_adapter_reads_text_tool_calls_structured_usage_and_finish() {
    let response = ProviderResponse::OpenAiResponses(common::openai_responses_response());
    let adapter = response.adapter();
    assert_eq!(adapter.text().unwrap(), "{\"answer\":\"hello\"}");
    assert_eq!(adapter.tool_calls()[0].arguments, "{\"q\":\"hello\"}");
    assert_eq!(
        adapter.structured_output().unwrap().get(),
        "{\"answer\":\"hello\"}"
    );
    assert_eq!(adapter.usage().unwrap().usage.cache_read_input_tokens, 2);
    assert_eq!(adapter.finish_reason(), FinishReason::ToolCalls);
}

#[test]
fn anthropic_adapter_reads_text_tool_calls_usage_and_finish() {
    let response = ProviderResponse::AnthropicMessage(common::anthropic_response(
        AnthropicStopReason::ToolUse,
    ));
    let adapter = response.adapter();
    assert_eq!(adapter.text().unwrap(), "hello");
    assert_eq!(
        adapter.tool_calls()[0].arguments,
        json!({"q": "hello"}).to_string()
    );
    assert_eq!(
        adapter.usage().unwrap().usage.cache_creation_input_tokens,
        3
    );
    assert_eq!(adapter.usage().unwrap().usage.cache_read_input_tokens, 4);
    assert_eq!(adapter.finish_reason(), FinishReason::ToolCalls);
}

#[test]
fn anthropic_finish_reason_mapping() {
    for (reason, expected) in [
        (AnthropicStopReason::EndTurn, FinishReason::Stop),
        (AnthropicStopReason::ToolUse, FinishReason::ToolCalls),
        (AnthropicStopReason::MaxTokens, FinishReason::Length),
        (AnthropicStopReason::Refusal, FinishReason::ContentFilter),
    ] {
        assert_eq!(
            ProviderResponse::AnthropicMessage(common::anthropic_response(reason))
                .adapter()
                .finish_reason(),
            expected
        );
    }
}

#[test]
fn google_adapter_reads_text_tool_calls_usage_and_finish() {
    let response =
        ProviderResponse::GeminiGenerateContent(common::google_response(GoogleFinishReason::Stop));
    let adapter = response.adapter();
    assert_eq!(adapter.text().unwrap(), "{\"answer\":\"hello\"}");
    assert_eq!(adapter.tool_calls()[0].name, "lookup");
    assert_eq!(
        adapter.structured_output().unwrap().get(),
        "{\"answer\":\"hello\"}"
    );
    assert_eq!(adapter.usage().unwrap().usage.cache_read_input_tokens, 2);
    assert_eq!(adapter.usage().unwrap().usage.reasoning_tokens, 1);
    assert_eq!(adapter.finish_reason(), FinishReason::Stop);
}

#[test]
fn google_finish_reason_mapping() {
    for (reason, expected) in [
        (GoogleFinishReason::Stop, FinishReason::Stop),
        (GoogleFinishReason::MaxTokens, FinishReason::Length),
        (GoogleFinishReason::Safety, FinishReason::ContentFilter),
    ] {
        assert_eq!(
            ProviderResponse::GeminiGenerateContent(common::google_response(reason))
                .adapter()
                .finish_reason(),
            expected
        );
    }
}
