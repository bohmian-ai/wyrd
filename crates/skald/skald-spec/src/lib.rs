//! Skald-spec: the one native provider type set, shared end to end.
//!
//! Pure Rust. No IO. No async. No PyO3. No HTTP clients.

pub mod adapter;
pub mod authoring;
pub mod convert;
pub mod error;
pub mod media;
pub mod message;
pub mod prompt;
pub mod request;
pub mod response;
pub mod wire;

pub use adapter::{ResponseAdapter, ToolCallView, UsageView};
pub use authoring::{DraftMessages, PromptDraft};
pub use convert::{ProviderMessageConversion, convert_message_dyn};
pub use error::{SkaldError, SkaldResult};
pub use media::{MediaKind, MediaRef, MediaSource};
pub use message::MessageNum;
pub use prompt::{Prompt, ProviderSettingsRef, ResponseType};
pub use request::{ProviderName, ProviderRequest, ToolDescriptor};
pub use response::ProviderResponse;
pub use wire::anthropic_citation::AnthropicCitationV1;
pub use wire::anthropic_messages::{
    AnthropicContentBlock, AnthropicMessage, AnthropicMessagesRequest, AnthropicMessagesResponse,
    AnthropicMessagesSettings, AnthropicStopReason, AnthropicStreamEvent, AnthropicUsage,
};
pub use wire::common::{FinishReason, TokenUsage};
pub use wire::google_embeddings::{
    GoogleBatchEmbedRequest, GoogleBatchEmbedResponse, GoogleEmbedContent, GoogleEmbedPart,
    GoogleEmbedRequest, GoogleEmbedding,
};
pub use wire::google_generate::{
    GoogleCandidate, GoogleContent, GoogleFinishReason, GoogleGenerateContentRequest,
    GoogleGenerateContentResponse, GoogleGenerateSettings, GooglePart, GoogleSafetyRating,
    GoogleUsageMetadata,
};
pub use wire::openai_chat::{
    OpenAiChatChoice, OpenAiChatChoiceDelta, OpenAiChatLogprobs, OpenAiChatMessage,
    OpenAiChatRequest, OpenAiChatResponse, OpenAiChatSettings, OpenAiChatStreamChunk,
    OpenAiToolCall, OpenAiUsage,
};
pub use wire::openai_embeddings::{
    OpenAiEmbeddingVector, OpenAiEmbeddingsInput, OpenAiEmbeddingsRequest, OpenAiEmbeddingsResponse,
};
pub use wire::openai_responses::{
    OpenAiResponseItem, OpenAiResponsesRequest, OpenAiResponsesResponse, OpenAiResponsesSettings,
    OpenAiResponsesStreamEvent,
};
pub use wire::vertex_generate::VertexGenerateContentRequest;
pub use wire::vertex_predict::{VertexPredictRequest, VertexPredictResponse, VertexPrediction};

#[cfg(test)]
pub(crate) mod common {
    #![allow(dead_code)]

    use serde::de::DeserializeOwned;
    use serde_json::{Value, json, value::RawValue};

    use crate::wire::anthropic_messages::*;
    use crate::wire::google_generate::*;
    use crate::wire::openai_chat::*;
    use crate::wire::openai_responses::*;
    use crate::wire::vertex_generate::VertexGenerateContentRequest;
    use crate::wire::vertex_predict::*;
    use crate::{MessageNum, ProviderName, ProviderRequest, ProviderResponse};

    pub(crate) fn typed<T: DeserializeOwned>(value: Value) -> T {
        serde_json::from_value(value).expect("fixture should match skald-spec wire type")
    }

    pub(crate) fn raw_value(value: Value) -> Box<RawValue> {
        RawValue::from_string(value.to_string()).expect("fixture should produce raw JSON")
    }

    pub(crate) fn openai_chat_request() -> OpenAiChatRequest {
        typed(json!({
            "model": "gpt-4o",
            "messages": [
                {"role": "system", "content": "You answer {{topic}}."},
                {"role": "user", "content": [{"type": "text", "text": "Hello {{name}}"}]}
            ],
            "temperature": 0.2,
            "tools": [{
                "type": "function",
                "function": {
                    "name": "lookup",
                    "description": "Lookup a thing",
                    "parameters": {"type": "object", "properties": {"q": {"type": "string"}}},
                    "strict": true
                }
            }],
            "tool_choice": {"type": "function", "function": {"name": "lookup"}},
            "response_format": {
                "type": "json_schema",
                "json_schema": {
                    "name": "answer",
                    "schema": {"type": "object", "properties": {"answer": {"type": "string"}}},
                    "strict": true
                }
            },
            "prompt_cache_key": "cache-key"
        }))
    }

    pub(crate) fn openai_chat_response() -> OpenAiChatResponse {
        typed(json!({
            "id": "chatcmpl_123",
            "object": "chat.completion",
            "created": 1740000000,
            "model": "gpt-4o",
            "choices": [{
                "index": 0,
                "message": {
                    "role": "assistant",
                    "content": "{\"answer\":\"hello\"}",
                    "tool_calls": [{
                        "id": "call_1",
                        "type": "function",
                        "function": {"name": "lookup", "arguments": "{\"q\":\"hello\"}"}
                    }]
                },
                "finish_reason": "tool_calls",
                "logprobs": {"content": [], "refusal": []}
            }],
            "usage": {
                "prompt_tokens": 10,
                "completion_tokens": 5,
                "total_tokens": 15,
                "prompt_tokens_details": {"audio_tokens": 0, "cached_tokens": 4},
                "completion_tokens_details": {
                    "accepted_prediction_tokens": 0,
                    "audio_tokens": 0,
                    "reasoning_tokens": 3,
                    "rejected_prediction_tokens": 0
                }
            }
        }))
    }

    pub(crate) fn openai_responses_request() -> OpenAiResponsesRequest {
        typed(json!({
            "model": "gpt-4o",
            "input": [
                {"type": "message", "role": "user", "content": [{"type": "input_text", "text": "Hi {{name}}"}]},
                {"type": "function_call", "call_id": "call_1", "name": "lookup", "arguments": "{\"q\":\"x\"}"},
                {"type": "function_call_output", "call_id": "call_1", "output": "ok"},
                {"type": "reasoning", "summary": "short", "encrypted_content": "abc"},
                {"type": "input_file", "file_id": "file_1"},
                {"type": "input_image", "image_url": "https://example.com/image.png", "detail": "low"}
            ],
            "instructions": "Answer tersely.",
            "reasoning": {"effort": "low", "summary": "auto"},
            "text": {"format": {"type": "json_schema", "name": "answer", "schema": {"type": "object"}, "strict": true}},
            "tools": [{"type": "function", "name": "lookup", "parameters": {"type": "object"}, "strict": true}],
            "tool_choice": {"type": "function", "name": "lookup"},
            "previous_response_id": "resp_prev"
        }))
    }

    pub(crate) fn openai_responses_response() -> OpenAiResponsesResponse {
        typed(json!({
            "id": "resp_123",
            "object": "response",
            "model": "gpt-4o",
            "status": "completed",
            "created_at": 1740000001,
            "output": [
                {"type": "message", "role": "assistant", "content": [{"type": "output_text", "text": "{\"answer\":\"hello\"}"}]},
                {"type": "function_call", "call_id": "call_1", "name": "lookup", "arguments": "{\"q\":\"hello\"}"}
            ],
            "usage": {
                "input_tokens": 9,
                "input_tokens_details": {"cached_tokens": 2},
                "output_tokens": 6,
                "output_tokens_details": {"reasoning_tokens": 1},
                "total_tokens": 15
            },
            "previous_response_id": null
        }))
    }

    pub(crate) fn anthropic_request() -> AnthropicMessagesRequest {
        typed(json!({
            "model": "claude-opus-4-1",
            "max_tokens": 256,
            "system": [{"type": "text", "text": "System {{topic}}", "cache_control": {"type": "ephemeral", "ttl": "5m"}}],
            "messages": [{
                "role": "user",
                "content": [
                    {"type": "text", "text": "Hello {{name}}"},
                    {"type": "image", "source": {"type": "url", "url": "https://example.com/image.png"}},
                    {"type": "document", "source": {"type": "text", "media_type": "text/plain", "data": "doc"}},
                    {"type": "tool_result", "tool_use_id": "tool_1", "content": "done"}
                ]
            }],
            "tools": [{"name": "lookup", "description": "Lookup", "input_schema": {"type": "object"}}],
            "thinking": {"type": "enabled", "budget_tokens": 128}
        }))
    }

    pub(crate) fn anthropic_response(
        stop_reason: AnthropicStopReason,
    ) -> AnthropicMessagesResponse {
        AnthropicMessagesResponse {
            id: "msg_123".to_string(),
            r#type: "message".to_string(),
            role: "assistant".to_string(),
            model: "claude-opus-4-1".to_string(),
            content: vec![
                AnthropicContentBlock::Text {
                    text: "hello".to_string(),
                    cache_control: None,
                    citations: None,
                },
                AnthropicContentBlock::ToolUse {
                    id: "tool_1".to_string(),
                    name: "lookup".to_string(),
                    input: json!({"q": "hello"}),
                },
            ],
            stop_reason: Some(stop_reason),
            stop_sequence: None,
            usage: AnthropicUsage {
                input_tokens: 11,
                output_tokens: 7,
                cache_creation_input_tokens: 3,
                cache_read_input_tokens: 4,
            },
        }
    }

    pub(crate) fn google_request() -> GoogleGenerateContentRequest {
        typed(json!({
            "contents": [{
                "role": "user",
                "parts": [
                    {"text": "Hello {{name}}"},
                    {"inline_data": {"mime_type": "image/png", "data": "abcd"}},
                    {"file_data": {"mime_type": "image/png", "file_uri": "gs://bucket/image.png"}},
                    {"function_call": {"name": "lookup", "args": {"q": "hello"}}},
                    {"function_response": {"name": "lookup", "response": {"ok": true}}}
                ]
            }],
            "generation_config": {
                "temperature": 0.1,
                "max_output_tokens": 64,
                "response_mime_type": "application/json",
                "response_schema": {"type": "object"}
            },
            "safety_settings": [{"category": "HARM_CATEGORY_DANGEROUS_CONTENT", "threshold": "BLOCK_MEDIUM_AND_ABOVE"}],
            "tools": [{"function_declarations": [{"name": "lookup", "parameters": {"type": "object"}}]}]
        }))
    }

    pub(crate) fn google_response(
        finish_reason: GoogleFinishReason,
    ) -> GoogleGenerateContentResponse {
        GoogleGenerateContentResponse {
            candidates: vec![GoogleCandidate {
                content: GoogleContent {
                    role: "model".to_string(),
                    parts: vec![
                        GooglePart::Text {
                            text: "{\"answer\":\"hello\"}".to_string(),
                        },
                        GooglePart::FunctionCall {
                            function_call: GoogleFunctionCall {
                                name: "lookup".to_string(),
                                args: json!({"q": "hello"}),
                            },
                        },
                    ],
                },
                finish_reason: Some(finish_reason),
                index: Some(0),
                safety_ratings: vec![GoogleSafetyRating {
                    category: "HARM_CATEGORY_DANGEROUS_CONTENT".to_string(),
                    probability: "NEGLIGIBLE".to_string(),
                    blocked: Some(false),
                }],
                citation_metadata: None,
                grounding_metadata: None,
                avg_logprobs: None,
            }],
            usage_metadata: Some(GoogleUsageMetadata {
                prompt_token_count: 8,
                candidates_token_count: 5,
                total_token_count: 13,
                cached_content_token_count: 2,
                thoughts_token_count: 1,
            }),
            model_version: Some("gemini-2.5-pro".to_string()),
            prompt_feedback: None,
        }
    }

    pub(crate) fn vertex_generate_request() -> VertexGenerateContentRequest {
        VertexGenerateContentRequest(google_request())
    }

    pub(crate) fn vertex_predict_request() -> VertexPredictRequest {
        typed(json!({
            "instances": [{"content": "embed this", "task_type": "RETRIEVAL_QUERY", "title": "query"}],
            "parameters": {"auto_truncate": true, "output_dimensionality": 3}
        }))
    }

    pub(crate) fn vertex_predict_response() -> VertexPredictResponse {
        typed(json!({
            "predictions": [{"embeddings": {"values": [0.1, 0.2, 0.3], "statistics": {"truncated": false}}}],
            "metadata": {"billable_character_count": 10}
        }))
    }

    pub(crate) fn openai_message() -> OpenAiChatMessage {
        typed(json!({
            "role": "assistant",
            "content": [{"type": "text", "text": "hello"}],
            "tool_calls": [{"id": "call_1", "type": "function", "function": {"name": "lookup", "arguments": "{\"q\":\"hello\"}"}}]
        }))
    }

    pub(crate) fn anthropic_message() -> AnthropicMessage {
        typed(json!({
            "role": "assistant",
            "content": [
                {"type": "text", "text": "hello"},
                {"type": "tool_use", "id": "tool_1", "name": "lookup", "input": {"q": "hello"}}
            ]
        }))
    }

    pub(crate) fn google_message() -> GoogleContent {
        typed(json!({
            "role": "model",
            "parts": [
                {"text": "hello"},
                {"function_call": {"name": "lookup", "args": {"q": "hello"}}}
            ]
        }))
    }

    pub(crate) fn provider_request_variants() -> Vec<ProviderRequest> {
        vec![
            ProviderRequest::OpenAiChatCompletion(openai_chat_request()),
            ProviderRequest::OpenAiResponses(openai_responses_request()),
            ProviderRequest::AnthropicMessage(anthropic_request()),
            ProviderRequest::GeminiGenerateContent(google_request()),
        ]
    }

    pub(crate) fn provider_response_variants() -> Vec<ProviderResponse> {
        vec![
            ProviderResponse::OpenAiChatCompletion(openai_chat_response()),
            ProviderResponse::OpenAiResponses(openai_responses_response()),
            ProviderResponse::AnthropicMessage(anthropic_response(AnthropicStopReason::EndTurn)),
            ProviderResponse::GeminiGenerateContent(google_response(GoogleFinishReason::Stop)),
        ]
    }

    pub(crate) fn message_variants() -> Vec<MessageNum> {
        vec![
            MessageNum::OpenAi(Box::new(openai_message())),
            MessageNum::Anthropic(anthropic_message()),
            MessageNum::Gemini(google_message()),
        ]
    }

    pub(crate) fn raw_request(provider: ProviderName) -> ProviderRequest {
        ProviderRequest::RawV1 {
            provider,
            body: raw_value(json!({"custom": true, "nested": {"x": 1}})),
        }
    }
}
