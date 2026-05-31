//! Native message constructors used inside the bounded tool loop.
//!
//! Tool-result and assistant-message envelopes are provider-specific; this
//! module isolates that matching so the loop body stays focused on control
//! flow.

use serde_json::Value;
use serde_json::value::RawValue;
use skald_spec::adapter::ToolCallView;
use skald_spec::wire::anthropic_messages::{
    AnthropicContentBlock, AnthropicMessage, AnthropicToolResultContent,
};
use skald_spec::wire::google_generate::{
    GoogleContent, GoogleFunctionCall, GoogleFunctionResponse, GooglePart,
};
use skald_spec::wire::openai_chat::{
    OpenAiChatMessage, OpenAiMessageContent, OpenAiToolCall, OpenAiToolFunctionCall,
};
use skald_spec::{MessageNum, ProviderName, ProviderResponse};

use crate::error::{AgentError, AgentResult};

/// Build the assistant-side message that announced the tool call(s).
pub fn assistant_message_from_response(
    response: &ProviderResponse,
    provider: &ProviderName,
) -> AgentResult<MessageNum> {
    let view = response.adapter();
    let calls = view.tool_calls();
    let text = view.text().map(|cow| cow.into_owned()).unwrap_or_default();

    match provider {
        ProviderName::OpenAi => Ok(MessageNum::OpenAi(OpenAiChatMessage {
            role: "assistant".to_owned(),
            content: if text.is_empty() {
                None
            } else {
                Some(OpenAiMessageContent::Text(text))
            },
            name: None,
            tool_calls: if calls.is_empty() {
                None
            } else {
                Some(
                    calls
                        .iter()
                        .map(|call| OpenAiToolCall {
                            id: call.id.clone().into_owned(),
                            kind: "function".to_owned(),
                            function: OpenAiToolFunctionCall {
                                name: call.name.clone().into_owned(),
                                arguments: call.arguments.clone().into_owned(),
                            },
                        })
                        .collect(),
                )
            },
            tool_call_id: None,
            refusal: None,
        })),
        ProviderName::Anthropic => {
            let mut blocks = Vec::with_capacity(calls.len() + usize::from(!text.is_empty()));
            if !text.is_empty() {
                blocks.push(AnthropicContentBlock::Text {
                    text,
                    cache_control: None,
                    citations: None,
                });
            }
            for call in &calls {
                let input = serde_json::from_str(call.arguments.as_ref()).unwrap_or(Value::Null);
                blocks.push(AnthropicContentBlock::ToolUse {
                    id: call.id.clone().into_owned(),
                    name: call.name.clone().into_owned(),
                    input,
                });
            }
            Ok(MessageNum::Anthropic(AnthropicMessage {
                role: "assistant".to_owned(),
                content: blocks,
            }))
        }
        ProviderName::Google | ProviderName::Vertex => {
            let mut parts = Vec::with_capacity(calls.len() + usize::from(!text.is_empty()));
            if !text.is_empty() {
                parts.push(GooglePart::Text { text });
            }
            for call in &calls {
                let args = serde_json::from_str(call.arguments.as_ref()).unwrap_or(Value::Null);
                parts.push(GooglePart::FunctionCall {
                    function_call: GoogleFunctionCall {
                        name: call.name.clone().into_owned(),
                        args,
                    },
                });
            }
            Ok(MessageNum::Gemini(GoogleContent {
                role: "model".to_owned(),
                parts,
            }))
        }
        ProviderName::Custom(_) => Err(AgentError::SystemPrompt {
            provider: provider.clone(),
            detail: "custom provider has no canonical assistant message envelope".to_owned(),
        }),
    }
}

/// Build the tool-result message that follows a dispatched call.
pub fn tool_result_message(
    provider: &ProviderName,
    call: &ToolCallView<'_>,
    result: &Result<Box<RawValue>, crate::AgentToolError>,
) -> AgentResult<MessageNum> {
    let (payload, is_error) = match result {
        Ok(raw) => (raw.get().to_owned(), false),
        Err(err) => (
            serde_json::json!({ "error": err.to_string() }).to_string(),
            true,
        ),
    };

    match provider {
        ProviderName::OpenAi => Ok(MessageNum::OpenAi(OpenAiChatMessage {
            role: "tool".to_owned(),
            content: Some(OpenAiMessageContent::Text(payload)),
            name: Some(call.name.clone().into_owned()),
            tool_calls: None,
            tool_call_id: Some(call.id.clone().into_owned()),
            refusal: None,
        })),
        ProviderName::Anthropic => Ok(MessageNum::Anthropic(AnthropicMessage {
            role: "user".to_owned(),
            content: vec![AnthropicContentBlock::ToolResult {
                tool_use_id: call.id.clone().into_owned(),
                content: AnthropicToolResultContent::Text(payload),
                is_error: Some(is_error),
                cache_control: None,
            }],
        })),
        ProviderName::Google | ProviderName::Vertex => {
            let response_value =
                serde_json::from_str(&payload).unwrap_or_else(|_| Value::String(payload.clone()));
            let response = if response_value.is_object() {
                response_value
            } else {
                serde_json::json!({ "result": response_value })
            };
            Ok(MessageNum::Gemini(GoogleContent {
                role: "user".to_owned(),
                parts: vec![GooglePart::FunctionResponse {
                    function_response: GoogleFunctionResponse {
                        name: call.name.clone().into_owned(),
                        response,
                    },
                }],
            }))
        }
        ProviderName::Custom(_) => Err(AgentError::SystemPrompt {
            provider: provider.clone(),
            detail: "custom provider has no canonical tool-result envelope".to_owned(),
        }),
    }
}
