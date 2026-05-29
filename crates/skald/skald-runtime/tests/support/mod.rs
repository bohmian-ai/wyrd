#![allow(dead_code)]

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use serde_json::{Map, json};
use skald_cache::InMemoryCache;
use skald_runtime::{MockProvider, ProviderRegistry, RuntimeConfig, SkaldRuntime};
use skald_spec::wire::anthropic_messages::{AnthropicSystem, AnthropicUsage};
use skald_spec::wire::openai_chat::OpenAiMessageContent;
use skald_spec::{
    AnthropicContentBlock, AnthropicMessage, AnthropicMessagesRequest, AnthropicMessagesResponse,
    AnthropicStopReason, GoogleCandidate, GoogleContent, GoogleFinishReason,
    GoogleGenerateContentRequest, GoogleGenerateContentResponse, GooglePart, GoogleUsageMetadata,
    OpenAiChatChoice, OpenAiChatMessage, OpenAiChatRequest, OpenAiChatResponse, OpenAiUsage,
    ProviderRequest, ProviderResponse,
};

pub fn runtime_with(mock: &MockProvider) -> SkaldRuntime {
    let mut registry = ProviderRegistry::new();
    registry.register(Arc::new(mock.clone()));
    let cache = Arc::new(InMemoryCache::new(8, Duration::from_secs(60)));
    SkaldRuntime::new(registry, cache, RuntimeConfig::default())
}

pub fn openai_request(text: &str) -> ProviderRequest {
    ProviderRequest::OpenAiChatCompletion(OpenAiChatRequest {
        model: "gpt-4o".to_owned(),
        messages: vec![OpenAiChatMessage {
            role: "user".to_owned(),
            content: Some(OpenAiMessageContent::Text(text.to_owned())),
            name: None,
            tool_calls: None,
            tool_call_id: None,
            refusal: None,
        }],
        temperature: None,
        top_p: None,
        max_tokens: None,
        max_completion_tokens: None,
        n: None,
        stop: None,
        presence_penalty: None,
        frequency_penalty: None,
        seed: None,
        logit_bias: None,
        user: None,
        reasoning_effort: None,
        modalities: None,
        audio: None,
        prediction: None,
        response_format: None,
        stream: None,
        stream_options: None,
        tools: None,
        tool_choice: None,
        parallel_tool_calls: None,
        prompt_cache_key: None,
        service_tier: None,
        safety_identifier: None,
        store: None,
        metadata: None,
        logprobs: None,
        top_logprobs: None,
        extra: Map::new(),
    })
}

pub fn openai_response(text: &str) -> ProviderResponse {
    ProviderResponse::OpenAiChatCompletion(OpenAiChatResponse {
        id: "chatcmpl-test".to_owned(),
        object: "chat.completion".to_owned(),
        created: 1,
        model: "gpt-4o".to_owned(),
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
        usage: Some(OpenAiUsage {
            prompt_tokens: 3,
            completion_tokens: 5,
            total_tokens: 8,
            prompt_tokens_details: None,
            completion_tokens_details: None,
        }),
        system_fingerprint: None,
        service_tier: None,
    })
}

pub fn anthropic_request(text: &str) -> ProviderRequest {
    ProviderRequest::AnthropicMessage(AnthropicMessagesRequest {
        model: "claude-sonnet-4-5".to_owned(),
        messages: vec![AnthropicMessage {
            role: "user".to_owned(),
            content: vec![AnthropicContentBlock::Text {
                text: text.to_owned(),
                cache_control: None,
                citations: None,
            }],
        }],
        max_tokens: 128,
        system: Some(AnthropicSystem::Text("Answer directly.".to_owned())),
        temperature: None,
        top_p: None,
        top_k: None,
        stop_sequences: None,
        metadata: None,
        stream: None,
        tools: None,
        tool_choice: None,
        thinking: None,
        extra: Map::new(),
    })
}

pub fn anthropic_response(text: &str) -> ProviderResponse {
    ProviderResponse::AnthropicMessage(AnthropicMessagesResponse {
        id: "msg_test".to_owned(),
        r#type: "message".to_owned(),
        role: "assistant".to_owned(),
        model: "claude-sonnet-4-5".to_owned(),
        content: vec![AnthropicContentBlock::Text {
            text: text.to_owned(),
            cache_control: None,
            citations: None,
        }],
        stop_reason: Some(AnthropicStopReason::EndTurn),
        stop_sequence: None,
        usage: AnthropicUsage {
            input_tokens: 4,
            output_tokens: 6,
            cache_creation_input_tokens: 0,
            cache_read_input_tokens: 0,
        },
    })
}

pub fn google_request(text: &str) -> ProviderRequest {
    ProviderRequest::GeminiGenerateContent(GoogleGenerateContentRequest {
        contents: vec![GoogleContent {
            role: "user".to_owned(),
            parts: vec![GooglePart::Text {
                text: text.to_owned(),
            }],
        }],
        system_instruction: None,
        generation_config: None,
        safety_settings: None,
        tools: None,
        tool_config: None,
        cached_content: None,
        labels: None,
    })
}

pub fn google_response(text: &str) -> ProviderResponse {
    ProviderResponse::GeminiGenerateContent(GoogleGenerateContentResponse {
        candidates: vec![GoogleCandidate {
            content: GoogleContent {
                role: "model".to_owned(),
                parts: vec![GooglePart::Text {
                    text: text.to_owned(),
                }],
            },
            finish_reason: Some(GoogleFinishReason::Stop),
            index: Some(0),
            safety_ratings: Vec::new(),
            citation_metadata: None,
            grounding_metadata: None,
            avg_logprobs: None,
        }],
        usage_metadata: Some(GoogleUsageMetadata {
            prompt_token_count: 2,
            candidates_token_count: 7,
            total_token_count: 9,
            cached_content_token_count: 0,
            thoughts_token_count: 0,
        }),
        model_version: None,
        prompt_feedback: None,
    })
}

pub fn openai_request_with_options() -> ProviderRequest {
    let mut request = match openai_request("with options") {
        ProviderRequest::OpenAiChatCompletion(request) => request,
        _ => unreachable!("helper returns OpenAI chat"),
    };
    request.prompt_cache_key = Some("cache-key".to_owned());
    request
        .extra
        .insert("extra_body".to_owned(), json!({"enabled": true}));
    request.metadata = Some(BTreeMap::from([("purpose".to_owned(), "test".to_owned())]));
    ProviderRequest::OpenAiChatCompletion(request)
}

pub fn google_request_with_options() -> ProviderRequest {
    let mut request = match google_request("with cached content") {
        ProviderRequest::GeminiGenerateContent(request) => request,
        _ => unreachable!("helper returns Google generate"),
    };
    request.cached_content = Some("cachedContents/abc".to_owned());
    ProviderRequest::GeminiGenerateContent(request)
}
