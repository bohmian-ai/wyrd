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
    /// Provider call identifier.
    pub id: Cow<'a, str>,
    /// Provider tool or function name.
    pub name: Cow<'a, str>,
    /// Provider-emitted arguments as JSON text when available.
    pub arguments: Cow<'a, str>,
}

/// Provider usage reduced into the shared adapter atom.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsageView {
    /// Token usage normalized from the provider-specific response.
    pub usage: TokenUsage,
}

impl<'a> ResponseAdapter<'a> {
    /// Create a borrowed adapter over a provider response.
    pub const fn new(response: &'a ProviderResponse) -> Self {
        Self { response }
    }

    /// Read assistant text, borrowing when a provider stores one contiguous string.
    pub fn text(&self) -> Option<Cow<'a, str>> {
        match self.response {
            ProviderResponse::OpenAiChatCompletion(response) => openai_chat_text(response),
            ProviderResponse::OpenAiResponses(response) => openai_responses_text(response),
            ProviderResponse::AnthropicMessage(response) => anthropic_text(response),
            ProviderResponse::GeminiGenerateContent(response)
            | ProviderResponse::VertexGenerateContent(response) => google_text(response),
            ProviderResponse::OpenAiEmbeddings(_)
            | ProviderResponse::GoogleBatchEmbed(_)
            | ProviderResponse::VertexPredict(_)
            | ProviderResponse::RawV1(_) => None,
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
                        // Anthropic stores tool input as JSON, not a raw
                        // string. Serialize here so callers get the same
                        // `arguments` view OpenAI exposes natively.
                        arguments: Cow::Owned(input.to_string()),
                    }),
                    _ => None,
                })
                .collect(),
            ProviderResponse::GeminiGenerateContent(response)
            | ProviderResponse::VertexGenerateContent(response) => response
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
                                // Gemini function calls do not expose a
                                // separate call id and store args as JSON.
                                arguments: Cow::Owned(function_call.args.to_string()),
                            }),
                            _ => None,
                        })
                        .collect()
                })
                .unwrap_or_default(),
            ProviderResponse::OpenAiEmbeddings(_)
            | ProviderResponse::GoogleBatchEmbed(_)
            | ProviderResponse::VertexPredict(_)
            | ProviderResponse::RawV1(_) => Vec::new(),
        }
    }

    /// Parse the first text payload that is valid JSON as structured output.
    ///
    /// Providers in S02 store structured output as assistant text, so this
    /// method returns an owned raw JSON value parsed from that text.
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
            ProviderResponse::GeminiGenerateContent(response)
            | ProviderResponse::VertexGenerateContent(response) => {
                response.usage_metadata.clone().map(TokenUsage::from)
            }
            ProviderResponse::OpenAiEmbeddings(_)
            | ProviderResponse::GoogleBatchEmbed(_)
            | ProviderResponse::VertexPredict(_)
            | ProviderResponse::RawV1(_) => None,
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
            ProviderResponse::GeminiGenerateContent(response)
            | ProviderResponse::VertexGenerateContent(response) => response
                .candidates
                .first()
                .and_then(|candidate| candidate.finish_reason.as_ref())
                .map(google_finish_reason)
                .unwrap_or(FinishReason::Other),
            ProviderResponse::OpenAiEmbeddings(_)
            | ProviderResponse::GoogleBatchEmbed(_)
            | ProviderResponse::VertexPredict(_)
            | ProviderResponse::RawV1(_) => FinishReason::Other,
        }
    }
}

fn openai_tool_call_view(call: &OpenAiToolCall) -> ToolCallView<'_> {
    // OpenAI already stores all tool-call view fields as strings, so the view
    // can borrow without allocation.
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
    // Prefer a borrowed return for the common one-text-part case. Only allocate
    // when the provider split assistant text across multiple content parts.
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
    // Responses output is an ordered list of output items. The adapter reduces
    // only text content parts and leaves reasoning/tool items to their own
    // accessors.
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
    // Anthropic can return several text blocks in one assistant message. Keep
    // a borrow when there is one block; concatenate only when needed.
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
    // Gemini text lives inside the first candidate's parts. Thought text is
    // included because it is text-bearing provider output in the same stream.
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
    // Keep provider spelling at the edge and expose only the small adapter atom.
    match reason {
        "stop" => FinishReason::Stop,
        "length" => FinishReason::Length,
        "content_filter" => FinishReason::ContentFilter,
        "tool_calls" | "function_call" => FinishReason::ToolCalls,
        _ => FinishReason::Other,
    }
}

fn anthropic_finish_reason(reason: &AnthropicStopReason) -> FinishReason {
    // Anthropic stop reasons have richer names; this is the intentionally
    // reduced branching surface shared by the runtime.
    match reason {
        AnthropicStopReason::EndTurn | AnthropicStopReason::StopSequence => FinishReason::Stop,
        AnthropicStopReason::MaxTokens => FinishReason::Length,
        AnthropicStopReason::ToolUse => FinishReason::ToolCalls,
        AnthropicStopReason::Refusal => FinishReason::ContentFilter,
        AnthropicStopReason::PauseTurn => FinishReason::Other,
    }
}

fn google_finish_reason(reason: &GoogleFinishReason) -> FinishReason {
    // Gemini has several policy/safety endings. Collapse them into
    // ContentFilter so callers do not need provider-specific policy branches.
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

#[cfg(test)]
mod adapter_tests {
    use serde_json::json;

    use crate::common;
    use crate::wire::anthropic_messages::AnthropicStopReason;
    use crate::wire::google_generate::GoogleFinishReason;
    use crate::wire::openai_chat::OpenAiMessageContent;
    use crate::{FinishReason, ProviderResponse};

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
        let response = ProviderResponse::GeminiGenerateContent(common::google_response(
            GoogleFinishReason::Stop,
        ));
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
}
