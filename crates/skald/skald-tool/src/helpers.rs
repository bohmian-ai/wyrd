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
        tool_call_id: Some(tool_call_id.into()),
        ..Default::default()
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

#[cfg(test)]
mod tool_use_result {
    use crate::{
        anthropic_tool_result_block, anthropic_tool_use_block, google_function_call_part,
        google_function_response_part, openai_function_tool_call, openai_tool_result_message,
    };
    use serde_json::json;
    use skald_spec::MessageNum;
    use skald_spec::wire::anthropic_messages::{AnthropicContentBlock, AnthropicMessage};
    use skald_spec::wire::google_generate::GoogleContent;
    use skald_spec::wire::openai_chat::OpenAiChatMessage;

    #[test]
    fn openai_tool_call_and_result_round_trip_through_message_num() {
        let call =
            openai_function_tool_call("call_1", "lookup_weather", json!({ "city": "Austin" }));
        let assistant = OpenAiChatMessage {
            role: "assistant".to_owned(),
            content: None,
            name: None,
            tool_calls: Some(vec![call.clone()]),
            tool_call_id: None,
            refusal: None,
            annotations: Vec::new(),
            audio: None,
        };
        let result = openai_tool_result_message("call_1", "{\"forecast\":\"sunny\"}");

        assert_eq!(call.function.name, "lookup_weather");
        assert_round_trips(MessageNum::OpenAi(Box::new(assistant)));
        assert_round_trips(MessageNum::OpenAi(Box::new(result)));
    }

    #[test]
    fn anthropic_tool_use_and_result_round_trip_through_message_num() {
        let use_block =
            anthropic_tool_use_block("toolu_1", "lookup_weather", json!({ "city": "Austin" }));
        let result_block = anthropic_tool_result_block("toolu_1", "sunny", Some(false));
        let message = AnthropicMessage {
            role: "user".to_owned(),
            content: vec![use_block, result_block],
        };

        assert!(matches!(
            &message.content[0],
            AnthropicContentBlock::ToolUse { name, .. } if name == "lookup_weather"
        ));
        assert_round_trips(MessageNum::Anthropic(message));
    }

    #[test]
    fn google_function_call_and_response_round_trip_through_message_num() {
        let content = GoogleContent {
            role: "model".to_owned(),
            parts: vec![
                google_function_call_part("lookup_weather", json!({ "city": "Austin" })),
                google_function_response_part("lookup_weather", json!({ "forecast": "sunny" })),
            ],
        };

        assert_round_trips(MessageNum::Gemini(content));
    }

    fn assert_round_trips(message: MessageNum) {
        let value = serde_json::to_value(&message).expect("message serializes");
        let decoded: MessageNum = serde_json::from_value(value).expect("message deserializes");

        assert_eq!(decoded, message);
    }
}
