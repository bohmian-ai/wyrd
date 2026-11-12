//! Provider protocol adapters for one attempt.
//!
//! [`prepare`] turns a caller body into the request a deployment's adapter
//! speaks. A body already in the deployment's dialect passes through with only
//! its model rewritten to the native identifier; `OpenAI` Chat Completions
//! ingress translates to Anthropic Messages or Google `GenerateContent` through
//! typed Skald wire structs. Anything the target cannot represent faithfully
//! is rejected before dispatch rather than dropped. Jev speaks only its own
//! `systemone` question protocol, carried as typed Skald [`JevRequest`] and
//! answers; Chat Completions reach it only as a pass-through of that typed
//! request in the last user message. [`complete`] maps the provider's answer back to the ingress
//! dialect and extracts usage; streamed answers relay through
//! [`stream::relay`].

mod anthropic;
mod batch;
mod google;
mod http;
mod jev;
mod media;
mod stream;

use std::borrow::Cow;
use std::time::{SystemTime, UNIX_EPOCH};

use base64::Engine as _;
use serde::Serialize;
use serde::de::{
    self, DeserializeOwned, DeserializeSeed, Deserializer, MapAccess, SeqAccess, Visitor,
};
use serde_json::value::{RawValue, to_raw_value};
use serde_json::{Map, Value, json};
use skald_providers::RequestBody;
use skald_spec::ProviderResponse;
use skald_spec::wire::anthropic_messages::{AnthropicMessagesRequest, AnthropicUsage};
use skald_spec::wire::borrowed::IgnoredObject;
use skald_spec::wire::google_generate::{GoogleGenerateContentRequest, GoogleUsageMetadata};
use skald_spec::wire::jev::{JevRequest, JevUsage};
use skald_spec::wire::openai_chat::{
    OpenAiChatMessage, OpenAiChatMessageView, OpenAiChatRequest, OpenAiChatRequestView,
    OpenAiChatResponse, OpenAiChatToolChoiceView, OpenAiContentPart, OpenAiContentPartView,
    OpenAiImageUrlView, OpenAiMessageContent, OpenAiMessageContentView, OpenAiStop,
    OpenAiStreamOptionsView, OpenAiToolCall, OpenAiToolCallView, OpenAiToolView, OpenAiUsage,
};
use skald_spec::wire::openai_embeddings::{
    OpenAiEmbeddingUsage, OpenAiEmbeddingsInputView, OpenAiEmbeddingsRequestView,
};
use skald_spec::wire::openai_responses::OpenAiResponsesUsage;
use wyrd_spec::gateway::native::{
    AnthropicError, AnthropicErrorEnvelope, GoogleError, GoogleErrorEnvelope, GoogleErrorInfo,
};
use wyrd_spec::gateway::openai::{OpenAiError, OpenAiErrorEnvelope};
use wyrd_spec::gateway::{
    GatewayOperation, GatewayUsageAmount, ProviderAdapter, ProviderDeployment,
};
use wyrd_spec::ids::ModelId;

use crate::credential::ProviderSecret;
use crate::engine::{AttemptUsage, JsonAnswer};
use crate::request::RequestJson;

pub use batch::{BATCH_ENDPOINTS, BatchAction, BatchInput};
pub use http::{BuiltinEndpoints, HttpProviderDispatch};
pub use media::MediaRequest;

/// Protocol of the request body a caller submitted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IngressDialect {
    /// OpenAI-compatible routes: Chat Completions, Responses, and Embeddings.
    OpenAi,
    /// Native Anthropic Messages produced by Skald.
    AnthropicMessages,
    /// Native Gemini `GenerateContent` produced by Skald.
    GeminiGenerateContent,
    /// Native Vertex `GenerateContent` produced by Skald.
    VertexGenerateContent,
    /// Native Jev `systemone` questions.
    Jev,
}

/// A requested operation or feature the selected deployment cannot represent.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{field}: {reason}")]
pub struct UnsupportedRequest {
    /// Request path of the unsupported member, such as `messages[2].name`.
    pub field: String,
    /// Stable human-readable reason.
    pub reason: String,
}

impl UnsupportedRequest {
    /// Builds the rejection of `field`.
    pub(crate) fn new(field: impl Into<String>, reason: impl Into<String>) -> Self {
        Self {
            field: field.into(),
            reason: reason.into(),
        }
    }
}

/// Upstream protocol family of a deployment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Wire {
    /// `OpenAI` routes, built-in or compatible.
    OpenAi,
    /// Anthropic Messages.
    Anthropic,
    /// Google `GenerateContent` on Gemini or Vertex.
    Google,
    /// Jev `systemone`.
    Jev,
}

impl Wire {
    /// Protocol family `adapter` speaks.
    pub(crate) const fn of(adapter: &ProviderAdapter) -> Self {
        match adapter {
            ProviderAdapter::OpenAi | ProviderAdapter::OpenAiCompatible { .. } => Self::OpenAi,
            ProviderAdapter::Anthropic => Self::Anthropic,
            ProviderAdapter::Gemini | ProviderAdapter::Vertex { .. } => Self::Google,
            ProviderAdapter::Jev => Self::Jev,
        }
    }
}

/// Request ready for one deployment.
///
/// A native request borrows the caller's body, so planning and dispatch never
/// copy it; translated requests own their typed target bodies.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Prepared<'a> {
    /// A body already in the deployment's dialect, sent unchanged except for
    /// its model; the answer returns unchanged.
    Native(NativeBody<'a>),
    /// Chat Completions translated to Anthropic Messages; the answer
    /// translates back. Boxed so the variant does not inflate every `Native`.
    Anthropic(Box<AnthropicMessagesRequest>),
    /// Chat Completions translated to Google `GenerateContent`; the answer
    /// translates back. Boxed so the variant does not inflate every `Native`.
    Google(Box<GoogleGenerateContentRequest>),
    /// A typed Jev question request with the deployment's model; the typed
    /// answer returns as Jev's own response. Boxed so the variant does not
    /// inflate every `Native`.
    Jev(Box<JevRequest>),
}

/// A caller's native JSON object with the deployment's model applied on the wire.
///
/// The override is applied as the body serializes, never by copying it: a
/// present `model` member keeps its position with the new value, and an
/// absent one follows the caller's members. Every other member serializes
/// exactly as submitted.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct NativeBody<'a> {
    /// The caller's object body, shared by every attempt of the call.
    body: &'a RequestJson,
    /// The deployment's native model, when the dialect carries it in the body.
    model: Option<&'a str>,
}

impl NativeBody<'_> {
    /// Serializes the body with its model applied as compact JSON bytes,
    /// through [`RequestJson::to_json`].
    ///
    /// # Errors
    ///
    /// Returns the serialization error of a member serde cannot write.
    pub(crate) fn to_json(self) -> serde_json::Result<Vec<u8>> {
        self.body.to_json(self.model)
    }
}

impl<'a> From<NativeBody<'a>> for RequestBody<'a> {
    /// The decoded body as the provider encoders take it, its model still
    /// unapplied.
    fn from(body: NativeBody<'a>) -> Self {
        match body.body.tree() {
            Value::Object(members) => Self::Object {
                members,
                model: body.model,
            },
            other => Self::Json(other),
        }
    }
}

impl Serialize for NativeBody<'_> {
    /// Serializes through [`RequestBody`], the encoders' one member order.
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        RequestBody::from(*self).serialize(serializer)
    }
}

/// The upstream form [`check`] selects for a representable request, carrying
/// only borrowed input, so selecting it builds no provider payload.
#[derive(Debug)]
pub(crate) enum Target<'a> {
    /// The caller's body, already in the deployment's dialect.
    Native(NativeBody<'a>),
    /// A native Jev `systemone` body for a Jev deployment.
    Jev,
    /// The Jev request carried by a Chat Completions body, decoded from the
    /// last user message's text.
    JevChat(jev::ChatQuestions),
    /// A Chat Completions body for an Anthropic deployment.
    Anthropic,
    /// A Chat Completions body for a Gemini or Vertex deployment.
    Google,
}

/// Checks that `deployment` can represent `body` for `model` and selects its
/// upstream form without building it.
///
/// Planning runs it for every candidate before admission, and [`prepare`]
/// runs it again for the dispatched attempt, so both apply one set of rules.
/// Translated bodies are checked through their borrowed projections: no
/// request text is copied and no target payload is built. Only the text of a
/// Jev pass-through's last user message is decoded, as the request it
/// carries.
///
/// # Errors
///
/// Returns [`UnsupportedRequest`] for an operation the adapter does not carry,
/// a native body sent to a different dialect, streaming of an operation that
/// does not stream, a `stream` member that disagrees with `stream`, a `media`
/// route or file set [`media::validate`] rejects, or any translated feature
/// without a faithful target representation. Images and Audio pass through
/// only to `OpenAI`-protocol adapters, as do Batches lifecycle bodies. Jev
/// `systemone` bodies reach only Jev deployments; a Chat Completions body
/// reaches Jev only as the typed Jev request in its last user message.
pub(crate) fn check<'a>(
    ingress: IngressDialect,
    operation: GatewayOperation,
    stream: bool,
    body: &'a RequestJson,
    deployment: &ProviderDeployment,
    model: &'a ModelId,
    media: Option<&MediaRequest>,
) -> Result<Target<'a>, UnsupportedRequest> {
    let model = model.as_str();
    media::validate(operation, media, body)?;
    let chat = operation == GatewayOperation::ChatCompletions;
    if stream && !operation.streams() {
        return Err(UnsupportedRequest::new(
            "stream",
            format!("{operation:?} does not stream"),
        ));
    }
    // Google dialects carry streaming in the method, not the body.
    if matches!(
        ingress,
        IngressDialect::OpenAi | IngressDialect::AnthropicMessages
    ) && body
        .member("stream")
        .and_then(|member| member.as_bool())
        .unwrap_or(false)
        != stream
    {
        return Err(UnsupportedRequest::new(
            "stream",
            "must match the call's streaming mode",
        ));
    }
    match (ingress, &deployment.adapter) {
        (
            IngressDialect::OpenAi,
            ProviderAdapter::OpenAi | ProviderAdapter::OpenAiCompatible { .. },
        ) => match operation {
            GatewayOperation::ChatCompletions
            | GatewayOperation::Responses
            | GatewayOperation::Images
            | GatewayOperation::Audio => Ok(Target::Native(native(body, Some(model))?)),
            GatewayOperation::Embeddings => {
                embeddings(body)?;
                Ok(Target::Native(native(body, Some(model))?))
            }
            // A batch body carries no model; its input file names the model.
            GatewayOperation::Batches => Ok(Target::Native(native(body, None)?)),
            GatewayOperation::SystemOne => {
                Err(unsupported_operation(operation, &deployment.adapter))
            }
        },
        (IngressDialect::Jev, ProviderAdapter::Jev) if operation == GatewayOperation::SystemOne => {
            jev::check(body, model)?;
            Ok(Target::Jev)
        }
        (IngressDialect::OpenAi, ProviderAdapter::Jev) if chat => {
            Ok(Target::JevChat(jev::chat_questions(&chat_view(body)?)?))
        }
        (IngressDialect::OpenAi, ProviderAdapter::Anthropic) if chat => {
            anthropic::check(&chat_view(body)?)?;
            Ok(Target::Anthropic)
        }
        (IngressDialect::OpenAi, ProviderAdapter::Gemini | ProviderAdapter::Vertex { .. })
            if chat =>
        {
            google::check(&chat_view(body)?)?;
            Ok(Target::Google)
        }
        (IngressDialect::AnthropicMessages, ProviderAdapter::Anthropic) if chat => {
            Ok(Target::Native(native(body, Some(model))?))
        }
        (
            IngressDialect::GeminiGenerateContent,
            ProviderAdapter::Gemini | ProviderAdapter::Vertex { .. },
        )
        | (IngressDialect::VertexGenerateContent, ProviderAdapter::Vertex { .. })
            if chat =>
        {
            Ok(Target::Native(native(body, None)?))
        }
        (IngressDialect::OpenAi, _) => Err(unsupported_operation(operation, &deployment.adapter)),
        (dialect, adapter) => Err(UnsupportedRequest::new(
            "body",
            format!(
                "a native {dialect:?} request cannot be sent to the {} adapter",
                adapter_name(adapter)
            ),
        )),
    }
}

/// Builds the upstream request of `body` for `model` on `deployment`, for
/// the attempt being dispatched.
///
/// Pure and deterministic: [`check`] selects the form and applies every
/// representability rule, then only that form is built, and nothing is kept
/// once the attempt ends. A native request only borrows `body`, so it is
/// never copied.
///
/// # Errors
///
/// Returns what [`check`] returns, or names `body` when a checked body does
/// not decode as its owned request.
pub(crate) fn prepare<'a>(
    ingress: IngressDialect,
    operation: GatewayOperation,
    stream: bool,
    body: &'a RequestJson,
    deployment: &ProviderDeployment,
    model: &'a ModelId,
    media: Option<&MediaRequest>,
) -> Result<Prepared<'a>, UnsupportedRequest> {
    Ok(
        match check(ingress, operation, stream, body, deployment, model, media)? {
            Target::Native(native) => Prepared::Native(native),
            Target::Jev => Prepared::Jev(Box::new(jev::request(body, model.as_str())?)),
            Target::JevChat(questions) => {
                Prepared::Jev(Box::new(jev::chat_request(questions, model.as_str())))
            }
            Target::Anthropic => Prepared::Anthropic(Box::new(anthropic::request(
                chat_request(body)?,
                model.as_str(),
            ))),
            Target::Google => Prepared::Google(Box::new(google::request(chat_request(body)?))),
        },
    )
}

/// A successful provider answer as the adapter receives it.
#[derive(Debug)]
pub(crate) enum Answer {
    /// Answer text of a native passthrough, not yet validated as JSON.
    Native(String),
    /// Answer of a translated request, decoded by its Skald client.
    Typed(Box<ProviderResponse>),
}

/// Why a successful provider answer does not complete the attempt.
#[derive(Debug)]
pub(crate) enum Unmapped {
    /// Native answer text that is not JSON at all, which may succeed at
    /// another deployment.
    Undecodable(serde_json::Error),
    /// An answer the ingress dialect cannot represent, with the usage that
    /// could be read.
    Unrepresentable(AttemptUsage),
}

/// Maps a successful provider answer to the ingress dialect and extracts its
/// usage.
///
/// Native answer text is validated in the same single pass that reads its
/// usage, read as `wire`, and its bytes return unchanged. When `capture`
/// selects it, that pass is its whole decode, returned for the capture
/// projection; otherwise only its usage member is decoded, under the same
/// rules. Every other answer returns no decoded copy. Typed Anthropic and
/// Google answers translate to a chat completion, serialized once and named
/// `id` when the provider supplies no usable identifier. A typed Jev answer
/// returns as Jev's own response with its usage, or, when `translated` (the
/// Chat Completions pass-through), as a chat completion whose content is
/// that answer's JSON.
///
/// # Errors
///
/// Returns [`Unmapped::Undecodable`] for native text that is not one JSON
/// value. Returns [`Unmapped::Unrepresentable`] with the usage that could be
/// read for native JSON a value decode rejects, such as an out-of-range
/// number or excess nesting, when a translated answer carries content, a
/// finish state, or a shape the ingress dialect cannot represent, or when the
/// answer is of a kind no prepared request produces.
pub(crate) fn complete(
    response: Answer,
    wire: Wire,
    translated: bool,
    capture: bool,
    id: &str,
    model: &str,
) -> Result<(JsonAnswer, AttemptUsage, Option<Value>), Unmapped> {
    let chat_completion = |usage: AttemptUsage, chat: Option<OpenAiChatResponse>| {
        chat.and_then(|chat| to_raw_value(&chat).ok())
            .map(|chat| (chat.into(), usage.clone(), None))
            .ok_or(Unmapped::Unrepresentable(usage))
    };
    let response = match response {
        Answer::Native(text) => return native_answer(text, wire, capture),
        Answer::Typed(response) => *response,
    };
    match response {
        ProviderResponse::AnthropicMessage(native) => {
            let usage = usage(
                Wire::Anthropic,
                &serde_json::to_value(&native).unwrap_or_default(),
            );
            chat_completion(usage, anthropic::response(native, model))
        }
        ProviderResponse::GeminiGenerateContent(native) => {
            let usage = usage(
                Wire::Google,
                &serde_json::to_value(&native).unwrap_or_default(),
            );
            chat_completion(usage, google::response(native, id, model))
        }
        ProviderResponse::JevSystemOne(answer) => {
            let usage = usage(
                Wire::Jev,
                &serde_json::to_value(&answer).unwrap_or_default(),
            );
            if translated {
                return chat_completion(usage, jev::chat_response(&answer, id));
            }
            to_raw_value(&answer)
                .map(|body| (body.into(), usage.clone(), None))
                .map_err(|_| Unmapped::Unrepresentable(usage))
        }
        _ => Err(Unmapped::Unrepresentable(AttemptUsage::default())),
    }
}

/// Validates native answer `text` once while reading its `wire` usage: as a
/// whole [`Value`] when `capture` selects it, otherwise through
/// [`usage_member`].
///
/// A value decode accepts only text a [`RawValue`] accepts, so a decoded
/// answer is checked JSON. Only a rejected answer is scanned again, to tell
/// text that is not JSON from JSON a value decode refuses.
///
/// # Errors
///
/// Returns [`Unmapped::Undecodable`] when `text` is not one JSON value, and
/// [`Unmapped::Unrepresentable`] with no usage for JSON a value decode
/// rejects.
fn native_answer(
    text: String,
    wire: Wire,
    capture: bool,
) -> Result<(JsonAnswer, AttemptUsage, Option<Value>), Unmapped> {
    let read = if capture {
        serde_json::from_str::<Value>(&text).map(|body| (usage(wire, &body), Some(body)))
    } else {
        usage_member(wire, &text).map(|member| {
            let usage = member.map_or_else(AttemptUsage::default, |member| counted(wire, &member));
            (usage, None)
        })
    };
    match read {
        Ok((usage, decoded)) => Ok((JsonAnswer::validated(text), usage, decoded)),
        Err(_) => match serde_json::from_str::<&RawValue>(&text) {
            Err(error) => Err(Unmapped::Undecodable(error)),
            Ok(_) => Err(Unmapped::Unrepresentable(AttemptUsage::default())),
        },
    }
}

/// Renders a non-success provider answer for the caller.
///
/// Every exact occurrence of the attempt's resolved `credential` is removed
/// from the raw text and again from every decoded JSON string and key, so a
/// provider echoing it, even behind JSON escapes, never relays it; a body
/// holding it as valid base64, in a JSON string, key, or non-JSON body, relays
/// only its status. Native
/// answers keep the remaining provider body (non-JSON text becomes a JSON
/// string); translated answers become the `OpenAI` error envelope carrying the
/// provider's message.
pub(crate) fn refusal(
    translated: bool,
    status: u16,
    body: &str,
    credential: Option<&ProviderSecret>,
) -> Value {
    let secret = credential
        .map(ProviderSecret::expose)
        .filter(|secret| !secret.is_empty());
    let body = secret.map_or_else(|| body.to_owned(), |secret| body.replace(secret, ""));
    let parsed = serde_json::from_str::<Value>(&body)
        .ok()
        .map(|value| match secret {
            Some(secret) => without_secret(value, secret),
            None => value,
        });
    // An encoded credential cannot be removed without guessing at the
    // provider's content, so only the status is relayed.
    if secret.is_some_and(|secret| match &parsed {
        Some(value) => holds_encoded_secret(value, secret),
        None => encodes_secret(&body, secret),
    }) {
        return openai_error_body(
            status,
            &format!("upstream provider answered HTTP {status}"),
            None,
            None,
        );
    }
    if !translated {
        return parsed.unwrap_or(Value::String(body));
    }
    let message = parsed
        .as_ref()
        .and_then(|value| value.pointer("/error/message"))
        .and_then(Value::as_str)
        .map_or_else(
            || format!("upstream provider answered HTTP {status}"),
            str::to_owned,
        );
    openai_error_body(status, &message, None, None)
}

/// Removes every exact occurrence of `secret` from the decoded strings and
/// object keys of `value`.
///
/// The value is edited in place: only a string or an object's keys holding
/// `secret` are rebuilt, so secret-free content keeps its allocations.
pub(crate) fn without_secret(mut value: Value, secret: &str) -> Value {
    remove_secret(&mut value, secret);
    value
}

/// Removes `secret` from `value` in place, as [`without_secret`] does.
fn remove_secret(value: &mut Value, secret: &str) {
    match value {
        Value::String(text) if text.contains(secret) => *text = text.replace(secret, ""),
        Value::Array(items) => {
            for item in items {
                remove_secret(item, secret);
            }
        }
        Value::Object(object) => {
            if object.keys().any(|key| key.contains(secret)) {
                *object = std::mem::take(object)
                    .into_iter()
                    .map(|(key, item)| (key.replace(secret, ""), item))
                    .collect();
            }
            for item in object.values_mut() {
                remove_secret(item, secret);
            }
        }
        _ => {}
    }
}

/// Whether any string or object key in `value`, read as standard base64 or
/// as the content of a base64 `data:` URL, decodes to bytes holding the
/// non-empty `secret`.
///
/// Selected capture decodes inline binary content into governed objects and
/// retains member names in canonical JSON, so a credential encoded as valid
/// base64 survives [`without_secret`]. Every string and key is judged, a
/// superset of capture's inline binary shapes, so a new shape there cannot
/// silently escape this judgment.
pub(crate) fn holds_encoded_secret(value: &Value, secret: &str) -> bool {
    match value {
        Value::String(text) => encodes_secret(text, secret),
        Value::Array(items) => items.iter().any(|item| holds_encoded_secret(item, secret)),
        Value::Object(object) => object
            .iter()
            .any(|(key, item)| encodes_secret(key, secret) || holds_encoded_secret(item, secret)),
        _ => false,
    }
}

/// Whether `text`, read as standard base64 or as the content of a base64
/// `data:` URL, decodes to bytes holding the non-empty `secret`.
fn encodes_secret(text: &str, secret: &str) -> bool {
    if secret.is_empty() {
        return false;
    }
    let encoded = text
        .strip_prefix("data:")
        .and_then(|rest| rest.split_once(";base64,"))
        .map_or(text, |(_, encoded)| encoded);
    // Base64 decodes at most three bytes per four characters, so text too
    // short to hold the secret is judged without decoding it.
    if encoded.len().saturating_mul(3) < secret.len().saturating_mul(4) {
        return false;
    }
    base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .is_ok_and(|bytes| {
            bytes
                .windows(secret.len())
                .any(|window| window == secret.as_bytes())
        })
}

/// Renders the `OpenAI` error envelope `{"error": {message, type, param, code}}`.
///
/// `type` follows `status` the way `OpenAI` SDKs classify errors. Translated
/// provider refusals carry no code or parameter; gateway-originated errors
/// pass their stable Wyrd code and offending request field.
#[must_use]
pub fn openai_error_body(
    status: u16,
    message: &str,
    code: Option<&str>,
    param: Option<&str>,
) -> Value {
    let kind = match status {
        401 => "authentication_error",
        403 => "permission_error",
        404 => "not_found_error",
        429 => "rate_limit_error",
        400..=499 => "invalid_request_error",
        _ => "api_error",
    };
    let envelope = OpenAiErrorEnvelope {
        error: OpenAiError {
            message: message.to_owned(),
            kind: kind.to_owned(),
            param: param.map(str::to_owned),
            code: code.map(str::to_owned),
        },
    };
    serde_json::to_value(envelope).unwrap_or(Value::Null)
}

/// Renders the Anthropic error envelope
/// `{"type": "error", "error": {type, message, code}}`.
///
/// `error.type` follows `status` the way Anthropic SDKs classify errors;
/// `code` carries the stable Wyrd code of a gateway-originated error.
#[must_use]
pub fn anthropic_error_body(status: u16, message: &str, code: Option<&str>) -> Value {
    let kind = match status {
        401 => "authentication_error",
        403 => "permission_error",
        404 => "not_found_error",
        413 => "request_too_large",
        429 => "rate_limit_error",
        504 => "timeout_error",
        503 | 529 => "overloaded_error",
        400..=499 => "invalid_request_error",
        _ => "api_error",
    };
    let envelope = AnthropicErrorEnvelope {
        kind: "error".to_owned(),
        error: AnthropicError {
            kind: kind.to_owned(),
            message: message.to_owned(),
            code: code.map(str::to_owned),
        },
    };
    serde_json::to_value(envelope).unwrap_or(Value::Null)
}

/// Renders the Google error envelope `{"error": {code, message, status,
/// details}}` whose single `ErrorInfo` detail names the stable Wyrd `code`.
///
/// `status` maps to the canonical Google RPC status name Google SDKs expose.
#[must_use]
pub fn google_error_body(status: u16, message: &str, code: &str) -> Value {
    let name = match status {
        401 => "UNAUTHENTICATED",
        403 => "PERMISSION_DENIED",
        404 => "NOT_FOUND",
        409 => "ABORTED",
        422 => "FAILED_PRECONDITION",
        429 => "RESOURCE_EXHAUSTED",
        499 => "CANCELLED",
        501 => "UNIMPLEMENTED",
        502 | 503 => "UNAVAILABLE",
        504 => "DEADLINE_EXCEEDED",
        400..=499 => "INVALID_ARGUMENT",
        _ => "INTERNAL",
    };
    let envelope = GoogleErrorEnvelope {
        error: GoogleError {
            code: status,
            message: message.to_owned(),
            status: name.to_owned(),
            details: vec![GoogleErrorInfo {
                kind: "type.googleapis.com/google.rpc.ErrorInfo".to_owned(),
                reason: code.to_owned(),
                domain: "wyrd".to_owned(),
            }],
        },
    };
    serde_json::to_value(envelope).unwrap_or(Value::Null)
}

/// Name of `adapter` as it appears on the wire.
pub(crate) fn adapter_name(adapter: &ProviderAdapter) -> &'static str {
    adapter.builtin_provider().unwrap_or("openai_compatible")
}

/// Rejection of `operation` on `adapter`.
fn unsupported_operation(
    operation: GatewayOperation,
    adapter: &ProviderAdapter,
) -> UnsupportedRequest {
    UnsupportedRequest::new(
        "operation",
        format!(
            "{operation:?} is not supported by the {} adapter",
            adapter_name(adapter)
        ),
    )
}

/// Borrows a native body, applying `model` when the dialect carries it in the
/// body.
///
/// # Errors
///
/// Returns [`UnsupportedRequest`] for a non-object body.
fn native<'a>(
    body: &'a RequestJson,
    model: Option<&'a str>,
) -> Result<NativeBody<'a>, UnsupportedRequest> {
    if !body.is_object() {
        return Err(UnsupportedRequest::new("body", "must be a JSON object"));
    }
    Ok(NativeBody { body, model })
}

/// Largest number of inputs one embeddings request may carry: the `OpenAI`
/// per-request maximum, so a compatible provider never receives a batch the
/// protocol does not define.
const MAX_EMBEDDING_INPUTS: usize = 2048;

/// Validates an `OpenAI` embeddings body before it passes through unchanged.
///
/// The body decodes through the borrowed projection of the typed Skald
/// request, so input is a string, a token array, or a batch of either, and no
/// input text is copied; a batch keeps its order because the body
/// is forwarded as submitted and the provider answers per index.
///
/// # Errors
///
/// Returns [`UnsupportedRequest`] naming `body` when it is not an embeddings
/// request, `input` when it is empty, carries an empty item, or exceeds
/// [`MAX_EMBEDDING_INPUTS`], `dimensions` when it is zero, and
/// `encoding_format` for a format other than `float` or `base64`.
fn embeddings(body: &RequestJson) -> Result<(), UnsupportedRequest> {
    let request = body
        .view::<OpenAiEmbeddingsRequestView>()
        .map_err(|error| {
            UnsupportedRequest::new(
                "body",
                format!("is not a valid embeddings request: {error}"),
            )
        })?;
    let (count, empty_item) = inputs(&request.input);
    if count == 0 || empty_item {
        return Err(UnsupportedRequest::new(
            "input",
            "must carry at least one non-empty item",
        ));
    }
    if count > MAX_EMBEDDING_INPUTS {
        return Err(UnsupportedRequest::new(
            "input",
            format!("must carry at most {MAX_EMBEDDING_INPUTS} items"),
        ));
    }
    if request.dimensions == Some(0) {
        return Err(UnsupportedRequest::new("dimensions", "must be positive"));
    }
    if request
        .encoding_format
        .is_some_and(|format| !matches!(format, "float" | "base64"))
    {
        return Err(UnsupportedRequest::new(
            "encoding_format",
            "must be float or base64",
        ));
    }
    Ok(())
}

/// Counts the items of an embeddings `input` and reports whether any is empty.
fn inputs(input: &OpenAiEmbeddingsInputView<'_>) -> (usize, bool) {
    match input {
        OpenAiEmbeddingsInputView::One(text) => (1, text.is_empty()),
        OpenAiEmbeddingsInputView::Tokens(tokens) => (1, tokens.is_empty()),
        OpenAiEmbeddingsInputView::Many(items) => {
            (items.len(), items.iter().any(|item| item.is_empty()))
        }
        OpenAiEmbeddingsInputView::TokenBatches(items) => {
            (items.len(), items.iter().any(Vec::is_empty))
        }
    }
}

/// True when a successful embeddings `answer` honors its `request`, read
/// through the request's borrowed projection.
///
/// The answer must carry one item per input in input order (`data[i].index ==
/// i`). Every vector must use the requested representation: an array of
/// numbers when `encoding_format` is omitted or `float`, a base64 string when
/// it is `base64`. A base64 vector must decode under the standard padded
/// alphabet to a whole number of `f32` values. When `dimensions` was requested
/// every vector must have exactly that many values. A provider silently
/// ignoring any of these would weaken the request, so the attempt fails
/// instead of returning the answer.
pub(crate) fn faithful_embeddings(request: &RequestJson, answer: &str) -> bool {
    let (Ok(request), Ok(answer)) = (
        request.view::<OpenAiEmbeddingsRequestView>(),
        serde_json::from_str::<Value>(answer),
    ) else {
        return false;
    };
    let (count, _) = inputs(&request.input);
    let Some(data) = answer.get("data").and_then(Value::as_array) else {
        return false;
    };
    let base64 = request.encoding_format == Some("base64");
    data.len() == count
        && data.iter().enumerate().all(|(position, item)| {
            let ordered = item
                .get("index")
                .and_then(Value::as_u64)
                .is_some_and(|index| usize::try_from(index).ok() == Some(position));
            let length = match (base64, item.get("embedding")) {
                (false, Some(Value::Array(vector))) => {
                    vector.iter().all(Value::is_number).then_some(vector.len())
                }
                (true, Some(Value::String(encoded))) => base64_floats(encoded),
                _ => None,
            };
            ordered
                && length.is_some_and(|length| {
                    request
                        .dimensions
                        .is_none_or(|dimensions| usize::try_from(dimensions).ok() == Some(length))
                })
        })
}

/// Number of little-endian `f32` values a standard padded base64 string
/// decodes to, or `None` when it does not decode or its bytes are not a whole
/// number of values.
fn base64_floats(encoded: &str) -> Option<usize> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .ok()?;
    bytes.len().is_multiple_of(4).then_some(bytes.len() / 4)
}

/// Rejects what a translated stream cannot represent faithfully: obfuscation
/// padding and streamed tool calls.
///
/// # Errors
///
/// Returns [`UnsupportedRequest`] naming `stream_options.include_obfuscation`
/// or `tools`.
fn streamable(
    adapter: &str,
    stream: Option<bool>,
    options: Option<&OpenAiStreamOptionsView<'_>>,
    tools: bool,
) -> Result<(), UnsupportedRequest> {
    let streaming = stream == Some(true);
    reject_present(
        adapter,
        &[
            (
                "stream_options.include_obfuscation",
                options.is_some_and(|options| options.include_obfuscation == Some(true)),
            ),
            ("tools", streaming && tools),
        ],
    )
}

/// Decodes an `OpenAI` Chat Completions body through its borrowed
/// projection, for checks that copy no request text.
///
/// # Errors
///
/// Returns [`UnsupportedRequest`] naming the decode failure.
fn chat_view(body: &RequestJson) -> Result<OpenAiChatRequestView<'_>, UnsupportedRequest> {
    body.view().map_err(not_chat)
}

/// Decodes a checked `OpenAI` Chat Completions body into its typed Skald
/// form, for the target being built.
///
/// # Errors
///
/// Returns [`UnsupportedRequest`] naming the decode failure.
fn chat_request(body: &RequestJson) -> Result<OpenAiChatRequest, UnsupportedRequest> {
    body.view().map_err(not_chat)
}

/// The rejection of a body that is not a chat completion request.
fn not_chat(error: serde_json::Error) -> UnsupportedRequest {
    UnsupportedRequest::new(
        "body",
        format!("is not a valid chat completion request: {error}"),
    )
}

/// Rejects every present member of `checks`, named by field.
///
/// # Errors
///
/// Returns [`UnsupportedRequest`] for the first present field.
fn reject_present(adapter: &str, checks: &[(&str, bool)]) -> Result<(), UnsupportedRequest> {
    match checks.iter().find(|(_, present)| *present) {
        Some((field, _)) => Err(UnsupportedRequest::new(
            *field,
            format!("is not supported by the {adapter} adapter"),
        )),
        None => Ok(()),
    }
}

/// Rejects the first unknown pass-through member.
///
/// # Errors
///
/// Returns [`UnsupportedRequest`] naming the first key of `extra`.
fn reject_extra(adapter: &str, extra: &IgnoredObject<'_>) -> Result<(), UnsupportedRequest> {
    match &extra.first_key {
        Some(key) => reject_present(adapter, &[(key, true)]),
        None => Ok(()),
    }
}

/// Whether `role` is that of a system or developer message.
fn is_system(role: &str) -> bool {
    matches!(role, "system" | "developer")
}

/// Checks the leading system and developer messages and returns the index of
/// the first conversational message.
///
/// # Errors
///
/// Returns [`UnsupportedRequest`] for a later system message, whose position
/// no target can represent, or non-text system content.
fn system_prefix(messages: &[OpenAiChatMessageView<'_>]) -> Result<usize, UnsupportedRequest> {
    let offset = messages.iter().take_while(|m| is_system(m.role)).count();
    if let Some(index) = messages.iter().skip(offset).position(|m| is_system(m.role)) {
        return Err(UnsupportedRequest::new(
            format!("messages[{}].role", index + offset),
            "system messages must precede the conversation for this adapter",
        ));
    }
    for (index, message) in messages[..offset].iter().enumerate() {
        let field = format!("messages[{index}]");
        plain_message(message, &field)?;
        text_only(message.content.as_ref(), &field)?;
    }
    Ok(offset)
}

/// Joined text of the leading system and developer messages of checked
/// `messages`, or `None`, and the index of the first conversational message.
fn system_text(messages: &[OpenAiChatMessage]) -> (Option<String>, usize) {
    let offset = messages.iter().take_while(|m| is_system(&m.role)).count();
    let texts: Vec<String> = messages[..offset]
        .iter()
        .map(|message| text_content(message.content.as_ref()).unwrap_or_default())
        .collect();
    ((!texts.is_empty()).then(|| texts.join("\n\n")), offset)
}

/// Rejects message members no translated target represents.
///
/// # Errors
///
/// Returns [`UnsupportedRequest`] for `name`, `refusal`, `audio`,
/// annotations, `tool_calls` outside assistant messages, or `tool_call_id`
/// outside tool messages.
fn plain_message(
    message: &OpenAiChatMessageView<'_>,
    field: &str,
) -> Result<(), UnsupportedRequest> {
    let checks = [
        ("name", message.name.is_some()),
        ("refusal", message.refusal.is_some()),
        ("audio", message.audio.is_some()),
        ("annotations", !message.annotations.is_empty()),
        (
            "tool_calls",
            message.role != "assistant" && message.tool_calls.is_some(),
        ),
        (
            "tool_call_id",
            message.role != "tool" && message.tool_call_id.is_some(),
        ),
    ];
    match checks.iter().find(|(_, present)| *present) {
        Some((member, _)) => Err(UnsupportedRequest::new(
            format!("{field}.{member}"),
            "is not supported for this message on a translated adapter",
        )),
        None => Ok(()),
    }
}

/// Checks that content is text only and returns whether its text is
/// non-empty, as [`text_content`] then yields it.
///
/// # Errors
///
/// Returns [`UnsupportedRequest`] when a content part is not text.
fn text_only(
    content: Option<&OpenAiMessageContentView<'_>>,
    field: &str,
) -> Result<bool, UnsupportedRequest> {
    match content {
        None => Ok(false),
        Some(OpenAiMessageContentView::Text(text)) => Ok(!text.is_empty()),
        Some(OpenAiMessageContentView::Parts(parts)) => {
            let mut text = false;
            for (index, part) in parts.iter().enumerate() {
                match part {
                    OpenAiContentPartView::Text { text: part } => text |= !part.is_empty(),
                    _ => {
                        return Err(UnsupportedRequest::new(
                            format!("{field}.content[{index}]"),
                            "only text content is supported for this message",
                        ));
                    }
                }
            }
            Ok(text)
        }
    }
}

/// Text of checked text-only content, or `None` when absent or empty.
fn text_content(content: Option<&OpenAiMessageContent>) -> Option<String> {
    let text = match content {
        None => String::new(),
        Some(OpenAiMessageContent::Text(text)) => text.clone(),
        Some(OpenAiMessageContent::Parts(parts)) => parts
            .iter()
            .filter_map(|part| match part {
                OpenAiContentPart::Text { text } => Some(text.as_str()),
                _ => None,
            })
            .collect(),
    };
    (!text.is_empty()).then_some(text)
}

/// Checks an image's detail hint and returns whether its URL is a base64
/// `data:` URL, which [`data_url`] splits.
///
/// # Errors
///
/// Returns [`UnsupportedRequest`] for a non-`auto` detail hint.
fn image(image: &OpenAiImageUrlView<'_>, field: &str) -> Result<bool, UnsupportedRequest> {
    if image.detail.is_some_and(|detail| detail != "auto") {
        return Err(UnsupportedRequest::new(
            format!("{field}.image_url.detail"),
            "image detail is not supported by a translated adapter",
        ));
    }
    Ok(data_url(image.url).is_some())
}

/// Splits a base64 `data:` image URL into media type and payload.
fn data_url(url: &str) -> Option<(&str, &str)> {
    url.strip_prefix("data:")?.split_once(";base64,")
}

/// Checks a tool call: a function whose arguments are a JSON object, checked
/// as raw JSON without building it.
///
/// # Errors
///
/// Returns [`UnsupportedRequest`] for a non-function call or arguments that
/// are not a JSON object.
fn arguments(call: &OpenAiToolCallView<'_>, field: &str) -> Result<(), UnsupportedRequest> {
    match serde_json::from_str::<&RawValue>(call.function.arguments) {
        Ok(raw) if call.kind == "function" && raw.get().starts_with('{') => Ok(()),
        _ => Err(UnsupportedRequest::new(
            format!("{field}.tool_calls"),
            "tool calls must be functions whose arguments are a JSON object",
        )),
    }
}

/// Decoded arguments of a checked tool call; [`arguments`] admitted only a
/// JSON object, so the empty-object fallback is never taken.
fn arguments_value(call: &OpenAiToolCall) -> Value {
    serde_json::from_str(&call.function.arguments).unwrap_or_else(|_| json!({}))
}

/// Checks that every tool is a non-strict function.
///
/// # Errors
///
/// Returns [`UnsupportedRequest`] naming the first other tool.
fn function_tools(adapter: &str, tools: &[OpenAiToolView<'_>]) -> Result<(), UnsupportedRequest> {
    match tools.iter().position(|tool| {
        !matches!(tool, OpenAiToolView::Function { function } if function.strict != Some(true))
    }) {
        Some(index) => Err(UnsupportedRequest::new(
            format!("tools[{index}]"),
            format!("only non-strict function tools are supported by the {adapter} adapter"),
        )),
        None => Ok(()),
    }
}

/// Checks that a tool choice is a mode or a named function.
///
/// # Errors
///
/// Returns [`UnsupportedRequest`] for allowed-tools and custom-tool choices.
fn tool_choice(
    adapter: &str,
    choice: Option<&OpenAiChatToolChoiceView<'_>>,
) -> Result<(), UnsupportedRequest> {
    match choice {
        Some(OpenAiChatToolChoiceView::Allowed(_) | OpenAiChatToolChoiceView::Custom(_)) => {
            Err(UnsupportedRequest::new(
                "tool_choice",
                format!(
                    "allowed-tools and custom-tool choices are not supported by the {adapter} adapter"
                ),
            ))
        }
        _ => Ok(()),
    }
}

/// Stop sequences of an `OpenAI` `stop` member.
fn stop_sequences(stop: Option<OpenAiStop>) -> Option<Vec<String>> {
    stop.map(|stop| match stop {
        OpenAiStop::One(one) => vec![one],
        OpenAiStop::Many(many) => many,
    })
}

/// JSON Schema of a function's parameters; absent parameters accept an empty
/// object.
fn parameters(parameters: Option<Map<String, Value>>) -> Value {
    parameters.map_or_else(
        || json!({"type": "object", "properties": {}}),
        Value::Object,
    )
}

/// Current Unix time in seconds for translated completions.
fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs())
}

/// Reads token usage of `wire` from a provider answer.
///
/// The usage object decodes through the matching typed Skald usage — chat,
/// Responses, then embeddings usage for `OpenAI` — and only that projection
/// is kept, as canonical JSON, so unknown or oversized members never reach
/// accounting. Normalized input and output tokens feed pricing. A missing
/// object, one no typed usage decodes, or counts whose sum overflows leave
/// usage unknown.
fn usage(wire: Wire, body: &Value) -> AttemptUsage {
    usage_names(wire)
        .iter()
        .find_map(|name| body.get(name))
        .map_or_else(AttemptUsage::default, |object| counted(wire, object))
}

/// The top-level members a `wire` answer may carry its usage object in, in
/// precedence order.
const fn usage_names(wire: Wire) -> &'static [&'static str] {
    match wire {
        Wire::OpenAi | Wire::Anthropic | Wire::Jev => &["usage"],
        Wire::Google => &["usage_metadata", "usageMetadata"],
    }
}

/// Reads the usage member of a `wire` answer `body` without decoding the
/// rest.
///
/// `body` is read as `serde_json::from_str::<Value>` reads it, with the same
/// syntax, number, and nesting checks, top-level arrays and scalars included,
/// and the last of duplicate members kept, so the member is the one [`usage`]
/// finds in the whole decode. Only that member is decoded; every other value
/// is checked and dropped, so a metadata-only answer never owns its content.
///
/// # Errors
///
/// Returns the JSON error when `body` is not one JSON value or exceeds the
/// nesting limit.
fn usage_member(wire: Wire, body: &str) -> serde_json::Result<Option<Value>> {
    let mut deserializer = serde_json::Deserializer::from_str(body);
    let member = UsageMember(usage_names(wire)).deserialize(&mut deserializer)?;
    deserializer.end()?;
    Ok(member)
}

/// Decodes the first of `names`, at most two, that a JSON object holds, and
/// checks every other value without keeping it.
///
/// Every value is read through `deserialize_any`, so `serde_json` applies the
/// same nesting limit a [`Value`] decode does. With no names it only checks
/// a value, which is how nested values are read.
struct UsageMember(&'static [&'static str]);

impl<'de> DeserializeSeed<'de> for UsageMember {
    type Value = Option<Value>;

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<Self::Value, D::Error> {
        deserializer.deserialize_any(self)
    }
}

impl<'de> Visitor<'de> for UsageMember {
    type Value = Option<Value>;

    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("a JSON value")
    }

    fn visit_bool<E: de::Error>(self, _: bool) -> Result<Self::Value, E> {
        Ok(None)
    }

    fn visit_i64<E: de::Error>(self, _: i64) -> Result<Self::Value, E> {
        Ok(None)
    }

    fn visit_u64<E: de::Error>(self, _: u64) -> Result<Self::Value, E> {
        Ok(None)
    }

    fn visit_f64<E: de::Error>(self, _: f64) -> Result<Self::Value, E> {
        Ok(None)
    }

    fn visit_str<E: de::Error>(self, _: &str) -> Result<Self::Value, E> {
        Ok(None)
    }

    fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
        Ok(None)
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
        while seq.next_element_seed(Self(&[]))?.is_some() {}
        Ok(None)
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
        let mut found: [Option<Value>; 2] = [None, None];
        while let Some(Name(name)) = map.next_key()? {
            match self.0.iter().position(|wanted| *wanted == name) {
                Some(slot) => found[slot] = Some(map.next_value()?),
                None => {
                    map.next_value_seed(Self(&[]))?;
                }
            }
        }
        Ok(found.into_iter().flatten().next())
    }
}

/// An object member name, borrowed from the answer unless it holds escapes.
#[derive(serde::Deserialize)]
#[serde(transparent)]
struct Name<'a>(#[serde(borrow)] Cow<'a, str>);

/// Reads token usage of `wire` from its usage `object`.
fn counted(wire: Wire, object: &Value) -> AttemptUsage {
    let counts = match wire {
        Wire::OpenAi => typed::<OpenAiUsage>(object)
            .map(|(usage, json)| (usage.prompt_tokens, Some(usage.completion_tokens), json))
            .or_else(|| {
                typed::<OpenAiResponsesUsage>(object)
                    .map(|(usage, json)| (usage.input_tokens, Some(usage.output_tokens), json))
            })
            .or_else(|| {
                typed::<OpenAiEmbeddingUsage>(object)
                    .map(|(usage, json)| (usage.prompt_tokens, None, json))
            }),
        Wire::Anthropic => typed::<AnthropicUsage>(object).and_then(|(usage, json)| {
            let input = usage
                .input_tokens
                .checked_add(usage.cache_creation_input_tokens)?
                .checked_add(usage.cache_read_input_tokens)?;
            Some((input, Some(usage.output_tokens), json))
        }),
        Wire::Google => typed::<GoogleUsageMetadata>(object).and_then(|(usage, json)| {
            let output = usage
                .candidates_token_count
                .checked_add(usage.thoughts_token_count)?;
            Some((usage.prompt_token_count, Some(output), json))
        }),
        Wire::Jev => typed::<JevUsage>(object)
            .map(|(usage, json)| (usage.input_tokens, Some(usage.output_tokens), json)),
    };
    let Some((input, output, json)) = counts else {
        return AttemptUsage::default();
    };
    let normalized = [("input_tokens", Some(input)), ("output_tokens", output)]
        .into_iter()
        .filter_map(|(dimension, quantity)| Some((dimension, quantity?)))
        .map(|(dimension, quantity)| {
            Some(GatewayUsageAmount {
                dimension: dimension.to_owned(),
                unit: "tokens".to_owned(),
                quantity: wyrd_spec::gateway::GatewayDecimal::new(&quantity.to_string()).ok()?,
            })
        })
        .collect::<Option<Vec<_>>>();
    let Some(normalized) = normalized else {
        return AttemptUsage::default();
    };
    AttemptUsage {
        provider_usage_json: Some(json),
        normalized: Some(normalized),
    }
}

/// Decodes `object` as the typed usage `T` and serializes that projection as
/// canonical JSON, or `None` when either step fails.
fn typed<T: DeserializeOwned + Serialize>(object: &Value) -> Option<(T, String)> {
    let usage = T::deserialize(object).ok()?;
    let json = serde_jcs::to_string(&usage).ok()?;
    Some((usage, json))
}

#[cfg(test)]
mod tests;
