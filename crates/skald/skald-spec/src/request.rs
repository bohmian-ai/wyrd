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
    OpenAiChatCompletion(OpenAiChatRequest),
    OpenAiResponses(OpenAiResponsesRequest),
    AnthropicMessage(AnthropicMessagesRequest),
    GeminiGenerateContent(GoogleGenerateContentRequest),
    Vertex(VertexGenerateContentRequest),
    RawV1 {
        provider: ProviderName,
        body: Box<RawValue>,
    },
}

impl<'de> Deserialize<'de> for ProviderRequest {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = Value::deserialize(deserializer)?;

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
    OpenAi,
    Anthropic,
    Google,
    Vertex,
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
