//! Public contracts of the provider-native gateway ingress.
//!
//! Anthropic Messages and Gemini `GenerateContent` ingress forward each
//! provider's own request and return its response and stream bytes unchanged.
//! The request and buffered response contracts here name the members the
//! gateway reads or a client relies on and keep every other member in a
//! flattened `extra` map, so a decode accepts any extension the official
//! clients send and the published schema stays open. The ingress validates a
//! body against its request contract but forwards the original JSON. The
//! envelopes of gateway-originated refusals keep the provider's field names,
//! so an unmodified provider SDK classifies the refusal, and add the stable
//! Wyrd error code where the provider shape has room for it.
//!
//! With the `server` feature the request and response contracts implement
//! [`utoipa::ToSchema`] from their generated JSON Schema, as the
//! `OpenAI`-compatible contracts do.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// `POST /v1/messages` request: an Anthropic Messages body whose `model` is an
/// exact Anthropic model id.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct GatewayAnthropicMessagesRequest {
    /// Exact Anthropic model id; only built-in Anthropic deployments serve it.
    pub model: String,
    /// Output-token maximum, which bounds the call's cost.
    pub max_tokens: u64,
    /// Conversation turns, oldest first.
    pub messages: Vec<GatewayAnthropicMessageParam>,
    /// `true` answers with Anthropic server-sent events.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
    /// Every other Anthropic member, such as `system`, `tools`, or
    /// `metadata`, forwarded unchanged.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// One turn of a [`GatewayAnthropicMessagesRequest`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct GatewayAnthropicMessageParam {
    /// `user` or `assistant`.
    pub role: String,
    /// Text, or an array of Anthropic content blocks.
    pub content: Value,
    /// Other Anthropic turn members, forwarded unchanged.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// Buffered `POST /v1/messages` answer: the provider's Anthropic message.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct GatewayAnthropicMessage {
    /// Provider message id.
    pub id: String,
    /// Always `message`.
    #[serde(rename = "type")]
    pub kind: String,
    /// Always `assistant`.
    pub role: String,
    /// Model that answered.
    pub model: String,
    /// Anthropic content blocks.
    pub content: Vec<Value>,
    /// Why generation stopped, such as `end_turn`.
    pub stop_reason: Option<String>,
    /// Stop sequence that ended generation, if any.
    pub stop_sequence: Option<String>,
    /// Token usage the gateway accounts.
    pub usage: GatewayAnthropicUsage,
    /// Other Anthropic message members, relayed unchanged.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// Token usage of a [`GatewayAnthropicMessage`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct GatewayAnthropicUsage {
    /// Prompt tokens.
    pub input_tokens: u64,
    /// Generated tokens.
    pub output_tokens: u64,
    /// Other usage members, such as cache token counts, relayed unchanged.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// `POST /v1beta/models/{model}:generateContent` and
/// `:streamGenerateContent` request: a Gemini `GenerateContent` body.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct GatewayGeminiGenerateContentRequest {
    /// Conversation turns, oldest first.
    pub contents: Vec<GatewayGeminiContent>,
    /// Every other Gemini member, such as `generationConfig`,
    /// `systemInstruction`, or `tools`, forwarded unchanged.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// One Gemini `Content` turn.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct GatewayGeminiContent {
    /// `user` or `model`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    /// Gemini parts, such as `{"text": "..."}`.
    #[serde(default)]
    pub parts: Vec<Value>,
    /// Other `Content` members, relayed unchanged.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// Buffered `generateContent` answer, and each `streamGenerateContent` event:
/// a Gemini `GenerateContentResponse`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct GatewayGeminiGenerateContentResponse {
    /// Generated candidates; absent when the prompt was blocked.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub candidates: Vec<GatewayGeminiCandidate>,
    /// Token usage the gateway accounts.
    #[serde(
        rename = "usageMetadata",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub usage_metadata: Option<Map<String, Value>>,
    /// Other response members, such as `modelVersion`, relayed unchanged.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// One candidate of a [`GatewayGeminiGenerateContentResponse`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct GatewayGeminiCandidate {
    /// Generated turn.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<GatewayGeminiContent>,
    /// Why generation stopped, such as `STOP`; absent until it finishes.
    #[serde(
        rename = "finishReason",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub finish_reason: Option<String>,
    /// Other candidate members, relayed unchanged.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

#[cfg(feature = "server")]
super::openai::openapi_schema!(
    GatewayAnthropicMessagesRequest,
    GatewayAnthropicMessage,
    GatewayGeminiGenerateContentRequest,
    GatewayGeminiGenerateContentResponse,
);

/// Anthropic error envelope of a gateway-originated refusal on
/// `POST /v1/messages`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct AnthropicErrorEnvelope {
    /// Always `error`.
    #[serde(rename = "type")]
    pub kind: String,
    /// The error.
    pub error: AnthropicError,
}

/// Body of an [`AnthropicErrorEnvelope`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct AnthropicError {
    /// Anthropic error type, such as `invalid_request_error`.
    #[serde(rename = "type")]
    pub kind: String,
    /// Human-readable message.
    pub message: String,
    /// Stable Wyrd error code.
    pub code: Option<String>,
}

/// Google error envelope of a gateway-originated refusal on the Gemini
/// `GenerateContent` routes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct GoogleErrorEnvelope {
    /// The error.
    pub error: GoogleError,
}

/// Body of a [`GoogleErrorEnvelope`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct GoogleError {
    /// HTTP status code.
    pub code: u16,
    /// Human-readable message.
    pub message: String,
    /// Canonical Google RPC status name, such as `INVALID_ARGUMENT`.
    pub status: String,
    /// Structured details; gateway refusals carry one `ErrorInfo` whose
    /// `reason` is the stable Wyrd error code.
    pub details: Vec<GoogleErrorInfo>,
}

/// `google.rpc.ErrorInfo` detail naming the stable Wyrd error code.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct GoogleErrorInfo {
    /// Always `type.googleapis.com/google.rpc.ErrorInfo`.
    #[serde(rename = "@type")]
    pub kind: String,
    /// Stable Wyrd error code.
    pub reason: String,
    /// Always `wyrd`.
    pub domain: String,
}
