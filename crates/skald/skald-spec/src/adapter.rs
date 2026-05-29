use std::borrow::Cow;

use serde_json::value::RawValue;

use crate::response::ProviderResponse;
use crate::wire::anthropic_messages::{
    AnthropicContentBlock, AnthropicMessagesResponse, AnthropicStopReason,
};
use crate::wire::common::{FinishReason, TokenUsage};
use crate::wire::google_generate::{GoogleFinishReason, GoogleGenerateContentResponse, GooglePart};
use crate::wire::openai_chat::{
    OpenAiChatResponse, OpenAiContentPart, OpenAiMessageContent, OpenAiToolCall,
};
use crate::wire::openai_responses::{
    OpenAiResponseContentPart, OpenAiResponseItem, OpenAiResponsesResponse,
};

/// Borrowed read view over a native provider response.
pub struct ResponseAdapter<'a> {
    response: &'a ProviderResponse,
}

/// Provider tool/function call view.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolCallView<'a> {
    pub id: Cow<'a, str>,
    pub name: Cow<'a, str>,
    pub arguments: Cow<'a, str>,
}

/// Provider usage reduced into the shared adapter atom.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsageView {
    pub usage: TokenUsage,
}

impl<'a> ResponseAdapter<'a> {
    pub const fn new(response: &'a ProviderResponse) -> Self {
        Self { response }
    }

    /// Read assistant text, borrowing when a provider stores one contiguous string.
    pub fn text(&self) -> Option<Cow<'a, str>> {
        match self.response {
            ProviderResponse::OpenAiChatCompletion(response) => openai_chat_text(response),
            ProviderResponse::OpenAiResponses(response) => openai_responses_text(response),
            ProviderResponse::AnthropicMessage(response) => anthropic_text(response),
            ProviderResponse::GeminiGenerateContent(response) => google_text(response),
            ProviderResponse::RawV1(_) => None,
        }
    }

    /// Read provider tool/function calls.
    pub fn tool_calls(&self) -> Vec<ToolCallView<'a>> {
        match self.response {
            ProviderResponse::OpenAiChatCompletion(response) => response
                .choices
                .first()
                .and_then(|choice| choice.message.tool_calls.as_ref())
                .map(|calls| calls.iter().map(openai_tool_call_view).collect())
                .unwrap_or_default(),
            ProviderResponse::OpenAiResponses(response) => response
                .output
                .iter()
                .filter_map(|item| match item {
                    OpenAiResponseItem::FunctionCall {
                        call_id,
                        name,
                        arguments,
                    } => Some(ToolCallView {
                        id: Cow::Borrowed(call_id),
                        name: Cow::Borrowed(name),
                        arguments: Cow::Borrowed(arguments),
                    }),
                    _ => None,
                })
                .collect(),
            ProviderResponse::AnthropicMessage(response) => response
                .content
                .iter()
                .filter_map(|block| match block {
                    AnthropicContentBlock::ToolUse { id, name, input } => Some(ToolCallView {
                        id: Cow::Borrowed(id),
                        name: Cow::Borrowed(name),
                        arguments: Cow::Owned(input.to_string()),
                    }),
                    _ => None,
                })
                .collect(),
            ProviderResponse::GeminiGenerateContent(response) => response
                .candidates
                .first()
                .map(|candidate| {
                    candidate
                        .content
                        .parts
                        .iter()
                        .filter_map(|part| match part {
                            GooglePart::FunctionCall { function_call } => Some(ToolCallView {
                                id: Cow::Borrowed(&function_call.name),
                                name: Cow::Borrowed(&function_call.name),
                                arguments: Cow::Owned(function_call.args.to_string()),
                            }),
                            _ => None,
                        })
                        .collect()
                })
                .unwrap_or_default(),
            ProviderResponse::RawV1(_) => Vec::new(),
        }
    }

    /// Parse the first text payload that is valid JSON as structured output.
    pub fn structured_output(&self) -> Option<Box<RawValue>> {
        let text = self.text()?;
        RawValue::from_string(text.into_owned()).ok()
    }

    /// Read reduced provider usage.
    pub fn usage(&self) -> Option<UsageView> {
        let usage = match self.response {
            ProviderResponse::OpenAiChatCompletion(response) => {
                response.usage.clone().map(TokenUsage::from)
            }
            ProviderResponse::OpenAiResponses(response) => {
                response.usage.clone().map(TokenUsage::from)
            }
            ProviderResponse::AnthropicMessage(response) => {
                Some(TokenUsage::from(response.usage.clone()))
            }
            ProviderResponse::GeminiGenerateContent(response) => {
                response.usage_metadata.clone().map(TokenUsage::from)
            }
            ProviderResponse::RawV1(_) => None,
        }?;
        Some(UsageView { usage })
    }

    /// Read reduced provider finish reason.
    pub fn finish_reason(&self) -> FinishReason {
        match self.response {
            ProviderResponse::OpenAiChatCompletion(response) => response
                .choices
                .first()
                .and_then(|choice| choice.finish_reason.as_deref())
                .map(openai_finish_reason)
                .unwrap_or(FinishReason::Other),
            ProviderResponse::OpenAiResponses(response) => {
                if response
                    .output
                    .iter()
                    .any(|item| matches!(item, OpenAiResponseItem::FunctionCall { .. }))
                {
                    FinishReason::ToolCalls
                } else {
                    match response.status.as_str() {
                        "completed" => FinishReason::Stop,
                        "incomplete" => FinishReason::Length,
                        _ => FinishReason::Other,
                    }
                }
            }
            ProviderResponse::AnthropicMessage(response) => response
                .stop_reason
                .as_ref()
                .map(anthropic_finish_reason)
                .unwrap_or(FinishReason::Other),
            ProviderResponse::GeminiGenerateContent(response) => response
                .candidates
                .first()
                .and_then(|candidate| candidate.finish_reason.as_ref())
                .map(google_finish_reason)
                .unwrap_or(FinishReason::Other),
            ProviderResponse::RawV1(_) => FinishReason::Other,
        }
    }
}

fn openai_tool_call_view(call: &OpenAiToolCall) -> ToolCallView<'_> {
    ToolCallView {
        id: Cow::Borrowed(&call.id),
        name: Cow::Borrowed(&call.function.name),
        arguments: Cow::Borrowed(&call.function.arguments),
    }
}

fn openai_chat_text(response: &OpenAiChatResponse) -> Option<Cow<'_, str>> {
    match response.choices.first()?.message.content.as_ref()? {
        OpenAiMessageContent::Text(text) => Some(Cow::Borrowed(text)),
        OpenAiMessageContent::Parts(parts) => text_from_openai_parts(parts),
    }
}

fn text_from_openai_parts(parts: &[OpenAiContentPart]) -> Option<Cow<'_, str>> {
    let mut text_parts = parts.iter().filter_map(|part| match part {
        OpenAiContentPart::Text { text } => Some(text.as_str()),
        _ => None,
    });
    let first = text_parts.next()?;
    match text_parts.next() {
        None => Some(Cow::Borrowed(first)),
        Some(second) => {
            let mut out = String::from(first);
            out.push_str(second);
            for text in text_parts {
                out.push_str(text);
            }
            Some(Cow::Owned(out))
        }
    }
}

fn openai_responses_text(response: &OpenAiResponsesResponse) -> Option<Cow<'_, str>> {
    let mut texts = response.output.iter().flat_map(|item| match item {
        OpenAiResponseItem::Message { content, .. } => content
            .iter()
            .filter_map(|part| match part {
                OpenAiResponseContentPart::OutputText { text }
                | OpenAiResponseContentPart::InputText { text } => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>(),
        _ => Vec::new(),
    });
    let first = texts.next()?;
    match texts.next() {
        None => Some(Cow::Borrowed(first)),
        Some(second) => {
            let mut out = String::from(first);
            out.push_str(second);
            for text in texts {
                out.push_str(text);
            }
            Some(Cow::Owned(out))
        }
    }
}

fn anthropic_text(response: &AnthropicMessagesResponse) -> Option<Cow<'_, str>> {
    let mut texts = response.content.iter().filter_map(|block| match block {
        AnthropicContentBlock::Text { text, .. } => Some(text.as_str()),
        _ => None,
    });
    let first = texts.next()?;
    match texts.next() {
        None => Some(Cow::Borrowed(first)),
        Some(second) => {
            let mut out = String::from(first);
            out.push_str(second);
            for text in texts {
                out.push_str(text);
            }
            Some(Cow::Owned(out))
        }
    }
}

fn google_text(response: &GoogleGenerateContentResponse) -> Option<Cow<'_, str>> {
    let mut texts = response
        .candidates
        .first()?
        .content
        .parts
        .iter()
        .filter_map(|part| match part {
            GooglePart::Text { text } | GooglePart::Thought { text, .. } => Some(text.as_str()),
            _ => None,
        });
    let first = texts.next()?;
    match texts.next() {
        None => Some(Cow::Borrowed(first)),
        Some(second) => {
            let mut out = String::from(first);
            out.push_str(second);
            for text in texts {
                out.push_str(text);
            }
            Some(Cow::Owned(out))
        }
    }
}

fn openai_finish_reason(reason: &str) -> FinishReason {
    match reason {
        "stop" => FinishReason::Stop,
        "length" => FinishReason::Length,
        "content_filter" => FinishReason::ContentFilter,
        "tool_calls" | "function_call" => FinishReason::ToolCalls,
        _ => FinishReason::Other,
    }
}

fn anthropic_finish_reason(reason: &AnthropicStopReason) -> FinishReason {
    match reason {
        AnthropicStopReason::EndTurn | AnthropicStopReason::StopSequence => FinishReason::Stop,
        AnthropicStopReason::MaxTokens => FinishReason::Length,
        AnthropicStopReason::ToolUse => FinishReason::ToolCalls,
        AnthropicStopReason::Refusal => FinishReason::ContentFilter,
        AnthropicStopReason::PauseTurn => FinishReason::Other,
    }
}

fn google_finish_reason(reason: &GoogleFinishReason) -> FinishReason {
    match reason {
        GoogleFinishReason::Stop => FinishReason::Stop,
        GoogleFinishReason::MaxTokens => FinishReason::Length,
        GoogleFinishReason::Safety
        | GoogleFinishReason::Blocklist
        | GoogleFinishReason::ProhibitedContent
        | GoogleFinishReason::Spii => FinishReason::ContentFilter,
        GoogleFinishReason::MalformedFunctionCall => FinishReason::ToolCalls,
        GoogleFinishReason::Recitation
        | GoogleFinishReason::Language
        | GoogleFinishReason::Other
        | GoogleFinishReason::FinishReasonUnspecified => FinishReason::Other,
    }
}
