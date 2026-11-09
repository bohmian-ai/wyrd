//! Jev `systemone` request preparation and Chat Completions pass-through.
//!
//! A native `POST /v1/systemone` body decodes into the closed, typed Skald
//! [`JevRequest`], is bound-checked, and takes the deployment's native model;
//! its typed answer returns as Jev's own response. On the unified
//! `/v1/chat/completions` route Jev is a pass-through, not a translation: the
//! text of the last user message must deserialize as the typed Jev request
//! `{state, questions}`, which is forwarded without per-field validation, and
//! the typed answer returns serialized as JSON in the sole assistant message.
//! No chat conversation is mapped onto Jev questions.

use std::borrow::Cow;
use std::collections::BTreeMap;

use serde::Deserialize;
use serde_json::Value;
use skald_spec::wire::jev::{JevQuestion, JevRequest, JevRequestView, JevResponse};
use skald_spec::wire::openai_chat::{
    OpenAiChatChoice, OpenAiChatMessage, OpenAiChatRequestView, OpenAiChatResponse,
    OpenAiContentPartView, OpenAiMessageContent, OpenAiMessageContentView, OpenAiUsage,
};

use super::UnsupportedRequest;

/// Jev request carried in a chat message's text: the typed request without
/// its model, which the deployment selects.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ChatQuestions {
    /// Text the questions are about.
    state: String,
    /// Questions keyed by the name their answer returns under.
    questions: BTreeMap<String, JevQuestion>,
}

/// The rejection of a body that is not a Jev request.
fn not_jev(error: serde_json::Error) -> UnsupportedRequest {
    UnsupportedRequest::new("body", format!("is not a valid Jev request: {error}"))
}

/// Checks a native `systemone` `body` on its borrowed projection, applying
/// the documented Jev bounds with the deployment's native `model`.
///
/// # Errors
///
/// Returns [`UnsupportedRequest`] naming `body` when it is not a Jev request
/// (unknown members or question forms included), or the member that breaks a
/// documented Jev bound.
pub(super) fn check(body: &Value, model: &str) -> Result<(), UnsupportedRequest> {
    let mut request = JevRequestView::deserialize(body).map_err(not_jev)?;
    request.model = model;
    request
        .validate()
        .map_err(|error| UnsupportedRequest::new(error.field, error.reason))
}

/// Decodes a native `systemone` `body` [`check`] admitted, rewriting its
/// model to the deployment's native `model`.
///
/// # Errors
///
/// Returns [`UnsupportedRequest`] naming `body` when it does not decode.
pub(super) fn request(body: &Value, model: &str) -> Result<JevRequest, UnsupportedRequest> {
    let mut request = JevRequest::deserialize(body).map_err(not_jev)?;
    model.clone_into(&mut request.model);
    Ok(request)
}

/// Reads the typed Jev request from the text of the last user message of a
/// Chat Completions request.
///
/// Every other chat member and message is ignored: the pass-through carries
/// only the Jev request. Typed deserialization (closed question forms, no
/// unknown members) is the only check; the native route's documented bounds
/// are not applied, so the provider judges the request. Streaming is refused
/// because Jev answers whole. Only that message's text is decoded, joined
/// first when it is split across parts.
///
/// # Errors
///
/// Returns [`UnsupportedRequest`] naming `stream` for a streaming call, or
/// `messages` when the last user message holds no text that decodes as
/// `{state, questions}` (a `model` member there included).
pub(super) fn chat_questions(
    chat: &OpenAiChatRequestView<'_>,
) -> Result<ChatQuestions, UnsupportedRequest> {
    if chat.stream == Some(true) {
        return Err(UnsupportedRequest::new(
            "stream",
            "the jev adapter does not stream",
        ));
    }
    chat.messages
        .iter()
        .rev()
        .find(|message| message.role == "user")
        .and_then(|message| text(message.content.as_ref()?))
        .ok_or_else(|| "it carries no text".to_owned())
        .and_then(|text| {
            serde_json::from_str::<ChatQuestions>(&text).map_err(|error| error.to_string())
        })
        .map_err(|detail| {
            UnsupportedRequest::new(
                "messages",
                format!(
                    "the last user message must be the JSON text of a Jev request \
                     {{state, questions}}: {detail}"
                ),
            )
        })
}

/// The Jev request for `model` carrying `questions` that
/// [`chat_questions`] read from a Chat Completions body.
pub(super) fn chat_request(questions: ChatQuestions, model: &str) -> JevRequest {
    JevRequest {
        model: model.to_owned(),
        state: questions.state,
        questions: questions.questions,
    }
}

/// Text of message `content`, borrowed when whole, or `None` when any part is
/// not text.
fn text<'a>(content: &OpenAiMessageContentView<'a>) -> Option<Cow<'a, str>> {
    match content {
        OpenAiMessageContentView::Text(text) => Some(Cow::Borrowed(text)),
        OpenAiMessageContentView::Parts(parts) => parts
            .iter()
            .map(|part| match part {
                OpenAiContentPartView::Text { text } => Some(*text),
                _ => None,
            })
            .collect::<Option<String>>()
            .map(Cow::Owned),
    }
}

/// Returns a Jev `answer` as a Chat Completion named `id`.
///
/// The sole assistant message carries the JSON text of the whole typed
/// answer; the envelope names the model that answered. Jev usage maps to
/// `prompt_tokens` and `completion_tokens` and stays absent when Jev omits
/// it. Returns `None` when the answer does not serialize or its token total
/// overflows.
pub(super) fn chat_response(answer: &JevResponse, id: &str) -> Option<OpenAiChatResponse> {
    let content = serde_json::to_string(answer).ok()?;
    let usage = match &answer.usage {
        Some(usage) => Some(OpenAiUsage {
            prompt_tokens: usage.input_tokens,
            completion_tokens: usage.output_tokens,
            total_tokens: usage.input_tokens.checked_add(usage.output_tokens)?,
            prompt_tokens_details: None,
            completion_tokens_details: None,
        }),
        None => None,
    };
    Some(OpenAiChatResponse {
        id: id.to_owned(),
        object: "chat.completion".to_owned(),
        created: super::now(),
        model: answer.model.clone(),
        choices: vec![OpenAiChatChoice {
            index: 0,
            message: OpenAiChatMessage {
                role: "assistant".to_owned(),
                content: Some(OpenAiMessageContent::Text(content)),
                ..OpenAiChatMessage::default()
            },
            finish_reason: Some("stop".to_owned()),
            logprobs: None,
        }],
        usage,
        system_fingerprint: None,
        service_tier: None,
    })
}
