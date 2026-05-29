use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;
use serde_json::value::RawValue;

use crate::wire::anthropic_messages::AnthropicMessagesResponse;
use crate::wire::google_generate::GoogleGenerateContentResponse;
use crate::wire::openai_chat::OpenAiChatResponse;
use crate::wire::openai_responses::OpenAiResponsesResponse;

#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
#[non_exhaustive]
pub enum ProviderResponse {
    OpenAiChatCompletion(OpenAiChatResponse),
    OpenAiResponses(OpenAiResponsesResponse),
    AnthropicMessage(AnthropicMessagesResponse),
    GeminiGenerateContent(GoogleGenerateContentResponse),
    RawV1(Box<RawValue>),
}

impl<'de> Deserialize<'de> for ProviderResponse {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = Value::deserialize(deserializer)?;

        if let Ok(response) = serde_json::from_value::<OpenAiChatResponse>(value.clone()) {
            return Ok(Self::OpenAiChatCompletion(response));
        }
        if let Ok(response) = serde_json::from_value::<OpenAiResponsesResponse>(value.clone()) {
            return Ok(Self::OpenAiResponses(response));
        }
        if let Ok(response) = serde_json::from_value::<AnthropicMessagesResponse>(value.clone()) {
            return Ok(Self::AnthropicMessage(response));
        }
        if let Ok(response) = serde_json::from_value::<GoogleGenerateContentResponse>(value.clone())
        {
            return Ok(Self::GeminiGenerateContent(response));
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
            (Self::AnthropicMessage(left), Self::AnthropicMessage(right)) => left == right,
            (Self::GeminiGenerateContent(left), Self::GeminiGenerateContent(right)) => {
                left == right
            }
            (Self::RawV1(left), Self::RawV1(right)) => left.get() == right.get(),
            _ => false,
        }
    }
}

impl ProviderResponse {
    pub const fn adapter(&self) -> crate::adapter::ResponseAdapter<'_> {
        crate::adapter::ResponseAdapter::new(self)
    }
}
