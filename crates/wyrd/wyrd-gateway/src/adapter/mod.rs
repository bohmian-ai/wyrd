//! Provider protocol adapters for one attempt.
//!
//! [`prepare`] turns a caller body into the request a deployment's adapter
//! speaks. A body already in the deployment's dialect passes through with only
//! its model rewritten to the native identifier; `OpenAI` Chat Completions
//! ingress translates to Anthropic Messages or Google `GenerateContent` through
//! typed Skald wire structs. Anything the target cannot represent faithfully
//! is rejected before dispatch rather than dropped. [`complete`] maps the
//! provider's answer back to the ingress dialect and extracts usage; streamed
//! answers relay through [`stream::relay`].

mod anthropic;
mod batch;
mod google;
mod http;
mod media;
mod stream;

use std::time::{SystemTime, UNIX_EPOCH};

use base64::Engine as _;
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::value::{RawValue, to_raw_value};
use serde_json::{Map, Value, json};
use skald_spec::ProviderResponse;
use skald_spec::wire::anthropic_messages::{AnthropicMessagesRequest, AnthropicUsage};
use skald_spec::wire::google_generate::{GoogleGenerateContentRequest, GoogleUsageMetadata};
use skald_spec::wire::openai_chat::{
    OpenAiChatMessage, OpenAiChatRequest, OpenAiChatResponse, OpenAiImageUrl, OpenAiMessageContent,
    OpenAiStop, OpenAiStreamOptions, OpenAiToolCall, OpenAiUsage,
};
use skald_spec::wire::openai_embeddings::{
    OpenAiEmbeddingUsage, OpenAiEmbeddingsInput, OpenAiEmbeddingsRequest,
};
use skald_spec::wire::openai_responses::OpenAiResponsesUsage;
use wyrd_spec::gateway::native::{
    AnthropicError, AnthropicErrorEnvelope, GoogleError, GoogleErrorEnvelope, GoogleErrorInfo,
};
use wyrd_spec::gateway::openai::{OpenAiError, OpenAiErrorEnvelope};
use wyrd_spec::gateway::{
    GatewayOperation, GatewayUsageAmount, ProviderAdapter, ProviderDeployment,
};

use crate::credential::ProviderSecret;
use crate::engine::AttemptUsage;

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
}

impl Wire {
    /// Protocol family `adapter` speaks.
    pub(crate) const fn of(adapter: &ProviderAdapter) -> Self {
        match adapter {
            ProviderAdapter::OpenAi | ProviderAdapter::OpenAiCompatible { .. } => Self::OpenAi,
            ProviderAdapter::Anthropic => Self::Anthropic,
            ProviderAdapter::Gemini | ProviderAdapter::Vertex { .. } => Self::Google,
        }
    }
}

/// Request ready for one deployment.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Prepared {
    /// A body already in the deployment's dialect, sent unchanged except for
    /// its model; the answer returns unchanged.
    Native(Value),
    /// Chat Completions translated to Anthropic Messages; the answer
    /// translates back. Boxed so the variant does not inflate every `Native`.
    Anthropic(Box<AnthropicMessagesRequest>),
    /// Chat Completions translated to Google `GenerateContent`; the answer
    /// translates back. Boxed so the variant does not inflate every `Native`.
    Google(Box<GoogleGenerateContentRequest>),
}

/// Builds the upstream request of `body` for `deployment`.
///
/// Pure and deterministic, so planning can use it to drop deployments that
/// cannot represent a request before admission and dispatch repeats it.
///
/// # Errors
///
/// Returns [`UnsupportedRequest`] for an operation the adapter does not carry,
/// a native body sent to a different dialect, streaming of an operation that
/// does not stream, a `stream` member that disagrees with `stream`, a `media`
/// route or file set [`media::validate`] rejects, or any translated feature
/// without a faithful target representation. Images and Audio pass through
/// only to `OpenAI`-protocol adapters, as do Batches lifecycle bodies.
pub(crate) fn prepare(
    ingress: IngressDialect,
    operation: GatewayOperation,
    stream: bool,
    body: &Value,
    deployment: &ProviderDeployment,
    media: Option<&MediaRequest>,
) -> Result<Prepared, UnsupportedRequest> {
    let model = deployment.model.model.as_str();
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
    ) && body.get("stream").and_then(Value::as_bool).unwrap_or(false) != stream
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
            | GatewayOperation::Audio => Ok(Prepared::Native(native(body, Some(model))?)),
            GatewayOperation::Embeddings => {
                embeddings(body)?;
                Ok(Prepared::Native(native(body, Some(model))?))
            }
            // A batch body carries no model; its input file names the model.
            GatewayOperation::Batches => Ok(Prepared::Native(native(body, None)?)),
        },
        (IngressDialect::OpenAi, ProviderAdapter::Anthropic) if chat => Ok(Prepared::Anthropic(
            Box::new(anthropic::request(chat_request(body)?, model)?),
        )),
        (IngressDialect::OpenAi, ProviderAdapter::Gemini | ProviderAdapter::Vertex { .. })
            if chat =>
        {
            Ok(Prepared::Google(Box::new(google::request(chat_request(
                body,
            )?)?)))
        }
        (IngressDialect::AnthropicMessages, ProviderAdapter::Anthropic) if chat => {
            Ok(Prepared::Native(native(body, Some(model))?))
        }
        (IngressDialect::GeminiGenerateContent, ProviderAdapter::Gemini)
        | (IngressDialect::VertexGenerateContent, ProviderAdapter::Vertex { .. })
            if chat =>
        {
            Ok(Prepared::Native(native(body, None)?))
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

/// Maps a successful provider answer to the ingress dialect and extracts its
/// usage.
///
/// A raw answer is a native passthrough whose bytes return unchanged; a parsed
/// copy only yields its usage, read as `wire`. Typed Anthropic and Google
/// answers translate to a chat completion, serialized once and named `id`
/// when the provider supplies no usable identifier.
///
/// # Errors
///
/// Returns the usage that could be read when a translated answer carries
/// content, a finish state, or a shape the ingress dialect cannot represent,
/// or when the answer is of a kind no prepared request produces.
pub(crate) fn complete(
    response: ProviderResponse,
    wire: Wire,
    id: &str,
    model: &str,
) -> Result<(Box<RawValue>, AttemptUsage), AttemptUsage> {
    let translated = |usage: AttemptUsage, chat: Option<OpenAiChatResponse>| {
        chat.and_then(|chat| to_raw_value(&chat).ok())
            .map(|chat| (chat, usage.clone()))
            .ok_or(usage)
    };
    match response {
        ProviderResponse::RawV1(raw) => {
            let body: Value =
                serde_json::from_str(raw.get()).map_err(|_| AttemptUsage::default())?;
            let usage = usage(wire, &body);
            Ok((raw, usage))
        }
        ProviderResponse::AnthropicMessage(native) => {
            let usage = usage(
                Wire::Anthropic,
                &serde_json::to_value(&native).unwrap_or_default(),
            );
            translated(usage, anthropic::response(native, model))
        }
        ProviderResponse::GeminiGenerateContent(native)
        | ProviderResponse::VertexGenerateContent(native) => {
            let usage = usage(
                Wire::Google,
                &serde_json::to_value(&native).unwrap_or_default(),
            );
            translated(usage, google::response(native, id, model))
        }
        _ => Err(AttemptUsage::default()),
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
pub(crate) fn without_secret(value: Value, secret: &str) -> Value {
    match value {
        Value::String(text) => Value::String(text.replace(secret, "")),
        Value::Array(items) => Value::Array(
            items
                .into_iter()
                .map(|item| without_secret(item, secret))
                .collect(),
        ),
        Value::Object(object) => Value::Object(
            object
                .into_iter()
                .map(|(key, item)| (key.replace(secret, ""), without_secret(item, secret)))
                .collect(),
        ),
        other => other,
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
fn adapter_name(adapter: &ProviderAdapter) -> &'static str {
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

/// Copies a native body, rewriting `model` when the dialect carries it in the
/// body.
///
/// # Errors
///
/// Returns [`UnsupportedRequest`] for a non-object body.
fn native(body: &Value, model: Option<&str>) -> Result<Value, UnsupportedRequest> {
    let Value::Object(object) = body else {
        return Err(UnsupportedRequest::new("body", "must be a JSON object"));
    };
    let mut object = object.clone();
    if let Some(model) = model {
        object.insert("model".to_owned(), Value::String(model.to_owned()));
    }
    Ok(Value::Object(object))
}

/// Largest number of inputs one embeddings request may carry: the `OpenAI`
/// per-request maximum, so a compatible provider never receives a batch the
/// protocol does not define.
const MAX_EMBEDDING_INPUTS: usize = 2048;

/// Validates an `OpenAI` embeddings body before it passes through unchanged.
///
/// The body decodes through the typed Skald request, so input is a string, a
/// token array, or a batch of either; a batch keeps its order because the body
/// is forwarded as submitted and the provider answers per index.
///
/// # Errors
///
/// Returns [`UnsupportedRequest`] naming `body` when it is not an embeddings
/// request, `input` when it is empty, carries an empty item, or exceeds
/// [`MAX_EMBEDDING_INPUTS`], `dimensions` when it is zero, and
/// `encoding_format` for a format other than `float` or `base64`.
fn embeddings(body: &Value) -> Result<(), UnsupportedRequest> {
    let request: OpenAiEmbeddingsRequest =
        serde_json::from_value(body.clone()).map_err(|error| {
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
        .as_deref()
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
fn inputs(input: &OpenAiEmbeddingsInput) -> (usize, bool) {
    match input {
        OpenAiEmbeddingsInput::One(text) => (1, text.is_empty()),
        OpenAiEmbeddingsInput::Tokens(tokens) => (1, tokens.is_empty()),
        OpenAiEmbeddingsInput::Many(items) => (items.len(), items.iter().any(String::is_empty)),
        OpenAiEmbeddingsInput::TokenBatches(items) => {
            (items.len(), items.iter().any(Vec::is_empty))
        }
    }
}

/// True when a successful embeddings `answer` honors its `request`.
///
/// The answer must carry one item per input in input order (`data[i].index ==
/// i`). Every vector must use the requested representation: an array of
/// numbers when `encoding_format` is omitted or `float`, a base64 string when
/// it is `base64`. A base64 vector must decode under the standard padded
/// alphabet to a whole number of `f32` values. When `dimensions` was requested
/// every vector must have exactly that many values. A provider silently
/// ignoring any of these would weaken the request, so the attempt fails
/// instead of returning the answer.
pub(crate) fn faithful_embeddings(request: &Value, answer: &RawValue) -> bool {
    let (Ok(request), Ok(answer)) = (
        serde_json::from_value::<OpenAiEmbeddingsRequest>(request.clone()),
        serde_json::from_str::<Value>(answer.get()),
    ) else {
        return false;
    };
    let (count, _) = inputs(&request.input);
    let Some(data) = answer.get("data").and_then(Value::as_array) else {
        return false;
    };
    let base64 = request.encoding_format.as_deref() == Some("base64");
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
    options: Option<&OpenAiStreamOptions>,
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

/// Decodes an `OpenAI` Chat Completions body into its typed Skald form.
///
/// # Errors
///
/// Returns [`UnsupportedRequest`] naming the decode failure.
fn chat_request(body: &Value) -> Result<OpenAiChatRequest, UnsupportedRequest> {
    serde_json::from_value(body.clone()).map_err(|error| {
        UnsupportedRequest::new(
            "body",
            format!("is not a valid chat completion request: {error}"),
        )
    })
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
fn reject_extra(adapter: &str, extra: &Map<String, Value>) -> Result<(), UnsupportedRequest> {
    reject_present(
        adapter,
        &extra
            .keys()
            .map(|key| (key.as_str(), true))
            .collect::<Vec<_>>(),
    )
}

/// Splits the leading system and developer messages from the conversation.
///
/// Returns their joined text, or `None`, and the index of the first
/// conversational message.
///
/// # Errors
///
/// Returns [`UnsupportedRequest`] for a later system message, whose position
/// no target can represent, or non-text system content.
fn system_prefix(
    messages: &[OpenAiChatMessage],
) -> Result<(Option<String>, usize), UnsupportedRequest> {
    let is_system =
        |message: &OpenAiChatMessage| matches!(message.role.as_str(), "system" | "developer");
    let offset = messages.iter().take_while(|m| is_system(m)).count();
    if let Some(index) = messages.iter().skip(offset).position(is_system) {
        return Err(UnsupportedRequest::new(
            format!("messages[{}].role", index + offset),
            "system messages must precede the conversation for this adapter",
        ));
    }
    let mut texts = Vec::with_capacity(offset);
    for (index, message) in messages[..offset].iter().enumerate() {
        let field = format!("messages[{index}]");
        plain_message(message, &field)?;
        texts.push(text_content(message.content.as_ref(), &field)?.unwrap_or_default());
    }
    Ok(((!texts.is_empty()).then(|| texts.join("\n\n")), offset))
}

/// Rejects message members no translated target represents.
///
/// # Errors
///
/// Returns [`UnsupportedRequest`] for `name`, `refusal`, `audio`,
/// annotations, `tool_calls` outside assistant messages, or `tool_call_id`
/// outside tool messages.
fn plain_message(message: &OpenAiChatMessage, field: &str) -> Result<(), UnsupportedRequest> {
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

/// Text of text-only content, or `None` when absent or empty.
///
/// # Errors
///
/// Returns [`UnsupportedRequest`] when a content part is not text.
fn text_content(
    content: Option<&OpenAiMessageContent>,
    field: &str,
) -> Result<Option<String>, UnsupportedRequest> {
    let text = match content {
        None => String::new(),
        Some(OpenAiMessageContent::Text(text)) => text.clone(),
        Some(OpenAiMessageContent::Parts(parts)) => {
            let mut text = String::new();
            for (index, part) in parts.iter().enumerate() {
                match part {
                    skald_spec::wire::openai_chat::OpenAiContentPart::Text { text: part } => {
                        text.push_str(part);
                    }
                    _ => {
                        return Err(UnsupportedRequest::new(
                            format!("{field}.content[{index}]"),
                            "only text content is supported for this message",
                        ));
                    }
                }
            }
            text
        }
    };
    Ok((!text.is_empty()).then_some(text))
}

/// Splits a base64 `data:` image URL into media type and payload.
///
/// # Errors
///
/// Returns [`UnsupportedRequest`] for a non-`auto` detail hint.
fn image(
    image: &OpenAiImageUrl,
    field: &str,
) -> Result<Option<(String, String)>, UnsupportedRequest> {
    if image
        .detail
        .as_deref()
        .is_some_and(|detail| detail != "auto")
    {
        return Err(UnsupportedRequest::new(
            format!("{field}.image_url.detail"),
            "image detail is not supported by a translated adapter",
        ));
    }
    Ok(image
        .url
        .strip_prefix("data:")
        .and_then(|rest| rest.split_once(";base64,"))
        .map(|(media, data)| (media.to_owned(), data.to_owned())))
}

/// Decodes tool-call arguments, which must be a JSON object.
///
/// # Errors
///
/// Returns [`UnsupportedRequest`] for a non-function call or arguments that
/// are not a JSON object.
fn arguments(call: &OpenAiToolCall, field: &str) -> Result<Value, UnsupportedRequest> {
    match serde_json::from_str::<Value>(&call.function.arguments) {
        Ok(value @ Value::Object(_)) if call.kind == "function" => Ok(value),
        _ => Err(UnsupportedRequest::new(
            format!("{field}.tool_calls"),
            "tool calls must be functions whose arguments are a JSON object",
        )),
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
    let object = match wire {
        Wire::OpenAi | Wire::Anthropic => body.get("usage"),
        Wire::Google => body
            .get("usage_metadata")
            .or_else(|| body.get("usageMetadata")),
    };
    let Some(object) = object else {
        return AttemptUsage::default();
    };
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
