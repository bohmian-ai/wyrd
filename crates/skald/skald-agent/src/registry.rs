//! Helpers that bridge serde forms in data contracts to native message envelopes.

use skald_spec::wire::anthropic_messages::{AnthropicContentBlock, AnthropicMessage};
use skald_spec::wire::google_generate::{GoogleContent, GooglePart};
use skald_spec::wire::openai_chat::{OpenAiChatMessage, OpenAiMessageContent};
use skald_spec::{MessageNum, ProviderName};

use crate::error::{AgentError, AgentResult};

/// Wraps a system prompt string into native messages for the given provider.
pub fn system_messages(prompt: &str, provider: &ProviderName) -> AgentResult<Vec<MessageNum>> {
    if prompt.trim().is_empty() {
        return Ok(Vec::new());
    }

    match provider {
        ProviderName::OpenAi => Ok(vec![MessageNum::OpenAi(Box::new(OpenAiChatMessage {
            role: "system".to_owned(),
            content: Some(OpenAiMessageContent::Text(prompt.to_owned())),
            ..Default::default()
        }))]),
        ProviderName::Anthropic => Ok(vec![MessageNum::Anthropic(AnthropicMessage {
            role: "system".to_owned(),
            content: vec![AnthropicContentBlock::Text {
                text: prompt.to_owned(),
                cache_control: None,
                citations: None,
            }],
        })]),
        ProviderName::Google | ProviderName::Vertex => {
            Ok(vec![MessageNum::Gemini(GoogleContent {
                role: "system".to_owned(),
                parts: vec![GooglePart::Text {
                    text: prompt.to_owned(),
                }],
            })])
        }
        ProviderName::Custom(provider) => Err(AgentError::Prompt {
            agent: provider.clone(),
            detail: "custom providers must author the system content into the prompt directly"
                .to_owned(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use skald_spec::ProviderName;

    use super::system_messages;

    #[test]
    fn system_messages_custom_provider_errors() {
        let provider = ProviderName::Custom("llama".to_owned());

        let err = system_messages("be helpful", &provider).expect_err("custom providers must fail");

        assert_eq!(err.code(), "SKALD_AGENT_422_PROMPT");
    }
}
