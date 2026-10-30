//! Fixture answers of the internal `mock` adapter.
//!
//! A mock deployment answers buffered `OpenAI` Chat Completions inside the
//! gateway from its explicit rules: no client, credential, or network. Rules
//! are stateless, so concurrent calls never consume a shared response.

use serde_json::Value;
use skald_spec::wire::openai_chat::{OpenAiContentPart, OpenAiMessageContent};
use wyrd_spec::gateway::MockRule;

use super::{UnsupportedRequest, chat_request};

/// Response of the first rule in `rules` matching the Chat Completions `body`
/// addressed to `model`.
///
/// A rule matches when `model` equals its model exactly, its role, when
/// given, is the last message's role, and its literal text, when given,
/// occurs in some textual message content.
///
/// # Errors
///
/// Returns [`UnsupportedRequest`] naming `body` when it is not a Chat
/// Completions request, or `messages` when no rule matches; the reason never
/// repeats request content.
pub(super) fn answer(
    body: &Value,
    model: &str,
    rules: &[MockRule],
) -> Result<Value, UnsupportedRequest> {
    let request = chat_request(body)?;
    let last_role = request.messages.last().map(|message| message.role.as_str());
    let contains = |text: &str| {
        request.messages.iter().any(|message| {
            match &message.content {
            Some(OpenAiMessageContent::Text(content)) => content.contains(text),
            Some(OpenAiMessageContent::Parts(parts)) => parts.iter().any(|part| {
                matches!(part, OpenAiContentPart::Text { text: content } if content.contains(text))
            }),
            None => false,
        }
        })
    };
    rules
        .iter()
        .find(|rule| {
            rule.model.as_str() == model
                && rule
                    .last_message_role
                    .as_deref()
                    .is_none_or(|role| last_role == Some(role))
                && rule.contains.as_deref().is_none_or(contains)
        })
        .map(|rule| rule.response.clone())
        .ok_or_else(|| {
            UnsupportedRequest::new(
                "messages",
                "no mock rule matches the model and conversation",
            )
        })
}
