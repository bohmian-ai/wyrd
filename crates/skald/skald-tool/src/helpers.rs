//! Provider-native tool-use and tool-result helper constructors.

use skald_spec::wire::anthropic_messages::{AnthropicContentBlock, AnthropicToolResultContent};
use skald_spec::wire::google_generate::{GoogleFunctionCall, GoogleFunctionResponse, GooglePart};
use skald_spec::wire::openai_chat::{
    OpenAiChatMessage, OpenAiMessageContent, OpenAiToolCall, OpenAiToolFunctionCall,
};

/// Creates an OpenAI assistant tool call payload.
pub fn openai_function_tool_call(
    id: impl Into<String>,
    name: impl Into<String>,
    arguments: serde_json::Value,
) -> OpenAiToolCall {
    OpenAiToolCall {
        id: id.into(),
        kind: "function".to_owned(),
        function: OpenAiToolFunctionCall {
            name: name.into(),
            arguments: arguments.to_string(),
        },
    }
}

/// Creates an OpenAI tool-result chat message.
pub fn openai_tool_result_message(
    tool_call_id: impl Into<String>,
    content: impl Into<String>,
) -> OpenAiChatMessage {
    OpenAiChatMessage {
        role: "tool".to_owned(),
        content: Some(OpenAiMessageContent::Text(content.into())),
        name: None,
        tool_calls: None,
        tool_call_id: Some(tool_call_id.into()),
        refusal: None,
    }
}

/// Creates an Anthropic tool_use content block.
pub fn anthropic_tool_use_block(
    id: impl Into<String>,
    name: impl Into<String>,
    input: serde_json::Value,
) -> AnthropicContentBlock {
    AnthropicContentBlock::ToolUse {
        id: id.into(),
        name: name.into(),
        input,
    }
}

/// Creates an Anthropic tool_result content block.
pub fn anthropic_tool_result_block(
    tool_use_id: impl Into<String>,
    content: impl Into<String>,
    is_error: Option<bool>,
) -> AnthropicContentBlock {
    AnthropicContentBlock::ToolResult {
        tool_use_id: tool_use_id.into(),
        content: AnthropicToolResultContent::Text(content.into()),
        is_error,
        cache_control: None,
    }
}

/// Creates a Google function_call part.
pub fn google_function_call_part(name: impl Into<String>, args: serde_json::Value) -> GooglePart {
    GooglePart::FunctionCall {
        function_call: GoogleFunctionCall {
            name: name.into(),
            args,
        },
    }
}

/// Creates a Google function_response part.
pub fn google_function_response_part(
    name: impl Into<String>,
    response: serde_json::Value,
) -> GooglePart {
    GooglePart::FunctionResponse {
        function_response: GoogleFunctionResponse {
            name: name.into(),
            response,
        },
    }
}
