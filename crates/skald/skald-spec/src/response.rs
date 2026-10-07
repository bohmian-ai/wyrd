use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;
use serde_json::value::RawValue;

use crate::request::ProviderName;

use crate::wire::anthropic_messages::AnthropicMessagesResponse;
use crate::wire::google_embeddings::GoogleBatchEmbedResponse;
use crate::wire::google_generate::GoogleGenerateContentResponse;
use crate::wire::openai_chat::{
    OpenAiChatChoice, OpenAiChatMessage, OpenAiChatResponse, OpenAiMessageContent, OpenAiUsage,
};
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
    /// Google GenerateContent response, from Gemini or Vertex.
    GeminiGenerateContent(GoogleGenerateContentResponse),
    /// Google Gemini BatchEmbedContents response.
    GoogleBatchEmbed(GoogleBatchEmbedResponse),
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
            (Self::VertexPredict(left), Self::VertexPredict(right)) => left == right,
            (Self::RawV1(left), Self::RawV1(right)) => left.get() == right.get(),
            _ => false,
        }
    }
}

impl ProviderResponse {
    /// Build a finished OpenAI Chat Completions response whose one choice is
    /// an assistant message carrying `text`.
    ///
    /// This is the offline shape the `mock` provider returns and the value a
    /// callback or test hands back to replace a model response. The fixed
    /// fields are id `mock_response`, model `mock-model`, finish reason
    /// `stop`, and zero token usage, so the response is deterministic.
    #[must_use]
    pub fn text(text: impl Into<String>) -> Self {
        Self::OpenAiChatCompletion(OpenAiChatResponse {
            id: "mock_response".to_owned(),
            object: "chat.completion".to_owned(),
            created: 0,
            model: "mock-model".to_owned(),
            choices: vec![OpenAiChatChoice {
                index: 0,
                message: OpenAiChatMessage {
                    role: "assistant".to_owned(),
                    content: Some(OpenAiMessageContent::Text(text.into())),
                    ..Default::default()
                },
                finish_reason: Some("stop".to_owned()),
                logprobs: None,
            }],
            usage: Some(OpenAiUsage {
                prompt_tokens: 0,
                completion_tokens: 0,
                total_tokens: 0,
                prompt_tokens_details: None,
                completion_tokens_details: None,
            }),
            system_fingerprint: None,
            service_tier: None,
        })
    }

    /// Borrow response text, tool calls, usage, structured output, and finish reason.
    pub const fn adapter(&self) -> crate::adapter::ResponseAdapter<'_> {
        crate::adapter::ResponseAdapter::new(self)
    }

    /// Returns the default provider of this response's schema dialect.
    ///
    /// This is not the destination that produced the response. GenerateContent
    /// is one schema shared by Gemini and Vertex, so a shared GenerateContent
    /// response reports Google even when Vertex produced it; callers that need
    /// the destination read it from the dispatched Prompt.
    pub fn provider(&self) -> ProviderName {
        match self {
            Self::OpenAiChatCompletion(_)
            | Self::OpenAiResponses(_)
            | Self::OpenAiEmbeddings(_) => ProviderName::OpenAi,
            Self::AnthropicMessage(_) => ProviderName::Anthropic,
            Self::GeminiGenerateContent(_) | Self::GoogleBatchEmbed(_) => ProviderName::Google,
            Self::VertexPredict(_) => ProviderName::Vertex,
            Self::RawV1(_) => ProviderName::Custom("raw".to_owned()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::ProviderResponse;
    use crate::request::ProviderName;

    /// `ProviderResponse::text` reads back as one OpenAI assistant text answer.
    #[test]
    fn text_builds_an_openai_assistant_answer() {
        let response = ProviderResponse::text("synthetic");

        assert_eq!(response.provider(), ProviderName::OpenAi);
        assert_eq!(response.adapter().text().as_deref(), Some("synthetic"));
    }
}
