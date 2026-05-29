use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;
use serde_json::value::RawValue;

use crate::wire::anthropic_messages::AnthropicMessagesRequest;
use crate::wire::google_generate::GoogleGenerateContentRequest;
use crate::wire::openai_chat::OpenAiChatRequest;
use crate::wire::openai_responses::OpenAiResponsesRequest;
use crate::wire::vertex_generate::VertexGenerateContentRequest;

/// One native LLM request.
#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
#[non_exhaustive]
pub enum ProviderRequest {
    /// OpenAI Chat Completions request.
    OpenAiChatCompletion(OpenAiChatRequest),
    /// OpenAI Responses API request.
    OpenAiResponses(OpenAiResponsesRequest),
    /// Anthropic Messages request.
    AnthropicMessage(AnthropicMessagesRequest),
    /// Google Gemini GenerateContent request.
    GeminiGenerateContent(GoogleGenerateContentRequest),
    /// Vertex GenerateContent request.
    Vertex(VertexGenerateContentRequest),
    /// Raw provider request body that no typed variant claimed.
    RawV1 {
        /// Provider dispatch target for the raw body.
        provider: ProviderName,
        /// Unmodified raw provider request JSON.
        body: Box<RawValue>,
    },
}

impl<'de> Deserialize<'de> for ProviderRequest {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = Value::deserialize(deserializer)?;

        // Keep the S01 untagged ordering explicit while still giving RawV1 a
        // dependable fallback. Deriving `Deserialize` directly would ask
        // `Box<RawValue>` to deserialize from any JSON value and can make
        // fallback behavior hard to reason about as typed variants evolve.
        if let Ok(request) = serde_json::from_value::<OpenAiChatRequest>(value.clone()) {
            return Ok(Self::OpenAiChatCompletion(request));
        }
        if let Ok(request) = serde_json::from_value::<OpenAiResponsesRequest>(value.clone()) {
            return Ok(Self::OpenAiResponses(request));
        }
        if let Ok(request) = serde_json::from_value::<AnthropicMessagesRequest>(value.clone()) {
            return Ok(Self::AnthropicMessage(request));
        }
        if let Ok(request) = serde_json::from_value::<GoogleGenerateContentRequest>(value.clone()) {
            return Ok(Self::GeminiGenerateContent(request));
        }
        // Vertex generate is transparent over the Google request shape. This
        // branch is retained for direct deserialization compatibility, but a
        // bare body that also matches Google is claimed by Google first.
        if let Ok(request) = serde_json::from_value::<VertexGenerateContentRequest>(value.clone()) {
            return Ok(Self::Vertex(request));
        }
        if let Ok(raw) = serde_json::from_value::<RawProviderRequest>(value) {
            return Ok(Self::RawV1 {
                provider: raw.provider,
                body: raw.body,
            });
        }

        Err(serde::de::Error::custom(
            "data did not match any ProviderRequest variant",
        ))
    }
}

#[derive(Deserialize)]
struct RawProviderRequest {
    provider: ProviderName,
    body: Box<RawValue>,
}

impl PartialEq for ProviderRequest {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::OpenAiChatCompletion(left), Self::OpenAiChatCompletion(right)) => left == right,
            (Self::OpenAiResponses(left), Self::OpenAiResponses(right)) => left == right,
            (Self::AnthropicMessage(left), Self::AnthropicMessage(right)) => left == right,
            (Self::GeminiGenerateContent(left), Self::GeminiGenerateContent(right)) => {
                left == right
            }
            (Self::Vertex(left), Self::Vertex(right)) => left == right,
            (
                Self::RawV1 {
                    provider: left_provider,
                    body: left_body,
                },
                Self::RawV1 {
                    provider: right_provider,
                    body: right_body,
                },
            ) => left_provider == right_provider && left_body.get() == right_body.get(),
            _ => false,
        }
    }
}

/// Which provider a request targets.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum ProviderName {
    /// OpenAI provider.
    OpenAi,
    /// Anthropic provider.
    Anthropic,
    /// Google AI Studio provider.
    Google,
    /// Google Vertex AI provider.
    Vertex,
    /// Provider not modeled by skald-spec.
    Custom(String),
}

impl ProviderRequest {
    /// Returns the durable provider dispatch target for every request variant.
    pub fn provider(&self) -> ProviderName {
        match self {
            Self::OpenAiChatCompletion(_) | Self::OpenAiResponses(_) => ProviderName::OpenAi,
            Self::AnthropicMessage(_) => ProviderName::Anthropic,
            Self::GeminiGenerateContent(_) => ProviderName::Google,
            Self::Vertex(_) => ProviderName::Vertex,
            Self::RawV1 { provider, .. } => provider.clone(),
        }
    }
}
