//! Jev `systemone` request preparation and answer translation.
//!
//! Native `POST /v1/systemone` bodies decode through the closed Skald Jev
//! request, are bound-checked, and pass through with only `model` rewritten.
//! `OpenAI` Chat Completions reach Jev only in one explicit shape: a single
//! user message whose text is a JSON object holding `state` and `questions`.
//! Anything else — more turns, other roles, non-text content, tools,
//! streaming, generation controls, or a `model` inside the message — is
//! refused before provider IO, so ordinary chat is never reinterpreted as a
//! Jev question. A translated answer is a Chat Completion whose sole assistant
//! content is the JSON text `{"model", "answers"}`, never fabricated prose.

use serde_json::{Map, Value};
use skald_spec::wire::jev::{JevRequest, JevResponse};
use skald_spec::wire::openai_chat::{
    OpenAiChatChoice, OpenAiChatMessage, OpenAiChatResponse, OpenAiMessageContent, OpenAiUsage,
};

use super::UnsupportedRequest;

/// Chat Completions members the Jev projection reads; `stream` is accepted
/// only as `false`.
const CHAT_MEMBERS: [&str; 3] = ["model", "messages", "stream"];

/// Validates a native `systemone` `body` and rewrites its model to the
/// deployment's native `model`.
///
/// # Errors
///
/// Returns [`UnsupportedRequest`] naming `body` when it is not a Jev request
/// (unknown members or question forms included), or the member that breaks a
/// documented Jev bound.
pub(super) fn native(body: &Value, model: &str) -> Result<Value, UnsupportedRequest> {
    let request: JevRequest = serde_json::from_value(body.clone()).map_err(|error| {
        UnsupportedRequest::new("body", format!("is not a valid Jev request: {error}"))
    })?;
    checked(request)?;
    super::native(body, Some(model))
}

/// Projects a Chat Completions `body` onto the Jev request for `model`.
///
/// # Errors
///
/// Returns [`UnsupportedRequest`] naming the first chat member Jev cannot
/// honor, a message that is not exactly one plain user turn with text
/// content, message text that is not a JSON object of `state` and
/// `questions` (a `model` member there included), or a broken Jev bound.
pub(super) fn request(body: &Value, model: &str) -> Result<JevRequest, UnsupportedRequest> {
    let Value::Object(body) = body else {
        return Err(UnsupportedRequest::new("body", "must be a JSON object"));
    };
    if let Some((member, _)) = body.iter().find(|(member, value)| {
        (!CHAT_MEMBERS.contains(&member.as_str()) && !value.is_null())
            || (member.as_str() == "stream" && !matches!(value, Value::Bool(false) | Value::Null))
    }) {
        return Err(UnsupportedRequest::new(
            member.as_str(),
            "is not supported by the jev adapter",
        ));
    }
    let text = match body
        .get("messages")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
    {
        Some([message]) => user_text(message)?,
        _ => {
            return Err(UnsupportedRequest::new(
                "messages",
                "the jev adapter requires exactly one user message",
            ));
        }
    };
    let mut question: Map<String, Value> = match serde_json::from_str(&text) {
        Ok(Value::Object(object)) => object,
        _ => {
            return Err(UnsupportedRequest::new(
                "messages[0].content",
                "must be the JSON text of an object with `state` and `questions`",
            ));
        }
    };
    if question.contains_key("model") {
        return Err(UnsupportedRequest::new(
            "messages[0].content.model",
            "the top-level model selects the Jev model",
        ));
    }
    question.insert("model".to_owned(), Value::String(model.to_owned()));
    let request: JevRequest = serde_json::from_value(Value::Object(question)).map_err(|error| {
        UnsupportedRequest::new(
            "messages[0].content",
            format!("is not a valid Jev question set: {error}"),
        )
    })?;
    checked(request)
}

/// Text of the single chat `message`, which must be a plain user turn.
///
/// # Errors
///
/// Returns [`UnsupportedRequest`] for another role, any member besides
/// `role` and `content`, or content that is absent or not all text.
fn user_text(message: &Value) -> Result<String, UnsupportedRequest> {
    let Some(message) = message.as_object() else {
        return Err(UnsupportedRequest::new("messages[0]", "must be an object"));
    };
    if message.get("role").and_then(Value::as_str) != Some("user") {
        return Err(UnsupportedRequest::new(
            "messages[0].role",
            "the jev adapter requires a user message",
        ));
    }
    if let Some(member) = message
        .keys()
        .find(|member| !matches!(member.as_str(), "role" | "content"))
    {
        return Err(UnsupportedRequest::new(
            format!("messages[0].{member}"),
            "is not supported by the jev adapter",
        ));
    }
    match message.get("content") {
        Some(Value::String(text)) => Ok(text.clone()),
        Some(Value::Array(parts)) if !parts.is_empty() => parts
            .iter()
            .enumerate()
            .map(|(index, part)| match (part.get("type"), part.get("text")) {
                (Some(Value::String(kind)), Some(Value::String(text))) if kind == "text" => {
                    Ok(text.as_str())
                }
                _ => Err(UnsupportedRequest::new(
                    format!("messages[0].content[{index}]"),
                    "only text content is supported by the jev adapter",
                )),
            })
            .collect(),
        _ => Err(UnsupportedRequest::new(
            "messages[0].content",
            "must be text",
        )),
    }
}

/// Applies the documented Jev bounds to `request`.
///
/// # Errors
///
/// Returns [`UnsupportedRequest`] naming the member that breaks a bound.
fn checked(request: JevRequest) -> Result<JevRequest, UnsupportedRequest> {
    request
        .validate()
        .map_err(|error| UnsupportedRequest::new(error.field, error.reason))?;
    Ok(request)
}

/// Translates a Jev `answer` into a Chat Completion named `id`.
///
/// The sole assistant message carries the JSON text of Jev's resolved model
/// and intact answers; the envelope names the resolved model. Jev usage maps
/// to `prompt_tokens` and `completion_tokens`, and stays absent when Jev
/// omits it. Returns `None` when the answer is not a Jev response, its
/// content does not serialize, or its token total overflows.
pub(super) fn response(answer: &Value, id: &str) -> Option<OpenAiChatResponse> {
    let answer: JevResponse = serde_json::from_value(answer.clone()).ok()?;
    let content = serde_json::to_string(&serde_json::json!({
        "model": answer.model,
        "answers": answer.answers,
    }))
    .ok()?;
    let usage = match answer.usage {
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
        model: answer.model,
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
