use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;

use crate::wire::anthropic_messages::AnthropicMessage;
use crate::wire::google_generate::GoogleContent;
use crate::wire::openai_chat::OpenAiChatMessage;

/// One native message in a provider's own shape.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
#[non_exhaustive]
pub enum MessageNum {
    OpenAi(OpenAiChatMessage),
    Anthropic(AnthropicMessage),
    Gemini(GoogleContent),
    RawV1(Box<RawValue>),
}

impl PartialEq for MessageNum {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::OpenAi(left), Self::OpenAi(right)) => left == right,
            (Self::Anthropic(left), Self::Anthropic(right)) => left == right,
            (Self::Gemini(left), Self::Gemini(right)) => left == right,
            (Self::RawV1(left), Self::RawV1(right)) => left.get() == right.get(),
            _ => false,
        }
    }
}
