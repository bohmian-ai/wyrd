use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;
use serde_json::value::RawValue;

use crate::wire::anthropic_messages::AnthropicMessage;
use crate::wire::google_generate::GoogleContent;
use crate::wire::openai_chat::OpenAiChatMessage;

/// One native message in a provider's own shape.
#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
#[non_exhaustive]
pub enum MessageNum {
    /// OpenAI chat message.
    OpenAi(Box<OpenAiChatMessage>),
    /// Anthropic message.
    Anthropic(AnthropicMessage),
    /// Google Gemini content turn.
    Gemini(GoogleContent),
    /// Raw provider message body that no typed variant claimed.
    RawV1(Box<RawValue>),
}

impl<'de> Deserialize<'de> for MessageNum {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = Value::deserialize(deserializer)?;

        // MessageNum is the workflow handoff unit. Keep provider-native
        // messages typed when their shape is known; otherwise retain the exact
        // JSON so the caller can reject or handle the unknown shape later.
        if let Ok(message) = serde_json::from_value::<AnthropicMessage>(value.clone()) {
            return Ok(Self::Anthropic(message));
        }
        if let Ok(message) = serde_json::from_value::<GoogleContent>(value.clone()) {
            return Ok(Self::Gemini(message));
        }
        if let Ok(message) = serde_json::from_value::<OpenAiChatMessage>(value.clone()) {
            return Ok(Self::OpenAi(Box::new(message)));
        }

        RawValue::from_string(value.to_string())
            .map(Self::RawV1)
            .map_err(serde::de::Error::custom)
    }
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
