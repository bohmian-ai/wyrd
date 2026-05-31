mod common;

use serde_json::{json, value::RawValue};
use skald_spec::wire::anthropic_messages::{
    AnthropicContentBlock, AnthropicMessage, AnthropicToolResultContent,
};
use skald_spec::wire::google_generate::GooglePart;
use skald_spec::wire::openai_chat::{OpenAiMessageContent, OpenAiToolCall};
use skald_spec::{MessageConversion, MessageNum, ProviderName, SkaldError, convert_message_dyn};

#[test]
fn openai_to_anthropic_message_converts_text_and_tool_call() {
    let converted: skald_spec::wire::anthropic_messages::AnthropicMessage =
        common::openai_message().convert().unwrap();
    assert_eq!(converted.role, "assistant");
    assert!(
        converted.content.iter().any(
            |block| matches!(block, AnthropicContentBlock::Text { text, .. } if text == "hello")
        )
    );
    assert!(converted.content.iter().any(
        |block| matches!(block, AnthropicContentBlock::ToolUse { name, .. } if name == "lookup")
    ));
}

#[test]
fn openai_to_anthropic_message_converts_tool_result() {
    let message = skald_spec::wire::openai_chat::OpenAiChatMessage {
        role: "tool".to_string(),
        content: Some(OpenAiMessageContent::Text("done".to_string())),
        name: None,
        tool_calls: None,
        tool_call_id: Some("call_1".to_string()),
        refusal: None,
    };
    let converted: skald_spec::wire::anthropic_messages::AnthropicMessage =
        message.convert().unwrap();
    assert!(matches!(
        &converted.content[0],
        AnthropicContentBlock::ToolResult { tool_use_id, .. } if tool_use_id == "call_1"
    ));
}

#[test]
fn openai_to_gemini_message_converts_text_and_tool_call() {
    let converted: skald_spec::wire::google_generate::GoogleContent =
        common::openai_message().convert().unwrap();
    assert_eq!(converted.role, "model");
    assert!(converted
        .parts
        .iter()
        .any(|part| matches!(part, GooglePart::FunctionCall { function_call } if function_call.name == "lookup")));
}

#[test]
fn anthropic_to_openai_message_converts_text_and_tool_use() {
    let converted: skald_spec::wire::openai_chat::OpenAiChatMessage =
        common::anthropic_message().convert().unwrap();
    assert_eq!(converted.role, "assistant");
    assert!(matches!(
        converted.tool_calls.as_ref().unwrap().first().unwrap(),
        OpenAiToolCall { function, .. } if function.name == "lookup"
    ));
}

#[test]
fn anthropic_to_openai_message_converts_tool_result() {
    let message = skald_spec::wire::anthropic_messages::AnthropicMessage {
        role: "user".to_string(),
        content: vec![AnthropicContentBlock::ToolResult {
            tool_use_id: "tool_1".to_string(),
            content: AnthropicToolResultContent::Text("done".to_string()),
            is_error: None,
            cache_control: None,
        }],
    };
    let converted: skald_spec::wire::openai_chat::OpenAiChatMessage = message.convert().unwrap();
    assert_eq!(converted.role, "tool");
    assert_eq!(converted.tool_call_id.as_deref(), Some("tool_1"));
}

#[test]
fn anthropic_to_gemini_message_converts_text_and_tool_use() {
    let converted: skald_spec::wire::google_generate::GoogleContent =
        common::anthropic_message().convert().unwrap();
    assert_eq!(converted.role, "model");
    assert!(converted
        .parts
        .iter()
        .any(|part| matches!(part, GooglePart::FunctionCall { function_call } if function_call.name == "lookup")));
}

#[test]
fn gemini_to_openai_message_converts_text_and_function_call() {
    let converted: skald_spec::wire::openai_chat::OpenAiChatMessage =
        common::google_message().convert().unwrap();
    assert_eq!(converted.role, "assistant");
    assert!(converted.tool_calls.is_some());
}

#[test]
fn gemini_to_anthropic_message_converts_text_and_function_call() {
    let converted: skald_spec::wire::anthropic_messages::AnthropicMessage =
        common::google_message().convert().unwrap();
    assert_eq!(converted.role, "assistant");
    assert!(converted.content.iter().any(
        |block| matches!(block, AnthropicContentBlock::ToolUse { name, .. } if name == "lookup")
    ));
}

#[test]
fn same_provider_conversion_is_identity() {
    for (provider, message) in [
        (
            ProviderName::OpenAi,
            MessageNum::OpenAi(common::openai_message()),
        ),
        (
            ProviderName::Anthropic,
            MessageNum::Anthropic(common::anthropic_message()),
        ),
        (
            ProviderName::Google,
            MessageNum::Gemini(common::google_message()),
        ),
    ] {
        assert_eq!(
            convert_message_dyn(provider.clone(), provider, &message).unwrap(),
            message
        );
    }
}

#[test]
fn unsupported_conversion_pair_returns_skald_spec_501() {
    let err = convert_message_dyn(
        ProviderName::OpenAi,
        ProviderName::Vertex,
        &MessageNum::OpenAi(common::openai_message()),
    )
    .unwrap_err();
    assert!(matches!(err, SkaldError::UnsupportedConversion { .. }));
    assert_eq!(err.code(), "SKALD_SPEC_501_UNSUPPORTED_CONVERSION");
}

#[test]
fn raw_v1_message_cannot_be_converted_returns_skald_spec_501() {
    let raw = MessageNum::RawV1(RawValue::from_string(json!({"x": 1}).to_string()).unwrap());
    let err = convert_message_dyn(ProviderName::OpenAi, ProviderName::Anthropic, &raw).unwrap_err();
    assert_eq!(err.code(), "SKALD_SPEC_501_UNSUPPORTED_CONVERSION");
}

#[test]
fn anthropic_tool_result_blocks_content_converts_text_only() {
    // AnthropicToolResultContent::Blocks with mixed text and non-text blocks.
    // Conversion to OpenAI should join only the text blocks and discard images.
    let message = AnthropicMessage {
        role: "user".to_string(),
        content: vec![AnthropicContentBlock::ToolResult {
            tool_use_id: "call_blocks".to_string(),
            content: AnthropicToolResultContent::Blocks(vec![
                AnthropicContentBlock::Text {
                    text: "first text".to_string(),
                    cache_control: None,
                    citations: None,
                },
                AnthropicContentBlock::Image {
                    source: skald_spec::wire::anthropic_messages::AnthropicImageSource::Base64 {
                        media_type: "image/png".to_string(),
                        data: "aGVsbG8=".to_string(),
                    },
                    cache_control: None,
                },
                AnthropicContentBlock::Text {
                    text: " second text".to_string(),
                    cache_control: None,
                    citations: None,
                },
            ]),
            is_error: None,
            cache_control: None,
        }],
    };

    let converted: skald_spec::wire::openai_chat::OpenAiChatMessage = message.convert().unwrap();
    assert_eq!(converted.role, "tool");
    assert_eq!(converted.tool_call_id.as_deref(), Some("call_blocks"));
    // Only text blocks are joined; the image block is discarded.
    let content_text = match converted.content {
        Some(OpenAiMessageContent::Text(text)) => text,
        other => panic!("expected Text content, got {other:?}"),
    };
    assert_eq!(content_text, "first text second text");
}
