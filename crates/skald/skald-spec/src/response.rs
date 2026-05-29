use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;

use crate::wire::anthropic_messages::AnthropicMessagesResponse;
use crate::wire::google_generate::GoogleGenerateContentResponse;
use crate::wire::openai_chat::OpenAiChatResponse;
use crate::wire::openai_responses::OpenAiResponsesResponse;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
#[non_exhaustive]
pub enum ProviderResponse {
    OpenAiChatCompletion(OpenAiChatResponse),
    OpenAiResponses(OpenAiResponsesResponse),
    AnthropicMessage(AnthropicMessagesResponse),
    GeminiGenerateContent(GoogleGenerateContentResponse),
    RawV1(Box<RawValue>),
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
