use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;
use serde_json::value::RawValue;

use crate::request::ProviderName;

use crate::wire::anthropic_messages::AnthropicMessagesResponse;
use crate::wire::google_embeddings::GoogleBatchEmbedResponse;
use crate::wire::google_generate::GoogleGenerateContentResponse;
use crate::wire::openai_chat::OpenAiChatResponse;
use crate::wire::openai_embeddings::OpenAiEmbeddingsResponse;
use crate::wire::openai_responses::OpenAiResponsesResponse;
use crate::wire::vertex_predict::VertexPredictResponse;

/// One native LLM provider response.
#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
#[non_exhaustive]
pub enum ProviderResponse {
    /// OpenAI Chat Completions response.
    OpenAiChatCompletion(OpenAiChatResponse),
    /// OpenAI Responses API response.
    OpenAiResponses(OpenAiResponsesResponse),
    /// OpenAI Embeddings response.
    OpenAiEmbeddings(OpenAiEmbeddingsResponse),
    /// Anthropic Messages response.
    AnthropicMessage(AnthropicMessagesResponse),
    /// Google Gemini GenerateContent response.
    GeminiGenerateContent(GoogleGenerateContentResponse),
    /// Google Gemini BatchEmbedContents response.
    GoogleBatchEmbed(GoogleBatchEmbedResponse),
    /// Vertex GenerateContent response.
    VertexGenerateContent(GoogleGenerateContentResponse),
    /// Vertex Predict response.
    VertexPredict(VertexPredictResponse),
    /// Raw provider response body that no typed variant claimed.
    RawV1(Box<RawValue>),
}

impl<'de> Deserialize<'de> for ProviderResponse {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = Value::deserialize(deserializer)?;

        // Responses do not carry an outer provider discriminator. Try the
        // strict typed envelopes first, then preserve any unclaimed JSON as
        // RawV1 instead of accepting it as an overly-permissive provider shape.
        if let Ok(response) = serde_json::from_value::<OpenAiChatResponse>(value.clone()) {
            return Ok(Self::OpenAiChatCompletion(response));
        }
        if let Ok(response) = serde_json::from_value::<OpenAiResponsesResponse>(value.clone()) {
            return Ok(Self::OpenAiResponses(response));
        }
        if let Ok(response) = serde_json::from_value::<OpenAiEmbeddingsResponse>(value.clone()) {
            return Ok(Self::OpenAiEmbeddings(response));
        }
        if let Ok(response) = serde_json::from_value::<AnthropicMessagesResponse>(value.clone()) {
            return Ok(Self::AnthropicMessage(response));
        }
        if let Ok(response) = serde_json::from_value::<GoogleGenerateContentResponse>(value.clone())
        {
            return Ok(Self::GeminiGenerateContent(response));
        }
        if let Ok(response) = serde_json::from_value::<GoogleBatchEmbedResponse>(value.clone()) {
            return Ok(Self::GoogleBatchEmbed(response));
        }
        if let Ok(response) = serde_json::from_value::<VertexPredictResponse>(value.clone()) {
            return Ok(Self::VertexPredict(response));
        }

        RawValue::from_string(value.to_string())
            .map(Self::RawV1)
            .map_err(serde::de::Error::custom)
    }
}

impl PartialEq for ProviderResponse {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::OpenAiChatCompletion(left), Self::OpenAiChatCompletion(right)) => left == right,
            (Self::OpenAiResponses(left), Self::OpenAiResponses(right)) => left == right,
            (Self::OpenAiEmbeddings(left), Self::OpenAiEmbeddings(right)) => left == right,
            (Self::AnthropicMessage(left), Self::AnthropicMessage(right)) => left == right,
            (Self::GeminiGenerateContent(left), Self::GeminiGenerateContent(right)) => {
                left == right
            }
            (Self::GoogleBatchEmbed(left), Self::GoogleBatchEmbed(right)) => left == right,
            (Self::VertexGenerateContent(left), Self::VertexGenerateContent(right)) => {
                left == right
            }
            (Self::VertexPredict(left), Self::VertexPredict(right)) => left == right,
            (Self::RawV1(left), Self::RawV1(right)) => left.get() == right.get(),
            _ => false,
        }
    }
}

impl ProviderResponse {
    /// Borrow response text, tool calls, usage, structured output, and finish reason.
    pub const fn adapter(&self) -> crate::adapter::ResponseAdapter<'_> {
        crate::adapter::ResponseAdapter::new(self)
    }

    /// Returns the provider that produced this response.
    pub fn provider(&self) -> ProviderName {
        match self {
            Self::OpenAiChatCompletion(_)
            | Self::OpenAiResponses(_)
            | Self::OpenAiEmbeddings(_) => ProviderName::OpenAi,
            Self::AnthropicMessage(_) => ProviderName::Anthropic,
            Self::GeminiGenerateContent(_) | Self::GoogleBatchEmbed(_) => ProviderName::Google,
            Self::VertexGenerateContent(_) | Self::VertexPredict(_) => ProviderName::Vertex,
            Self::RawV1(_) => ProviderName::Custom("raw".to_owned()),
        }
    }
}
