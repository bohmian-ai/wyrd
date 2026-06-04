//! Cross-provider message translation for workflow task handoff.
//!
//! This module owns no conversion logic. It dispatches into
//! [`skald_spec::convert_message_dyn`], the canonical native message converter
//! used by Skald provider surfaces.

use skald_spec::wire::anthropic_messages::{AnthropicContentBlock, AnthropicMessage};
use skald_spec::wire::google_generate::{GoogleContent, GooglePart};
use skald_spec::wire::openai_chat::{OpenAiChatMessage, OpenAiMessageContent};
use skald_spec::{MessageNum, ProviderName, ProviderResponse, convert_message_dyn};

use crate::error::{WorkflowError, WorkflowResult};

/// Translate native messages from one provider to another for workflow handoff.
///
/// `MessageNum::RawV1` messages cannot be handed off, even when the source and
/// destination providers match, because v1 handoff requires typed text messages.
///
/// # Errors
///
/// Returns [`WorkflowError::UnsupportedHandoff`] when a message is raw or the
/// canonical converter does not support the source/destination pair.
pub fn handoff_messages(
    src_provider: &ProviderName,
    dst_provider: &ProviderName,
    msgs: &[MessageNum],
) -> WorkflowResult<Vec<MessageNum>> {
    if msgs.iter().any(|msg| matches!(msg, MessageNum::RawV1(_))) {
        return Err(WorkflowError::UnsupportedHandoff {
            src: src_provider.clone(),
            dst: dst_provider.clone(),
        });
    }

    if src_provider == dst_provider {
        return Ok(msgs.to_vec());
    }

    let mut out = Vec::with_capacity(msgs.len());
    for msg in msgs {
        let converted = convert_message_dyn(src_provider.clone(), dst_provider.clone(), msg)
            .map_err(|_err| WorkflowError::UnsupportedHandoff {
                src: src_provider.clone(),
                dst: dst_provider.clone(),
            })?;
        out.push(converted);
    }
    Ok(out)
}

/// Extract text-only assistant messages a completed task contributes to its successors.
///
/// v1 handoff deliberately carries only the response text as one assistant
/// message. Tool-call replay across providers remains deferred; tool results
/// stay inside the provider boundary that produced them.
///
/// # Errors
///
/// Returns [`WorkflowError::UnsupportedHandoff`] when the response/provider
/// pairing cannot produce a typed text message for handoff.
pub fn extract_messages_for_handoff(
    response: &ProviderResponse,
    src_provider: &ProviderName,
) -> WorkflowResult<Vec<MessageNum>> {
    let text = response
        .adapter()
        .text()
        .map(|text| text.into_owned())
        .unwrap_or_default();

    match (response, src_provider) {
        (ProviderResponse::OpenAiChatCompletion(_), ProviderName::OpenAi)
        | (ProviderResponse::OpenAiResponses(_), ProviderName::OpenAi) => {
            Ok(vec![MessageNum::OpenAi(OpenAiChatMessage {
                role: "assistant".to_owned(),
                content: Some(OpenAiMessageContent::Text(text)),
                name: None,
                tool_calls: None,
                tool_call_id: None,
                refusal: None,
            })])
        }
        (ProviderResponse::AnthropicMessage(_), ProviderName::Anthropic) => {
            Ok(vec![MessageNum::Anthropic(AnthropicMessage {
                role: "assistant".to_owned(),
                content: vec![AnthropicContentBlock::Text {
                    text,
                    cache_control: None,
                    citations: None,
                }],
            })])
        }
        (ProviderResponse::GeminiGenerateContent(_), ProviderName::Google)
        | (ProviderResponse::VertexGenerateContent(_), ProviderName::Vertex) => {
            Ok(vec![MessageNum::Gemini(GoogleContent {
                role: "model".to_owned(),
                parts: vec![GooglePart::Text { text }],
            })])
        }
        _ => Err(WorkflowError::UnsupportedHandoff {
            src: src_provider.clone(),
            dst: src_provider.clone(),
        }),
    }
}
