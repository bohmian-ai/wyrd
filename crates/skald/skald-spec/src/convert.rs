use serde_json::Value;

use crate::error::{SkaldError, SkaldResult};
use crate::message::MessageNum;
use crate::request::ProviderName;
use crate::wire::anthropic_messages::{
    AnthropicContentBlock, AnthropicMessage, AnthropicToolResultContent,
};
use crate::wire::google_generate::{
    GoogleContent, GoogleFunctionCall, GoogleFunctionResponse, GooglePart,
};
use crate::wire::openai_chat::{
    OpenAiChatMessage, OpenAiContentPart, OpenAiMessageContent, OpenAiToolCall,
    OpenAiToolFunctionCall,
};

/// Directed native-to-native message conversion.
pub trait ProviderMessageConversion<Target> {
    /// Convert this provider-native message into the target provider shape.
    fn convert(&self) -> SkaldResult<Target>;
}

impl ProviderMessageConversion<AnthropicMessage> for OpenAiChatMessage {
    fn convert(&self) -> SkaldResult<AnthropicMessage> {
        // OpenAI tool-result turns are separate `role: tool` messages.
        // Anthropic carries the same handoff as a user-side `tool_result`
        // content block.
        if self.role == "tool" {
            return Ok(AnthropicMessage {
                role: "user".to_string(),
                content: vec![AnthropicContentBlock::ToolResult {
                    tool_use_id: self.tool_call_id.clone().unwrap_or_default(),
                    content: AnthropicToolResultContent::Text(openai_message_text(self)),
                    is_error: None,
                    cache_control: None,
                }],
            });
        }

        let mut content = openai_content_to_anthropic(self.content.as_ref());
        if let Some(tool_calls) = &self.tool_calls {
            content.extend(
                tool_calls
                    .iter()
                    .map(|call| AnthropicContentBlock::ToolUse {
                        id: call.id.clone(),
                        name: call.function.name.clone(),
                        input: serde_json::from_str(&call.function.arguments)
                            .unwrap_or(Value::Null),
                    }),
            );
        }

        Ok(AnthropicMessage {
            role: openai_role_to_anthropic(&self.role).to_string(),
            content,
        })
    }
}

impl ProviderMessageConversion<GoogleContent> for OpenAiChatMessage {
    fn convert(&self) -> SkaldResult<GoogleContent> {
        // Gemini represents tool results as function responses inside user
        // content, so OpenAI's tool message becomes one function_response part.
        if self.role == "tool" {
            return Ok(GoogleContent {
                role: "user".to_string(),
                parts: vec![GooglePart::FunctionResponse {
                    function_response: GoogleFunctionResponse {
                        name: self.tool_call_id.clone().unwrap_or_default(),
                        response: Value::String(openai_message_text(self)),
                    },
                }],
            });
        }

        let mut parts = openai_content_to_google(self.content.as_ref());
        if let Some(tool_calls) = &self.tool_calls {
            parts.extend(tool_calls.iter().map(|call| GooglePart::FunctionCall {
                function_call: GoogleFunctionCall {
                    name: call.function.name.clone(),
                    args: serde_json::from_str(&call.function.arguments).unwrap_or(Value::Null),
                },
            }));
        }

        Ok(GoogleContent {
            role: openai_role_to_google(&self.role).to_string(),
            parts,
        })
    }
}

impl ProviderMessageConversion<OpenAiChatMessage> for AnthropicMessage {
    fn convert(&self) -> SkaldResult<OpenAiChatMessage> {
        // Pull Anthropic tool_use blocks up to OpenAI's assistant-level
        // `tool_calls` array. Text/image blocks remain in message content.
        let tool_calls = self
            .content
            .iter()
            .filter_map(|block| match block {
                AnthropicContentBlock::ToolUse { id, name, input } => Some(OpenAiToolCall {
                    id: id.clone(),
                    kind: "function".to_string(),
                    function: OpenAiToolFunctionCall {
                        name: name.clone(),
                        arguments: input.to_string(),
                    },
                }),
                _ => None,
            })
            .collect::<Vec<_>>();

        // Anthropic tool_result blocks map to OpenAI's dedicated tool role.
        if let Some((tool_use_id, content)) = anthropic_tool_result(&self.content) {
            return Ok(OpenAiChatMessage {
                role: "tool".to_string(),
                content: Some(OpenAiMessageContent::Text(content)),
                tool_call_id: Some(tool_use_id),
                ..Default::default()
            });
        }

        Ok(OpenAiChatMessage {
            role: anthropic_role_to_openai(&self.role).to_string(),
            content: Some(OpenAiMessageContent::Parts(anthropic_content_to_openai(
                &self.content,
            ))),
            tool_calls: if tool_calls.is_empty() {
                None
            } else {
                Some(tool_calls)
            },
            ..Default::default()
        })
    }
}

impl ProviderMessageConversion<GoogleContent> for AnthropicMessage {
    fn convert(&self) -> SkaldResult<GoogleContent> {
        // Directly map Anthropic text/tool blocks to Gemini parts. Media and
        // document blocks are intentionally omitted here until S02 has a
        // provider-safe binary/media bridge.
        let parts = self
            .content
            .iter()
            .filter_map(|block| match block {
                AnthropicContentBlock::Text { text, .. } => {
                    Some(GooglePart::Text { text: text.clone() })
                }
                AnthropicContentBlock::ToolUse { name, input, .. } => {
                    Some(GooglePart::FunctionCall {
                        function_call: GoogleFunctionCall {
                            name: name.clone(),
                            args: input.clone(),
                        },
                    })
                }
                AnthropicContentBlock::ToolResult { content, .. } => {
                    Some(GooglePart::FunctionResponse {
                        function_response: GoogleFunctionResponse {
                            name: "tool_result".to_string(),
                            response: Value::String(anthropic_tool_result_content_text(content)),
                        },
                    })
                }
                _ => None,
            })
            .collect();

        Ok(GoogleContent {
            role: anthropic_role_to_google(&self.role).to_string(),
            parts,
        })
    }
}

impl ProviderMessageConversion<OpenAiChatMessage> for GoogleContent {
    fn convert(&self) -> SkaldResult<OpenAiChatMessage> {
        // Gemini function_call parts become OpenAI assistant tool calls. Gemini
        // does not have a separate call id, so the function name is reused.
        let tool_calls = self
            .parts
            .iter()
            .filter_map(|part| match part {
                GooglePart::FunctionCall { function_call } => Some(OpenAiToolCall {
                    id: function_call.name.clone(),
                    kind: "function".to_string(),
                    function: OpenAiToolFunctionCall {
                        name: function_call.name.clone(),
                        arguments: function_call.args.to_string(),
                    },
                }),
                _ => None,
            })
            .collect::<Vec<_>>();

        // Gemini function_response parts represent tool results, which OpenAI
        // expects as role=tool messages linked by tool_call_id.
        if let Some((name, response)) = google_function_response(&self.parts) {
            return Ok(OpenAiChatMessage {
                role: "tool".to_string(),
                content: Some(OpenAiMessageContent::Text(response.to_string())),
                tool_call_id: Some(name),
                ..Default::default()
            });
        }

        Ok(OpenAiChatMessage {
            role: google_role_to_openai(&self.role).to_string(),
            content: Some(OpenAiMessageContent::Parts(google_content_to_openai(
                &self.parts,
            ))),
            tool_calls: if tool_calls.is_empty() {
                None
            } else {
                Some(tool_calls)
            },
            ..Default::default()
        })
    }
}

impl ProviderMessageConversion<AnthropicMessage> for GoogleContent {
    fn convert(&self) -> SkaldResult<AnthropicMessage> {
        // Gemini content has no neutral intermediate. Text, function_call, and
        // function_response parts map directly into Anthropic blocks.
        let content = self
            .parts
            .iter()
            .filter_map(|part| match part {
                GooglePart::Text { text } | GooglePart::Thought { text, .. } => {
                    Some(AnthropicContentBlock::Text {
                        text: text.clone(),
                        cache_control: None,
                        citations: None,
                    })
                }
                GooglePart::FunctionCall { function_call } => {
                    Some(AnthropicContentBlock::ToolUse {
                        id: function_call.name.clone(),
                        name: function_call.name.clone(),
                        input: function_call.args.clone(),
                    })
                }
                GooglePart::FunctionResponse { function_response } => {
                    Some(AnthropicContentBlock::ToolResult {
                        tool_use_id: function_response.name.clone(),
                        content: AnthropicToolResultContent::Text(
                            function_response.response.to_string(),
                        ),
                        is_error: None,
                        cache_control: None,
                    })
                }
                _ => None,
            })
            .collect();

        Ok(AnthropicMessage {
            role: google_role_to_anthropic(&self.role).to_string(),
            content,
        })
    }
}

/// Convert a provider message using runtime provider names.
///
/// Same-provider conversion returns a cloned identity value. `RawV1` messages
/// and unsupported provider pairs return `SkaldError::UnsupportedConversion`.
pub fn convert_message_dyn(
    src: ProviderName,
    dst: ProviderName,
    msg: &MessageNum,
) -> SkaldResult<MessageNum> {
    if matches!(msg, MessageNum::RawV1(_)) {
        return Err(SkaldError::unsupported_conversion(src, dst));
    }
    if src == dst {
        return Ok(msg.clone());
    }

    match (src.clone(), dst.clone(), msg) {
        (ProviderName::OpenAi, ProviderName::Anthropic, MessageNum::OpenAi(message)) => {
            message.convert().map(MessageNum::Anthropic)
        }
        (ProviderName::OpenAi, ProviderName::Google, MessageNum::OpenAi(message)) => {
            message.convert().map(MessageNum::Gemini)
        }
        (ProviderName::Anthropic, ProviderName::OpenAi, MessageNum::Anthropic(message)) => {
            message.convert().map(|m| MessageNum::OpenAi(Box::new(m)))
        }
        (ProviderName::Anthropic, ProviderName::Google, MessageNum::Anthropic(message)) => {
            message.convert().map(MessageNum::Gemini)
        }
        (ProviderName::Google, ProviderName::OpenAi, MessageNum::Gemini(message)) => {
            message.convert().map(|m| MessageNum::OpenAi(Box::new(m)))
        }
        (ProviderName::Google, ProviderName::Anthropic, MessageNum::Gemini(message)) => {
            message.convert().map(MessageNum::Anthropic)
        }
        _ => Err(SkaldError::unsupported_conversion(src, dst)),
    }
}

fn openai_role_to_anthropic(role: &str) -> &str {
    // Anthropic accepts only user/assistant messages; system/developer OpenAI
    // roles become user content when a standalone message is converted.
    if role == "assistant" {
        "assistant"
    } else {
        "user"
    }
}

fn openai_role_to_google(role: &str) -> &str {
    // Gemini uses `model` for assistant output and `user` for all input-side
    // turns.
    if role == "assistant" { "model" } else { "user" }
}

fn anthropic_role_to_openai(role: &str) -> &str {
    // Anthropic roles line up with OpenAI except for unsupported future roles,
    // which degrade to user-side content.
    if role == "assistant" {
        "assistant"
    } else {
        "user"
    }
}

fn anthropic_role_to_google(role: &str) -> &str {
    // Gemini uses `model` instead of `assistant`.
    if role == "assistant" { "model" } else { "user" }
}

fn google_role_to_openai(role: &str) -> &str {
    // Gemini's `model` role is OpenAI's assistant role.
    if role == "model" { "assistant" } else { "user" }
}

fn google_role_to_anthropic(role: &str) -> &str {
    // Gemini's `model` role is Anthropic's assistant role.
    if role == "model" { "assistant" } else { "user" }
}

fn openai_message_text(message: &OpenAiChatMessage) -> String {
    // Tool-result conversions need a plain text payload. Non-text content is
    // ignored rather than serialized into a made-up neutral media envelope.
    match &message.content {
        Some(OpenAiMessageContent::Text(text)) => text.clone(),
        Some(OpenAiMessageContent::Parts(parts)) => parts
            .iter()
            .filter_map(|part| match part {
                OpenAiContentPart::Text { text } => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join(""),
        None => String::new(),
    }
}

fn openai_content_to_anthropic(
    content: Option<&OpenAiMessageContent>,
) -> Vec<AnthropicContentBlock> {
    // Map only content types with a direct Anthropic native equivalent in S02.
    // Audio/file parts are left out until a later media-specific stage defines
    // provider-safe handling.
    match content {
        Some(OpenAiMessageContent::Text(text)) => vec![AnthropicContentBlock::Text {
            text: text.clone(),
            cache_control: None,
            citations: None,
        }],
        Some(OpenAiMessageContent::Parts(parts)) => parts
            .iter()
            .filter_map(|part| match part {
                OpenAiContentPart::Text { text } => Some(AnthropicContentBlock::Text {
                    text: text.clone(),
                    cache_control: None,
                    citations: None,
                }),
                OpenAiContentPart::ImageUrl { image_url } => Some(AnthropicContentBlock::Image {
                    source: crate::wire::anthropic_messages::AnthropicImageSource::Url {
                        url: image_url.url.clone(),
                    },
                    cache_control: None,
                }),
                _ => None,
            })
            .collect(),
        None => Vec::new(),
    }
}

fn openai_content_to_google(content: Option<&OpenAiMessageContent>) -> Vec<GooglePart> {
    // Text maps directly. Image URLs become Gemini file_data URIs because that
    // is the closest native Gemini shape without downloading bytes.
    match content {
        Some(OpenAiMessageContent::Text(text)) => vec![GooglePart::Text { text: text.clone() }],
        Some(OpenAiMessageContent::Parts(parts)) => parts
            .iter()
            .filter_map(|part| match part {
                OpenAiContentPart::Text { text } => Some(GooglePart::Text { text: text.clone() }),
                OpenAiContentPart::ImageUrl { image_url } => Some(GooglePart::FileData {
                    file_data: crate::wire::google_generate::GoogleFileData {
                        mime_type: "image/*".to_string(),
                        file_uri: image_url.url.clone(),
                    },
                }),
                _ => None,
            })
            .collect(),
        None => Vec::new(),
    }
}

fn anthropic_content_to_openai(content: &[AnthropicContentBlock]) -> Vec<OpenAiContentPart> {
    // This helper maps only message content. Anthropic tool_use/tool_result
    // blocks are handled by the caller because OpenAI stores them outside the
    // normal content array.
    content
        .iter()
        .filter_map(|block| match block {
            AnthropicContentBlock::Text { text, .. } => {
                Some(OpenAiContentPart::Text { text: text.clone() })
            }
            AnthropicContentBlock::Image { source, .. } => {
                anthropic_image_url(source).map(|url| OpenAiContentPart::ImageUrl {
                    image_url: crate::wire::openai_chat::OpenAiImageUrl { url, detail: None },
                })
            }
            _ => None,
        })
        .collect()
}

fn google_content_to_openai(parts: &[GooglePart]) -> Vec<OpenAiContentPart> {
    // Gemini function calls/responses are handled by the caller; this helper is
    // only for content parts that fit inside OpenAI message content.
    parts
        .iter()
        .filter_map(|part| match part {
            GooglePart::Text { text } | GooglePart::Thought { text, .. } => {
                Some(OpenAiContentPart::Text { text: text.clone() })
            }
            GooglePart::FileData { file_data } => Some(OpenAiContentPart::ImageUrl {
                image_url: crate::wire::openai_chat::OpenAiImageUrl {
                    url: file_data.file_uri.clone(),
                    detail: None,
                },
            }),
            _ => None,
        })
        .collect()
}

fn anthropic_image_url(
    source: &crate::wire::anthropic_messages::AnthropicImageSource,
) -> Option<String> {
    // Only URL-backed images can cross without inventing storage or byte
    // transfer behavior inside skald-spec.
    match source {
        crate::wire::anthropic_messages::AnthropicImageSource::Url { url } => Some(url.clone()),
        _ => None,
    }
}

fn anthropic_tool_result(content: &[AnthropicContentBlock]) -> Option<(String, String)> {
    // Anthropic can mix tool results with other blocks; OpenAI needs a single
    // tool message, so conversion uses the first tool_result block.
    content.iter().find_map(|block| match block {
        AnthropicContentBlock::ToolResult {
            tool_use_id,
            content,
            ..
        } => Some((
            tool_use_id.clone(),
            anthropic_tool_result_content_text(content),
        )),
        _ => None,
    })
}

fn anthropic_tool_result_content_text(content: &AnthropicToolResultContent) -> String {
    // Tool result content may be either raw text or blocks. Keep text blocks and
    // ignore non-text blocks rather than fabricating a neutral representation.
    match content {
        AnthropicToolResultContent::Text(text) => text.clone(),
        AnthropicToolResultContent::Blocks(blocks) => blocks
            .iter()
            .filter_map(|block| match block {
                AnthropicContentBlock::Text { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join(""),
    }
}

fn google_function_response(parts: &[GooglePart]) -> Option<(String, Value)> {
    // Gemini can include several parts in one turn. OpenAI conversion emits one
    // tool message, so use the first function_response part.
    parts.iter().find_map(|part| match part {
        GooglePart::FunctionResponse { function_response } => Some((
            function_response.name.clone(),
            function_response.response.clone(),
        )),
        _ => None,
    })
}

#[cfg(test)]
mod conversion {
    use serde_json::{json, value::RawValue};

    use crate::common;
    use crate::wire::anthropic_messages::{
        AnthropicContentBlock, AnthropicMessage, AnthropicToolResultContent,
    };
    use crate::wire::google_generate::GooglePart;
    use crate::wire::openai_chat::{OpenAiMessageContent, OpenAiToolCall};
    use crate::{
        MessageNum, ProviderMessageConversion, ProviderName, SkaldError, convert_message_dyn,
    };

    #[test]
    fn openai_to_anthropic_message_converts_text_and_tool_call() {
        let converted: crate::wire::anthropic_messages::AnthropicMessage =
            common::openai_message().convert().unwrap();
        assert_eq!(converted.role, "assistant");
        assert!(converted.content.iter().any(
            |block| matches!(block, AnthropicContentBlock::Text { text, .. } if text == "hello")
        ));
        assert!(converted.content.iter().any(
            |block| matches!(block, AnthropicContentBlock::ToolUse { name, .. } if name == "lookup")
        ));
    }

    #[test]
    fn openai_to_anthropic_message_converts_tool_result() {
        let message = crate::wire::openai_chat::OpenAiChatMessage {
            role: "tool".to_string(),
            content: Some(OpenAiMessageContent::Text("done".to_string())),
            name: None,
            tool_calls: None,
            tool_call_id: Some("call_1".to_string()),
            refusal: None,
            annotations: Vec::new(),
            audio: None,
        };
        let converted: crate::wire::anthropic_messages::AnthropicMessage =
            message.convert().unwrap();
        assert!(matches!(
            &converted.content[0],
            AnthropicContentBlock::ToolResult { tool_use_id, .. } if tool_use_id == "call_1"
        ));
    }

    #[test]
    fn openai_to_gemini_message_converts_text_and_tool_call() {
        let converted: crate::wire::google_generate::GoogleContent =
            common::openai_message().convert().unwrap();
        assert_eq!(converted.role, "model");
        assert!(converted
            .parts
            .iter()
            .any(|part| matches!(part, GooglePart::FunctionCall { function_call } if function_call.name == "lookup")));
    }

    #[test]
    fn anthropic_to_openai_message_converts_text_and_tool_use() {
        let converted: crate::wire::openai_chat::OpenAiChatMessage =
            common::anthropic_message().convert().unwrap();
        assert_eq!(converted.role, "assistant");
        assert!(matches!(
            converted.tool_calls.as_ref().unwrap().first().unwrap(),
            OpenAiToolCall { function, .. } if function.name == "lookup"
        ));
    }

    #[test]
    fn anthropic_to_openai_message_converts_tool_result() {
        let message = crate::wire::anthropic_messages::AnthropicMessage {
            role: "user".to_string(),
            content: vec![AnthropicContentBlock::ToolResult {
                tool_use_id: "tool_1".to_string(),
                content: AnthropicToolResultContent::Text("done".to_string()),
                is_error: None,
                cache_control: None,
            }],
        };
        let converted: crate::wire::openai_chat::OpenAiChatMessage = message.convert().unwrap();
        assert_eq!(converted.role, "tool");
        assert_eq!(converted.tool_call_id.as_deref(), Some("tool_1"));
    }

    #[test]
    fn anthropic_to_gemini_message_converts_text_and_tool_use() {
        let converted: crate::wire::google_generate::GoogleContent =
            common::anthropic_message().convert().unwrap();
        assert_eq!(converted.role, "model");
        assert!(converted
            .parts
            .iter()
            .any(|part| matches!(part, GooglePart::FunctionCall { function_call } if function_call.name == "lookup")));
    }

    #[test]
    fn gemini_to_openai_message_converts_text_and_function_call() {
        let converted: crate::wire::openai_chat::OpenAiChatMessage =
            common::google_message().convert().unwrap();
        assert_eq!(converted.role, "assistant");
        assert!(converted.tool_calls.is_some());
    }

    #[test]
    fn gemini_to_anthropic_message_converts_text_and_function_call() {
        let converted: crate::wire::anthropic_messages::AnthropicMessage =
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
                MessageNum::OpenAi(Box::new(common::openai_message())),
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
            &MessageNum::OpenAi(Box::new(common::openai_message())),
        )
        .unwrap_err();
        assert!(matches!(err, SkaldError::UnsupportedConversion { .. }));
        assert_eq!(err.code(), "SKALD_SPEC_501_UNSUPPORTED_CONVERSION");
    }

    #[test]
    fn raw_v1_message_cannot_be_converted_returns_skald_spec_501() {
        let raw = MessageNum::RawV1(RawValue::from_string(json!({"x": 1}).to_string()).unwrap());
        let err =
            convert_message_dyn(ProviderName::OpenAi, ProviderName::Anthropic, &raw).unwrap_err();
        assert_eq!(err.code(), "SKALD_SPEC_501_UNSUPPORTED_CONVERSION");
    }

    #[test]
    fn anthropic_tool_result_blocks_content_converts_text_only() {
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
                        source: crate::wire::anthropic_messages::AnthropicImageSource::Base64 {
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

        let converted: crate::wire::openai_chat::OpenAiChatMessage = message.convert().unwrap();
        assert_eq!(converted.role, "tool");
        assert_eq!(converted.tool_call_id.as_deref(), Some("call_blocks"));
        let content_text = match converted.content {
            Some(OpenAiMessageContent::Text(text)) => text,
            other => panic!("expected Text content, got {other:?}"),
        };
        assert_eq!(content_text, "first text second text");
    }
}
