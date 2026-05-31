//! Helpers that bridge serde forms in [`crate::AgentDef`] to native values.

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
        ProviderName::OpenAi => Ok(vec![MessageNum::OpenAi(OpenAiChatMessage {
            role: "system".to_owned(),
            content: Some(OpenAiMessageContent::Text(prompt.to_owned())),
            name: None,
            tool_calls: None,
            tool_call_id: None,
            refusal: None,
        })]),
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
        ProviderName::Custom(_) => Err(AgentError::SystemPrompt {
            provider: provider.clone(),
            detail: "custom providers must author the system content into the Prompt directly"
                .to_owned(),
        }),
    }
}
