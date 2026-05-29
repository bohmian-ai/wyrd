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
pub trait MessageConversion<Target> {
    fn convert(&self) -> SkaldResult<Target>;
}

impl MessageConversion<AnthropicMessage> for OpenAiChatMessage {
    fn convert(&self) -> SkaldResult<AnthropicMessage> {
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

impl MessageConversion<GoogleContent> for OpenAiChatMessage {
    fn convert(&self) -> SkaldResult<GoogleContent> {
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

impl MessageConversion<OpenAiChatMessage> for AnthropicMessage {
    fn convert(&self) -> SkaldResult<OpenAiChatMessage> {
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

        if let Some((tool_use_id, content)) = anthropic_tool_result(&self.content) {
            return Ok(OpenAiChatMessage {
                role: "tool".to_string(),
                content: Some(OpenAiMessageContent::Text(content)),
                name: None,
                tool_calls: None,
                tool_call_id: Some(tool_use_id),
                refusal: None,
            });
        }

        Ok(OpenAiChatMessage {
            role: anthropic_role_to_openai(&self.role).to_string(),
            content: Some(OpenAiMessageContent::Parts(anthropic_content_to_openai(
                &self.content,
            ))),
            name: None,
            tool_calls: if tool_calls.is_empty() {
                None
            } else {
                Some(tool_calls)
            },
            tool_call_id: None,
            refusal: None,
        })
    }
}

impl MessageConversion<GoogleContent> for AnthropicMessage {
    fn convert(&self) -> SkaldResult<GoogleContent> {
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

impl MessageConversion<OpenAiChatMessage> for GoogleContent {
    fn convert(&self) -> SkaldResult<OpenAiChatMessage> {
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

        if let Some((name, response)) = google_function_response(&self.parts) {
            return Ok(OpenAiChatMessage {
                role: "tool".to_string(),
                content: Some(OpenAiMessageContent::Text(response.to_string())),
                name: None,
                tool_calls: None,
                tool_call_id: Some(name),
                refusal: None,
            });
        }

        Ok(OpenAiChatMessage {
            role: google_role_to_openai(&self.role).to_string(),
            content: Some(OpenAiMessageContent::Parts(google_content_to_openai(
                &self.parts,
            ))),
            name: None,
            tool_calls: if tool_calls.is_empty() {
                None
            } else {
                Some(tool_calls)
            },
            tool_call_id: None,
            refusal: None,
        })
    }
}

impl MessageConversion<AnthropicMessage> for GoogleContent {
    fn convert(&self) -> SkaldResult<AnthropicMessage> {
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
            message.convert().map(MessageNum::OpenAi)
        }
        (ProviderName::Anthropic, ProviderName::Google, MessageNum::Anthropic(message)) => {
            message.convert().map(MessageNum::Gemini)
        }
        (ProviderName::Google, ProviderName::OpenAi, MessageNum::Gemini(message)) => {
            message.convert().map(MessageNum::OpenAi)
        }
        (ProviderName::Google, ProviderName::Anthropic, MessageNum::Gemini(message)) => {
            message.convert().map(MessageNum::Anthropic)
        }
        _ => Err(SkaldError::unsupported_conversion(src, dst)),
    }
}

fn openai_role_to_anthropic(role: &str) -> &str {
    if role == "assistant" {
        "assistant"
    } else {
        "user"
    }
}

fn openai_role_to_google(role: &str) -> &str {
    if role == "assistant" { "model" } else { "user" }
}

fn anthropic_role_to_openai(role: &str) -> &str {
    if role == "assistant" {
        "assistant"
    } else {
        "user"
    }
}

fn anthropic_role_to_google(role: &str) -> &str {
    if role == "assistant" { "model" } else { "user" }
}

fn google_role_to_openai(role: &str) -> &str {
    if role == "model" { "assistant" } else { "user" }
}

fn google_role_to_anthropic(role: &str) -> &str {
    if role == "model" { "assistant" } else { "user" }
}

fn openai_message_text(message: &OpenAiChatMessage) -> String {
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
    match source {
        crate::wire::anthropic_messages::AnthropicImageSource::Url { url } => Some(url.clone()),
        _ => None,
    }
}

fn anthropic_tool_result(content: &[AnthropicContentBlock]) -> Option<(String, String)> {
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
    parts.iter().find_map(|part| match part {
        GooglePart::FunctionResponse { function_response } => Some((
            function_response.name.clone(),
            function_response.response.clone(),
        )),
        _ => None,
    })
}
