use serde::Deserialize;
use serde_json::Value;

use crate::error::{SkaldError, SkaldResult};
use crate::prompt::{Prompt, ResponseType};
use crate::request::{ProviderName, ProviderRequest};
use crate::wire::anthropic_messages::{
    AnthropicMessage, AnthropicMessagesRequest, AnthropicMessagesSettings, AnthropicSystem,
};
use crate::wire::google_generate::{
    GoogleContent, GoogleGenerateContentRequest, GoogleGenerateSettings, GooglePart,
};
use crate::wire::openai_chat::{
    OpenAiChatMessage, OpenAiChatRequest, OpenAiChatSettings, OpenAiMessageContent,
};
use crate::wire::openai_responses::{
    OpenAiResponseContentPart, OpenAiResponseItem, OpenAiResponsesRequest, OpenAiResponsesSettings,
};
use crate::wire::vertex_generate::VertexGenerateContentRequest;

/// Declarative prompt draft that compiles into a native `Prompt`.
///
/// Both the YAML authoring path and the py-wyrd `Prompt(provider=…)` constructor
/// compile through here, ensuring one canonical assembly per provider.
#[derive(Debug, Clone, Deserialize)]
pub struct PromptDraft {
    /// Provider name (e.g. `"openai"`, `"anthropic"`, `"google"`, `"vertex"`).
    pub provider: String,
    /// Model identifier.
    pub model: String,
    /// Optional operation override. Set to `"responses"` for the OpenAI Responses API.
    #[serde(default)]
    pub operation: Option<String>,
    /// Optional system prompt / instructions.
    #[serde(default)]
    pub system: Option<String>,
    /// User messages — a single string or a list of strings.
    #[serde(default)]
    pub messages: DraftMessages,
    /// Provider-native generation settings as a JSON value.
    #[serde(default)]
    pub model_settings: Option<Value>,
    /// Optional author-assigned version.
    #[serde(default)]
    pub version: Option<String>,
}

/// User messages in a `PromptDraft` — a single string or a list of strings.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(untagged)]
pub enum DraftMessages {
    /// No messages.
    #[default]
    None,
    /// Single user message string.
    One(String),
    /// Multiple user message strings.
    Many(Vec<String>),
}

impl DraftMessages {
    fn into_vec(self) -> Vec<String> {
        match self {
            Self::None => Vec::new(),
            Self::One(s) => vec![s],
            Self::Many(v) => v,
        }
    }
}

impl PromptDraft {
    /// Compile this declarative draft into a native `Prompt`.
    ///
    /// `provider` is the discriminator — not untagged guessing.
    pub fn compile(self) -> SkaldResult<Prompt> {
        let provider = provider_from_str(&self.provider);
        let model = self.model.clone();
        let messages = self.messages.into_vec();
        let is_responses = self
            .operation
            .as_deref()
            .is_some_and(|op| op.eq_ignore_ascii_case("responses"));

        let request = match provider {
            ProviderName::OpenAi if is_responses => {
                build_openai_responses(model.clone(), self.system, messages, self.model_settings)?
            }
            ProviderName::OpenAi => {
                build_openai_chat(model.clone(), self.system, messages, self.model_settings)?
            }
            ProviderName::Anthropic => {
                build_anthropic(model.clone(), self.system, messages, self.model_settings)?
            }
            ProviderName::Google => {
                build_gemini(self.system, messages, self.model_settings, false)?
            }
            ProviderName::Vertex => build_gemini(self.system, messages, self.model_settings, true)?,
            ProviderName::Custom(name) => {
                return Err(SkaldError::PromptDraftInvalid {
                    provider: name,
                    message: "unknown provider for declarative compile".to_owned(),
                });
            }
        };

        Prompt::new(request, model, self.version, ResponseType::Text)
    }
}

fn provider_from_str(value: &str) -> ProviderName {
    match value.trim().to_ascii_lowercase().as_str() {
        "openai" | "open_ai" => ProviderName::OpenAi,
        "anthropic" => ProviderName::Anthropic,
        "google" | "gemini" => ProviderName::Google,
        "vertex" | "vertex_ai" => ProviderName::Vertex,
        _ => ProviderName::Custom(value.to_owned()),
    }
}

fn decode_settings<T: serde::de::DeserializeOwned + Default>(
    provider: ProviderName,
    value: Option<Value>,
) -> SkaldResult<T> {
    let Some(v) = value else {
        return Ok(T::default());
    };
    serde_json::from_value(v).map_err(|e| SkaldError::SettingsDecode {
        provider,
        message: e.to_string(),
    })
}

fn build_openai_chat(
    model: String,
    system: Option<String>,
    messages: Vec<String>,
    settings_value: Option<Value>,
) -> SkaldResult<ProviderRequest> {
    let settings: OpenAiChatSettings = decode_settings(ProviderName::OpenAi, settings_value)?;
    let mut chat_messages: Vec<OpenAiChatMessage> = Vec::new();
    if let Some(system_text) = system {
        chat_messages.push(OpenAiChatMessage {
            role: "system".to_owned(),
            content: Some(OpenAiMessageContent::Text(system_text)),
            name: None,
            tool_calls: None,
            tool_call_id: None,
            refusal: None,
        });
    }
    for text in messages {
        chat_messages.push(OpenAiChatMessage {
            role: "user".to_owned(),
            content: Some(OpenAiMessageContent::Text(text)),
            name: None,
            tool_calls: None,
            tool_call_id: None,
            refusal: None,
        });
    }
    Ok(ProviderRequest::OpenAiChatCompletion(OpenAiChatRequest {
        model,
        messages: chat_messages,
        response_format: None,
        stream: None,
        stream_options: None,
        tools: None,
        tool_choice: None,
        parallel_tool_calls: None,
        settings,
    }))
}

fn build_openai_responses(
    model: String,
    system: Option<String>,
    messages: Vec<String>,
    settings_value: Option<Value>,
) -> SkaldResult<ProviderRequest> {
    let settings: OpenAiResponsesSettings = decode_settings(ProviderName::OpenAi, settings_value)?;
    let input = messages
        .into_iter()
        .map(|text| OpenAiResponseItem::Message {
            role: "user".to_owned(),
            content: vec![OpenAiResponseContentPart::InputText { text }],
        })
        .collect();
    Ok(ProviderRequest::OpenAiResponses(OpenAiResponsesRequest {
        model,
        input,
        instructions: system,
        text: None,
        tools: None,
        tool_choice: None,
        parallel_tool_calls: None,
        previous_response_id: None,
        stream: None,
        settings,
    }))
}

fn build_anthropic(
    model: String,
    system: Option<String>,
    messages: Vec<String>,
    settings_value: Option<Value>,
) -> SkaldResult<ProviderRequest> {
    let settings: AnthropicMessagesSettings =
        decode_settings(ProviderName::Anthropic, settings_value)?;
    let anthropic_messages = messages
        .into_iter()
        .map(|text| AnthropicMessage {
            role: "user".to_owned(),
            content: vec![
                crate::wire::anthropic_messages::AnthropicContentBlock::Text {
                    text,
                    cache_control: None,
                    citations: None,
                },
            ],
        })
        .collect();
    Ok(ProviderRequest::AnthropicMessage(
        AnthropicMessagesRequest {
            model,
            messages: anthropic_messages,
            system: system.map(AnthropicSystem::Text),
            stream: None,
            tools: None,
            tool_choice: None,
            settings,
        },
    ))
}

fn build_gemini(
    system: Option<String>,
    messages: Vec<String>,
    settings_value: Option<Value>,
    vertex_target: bool,
) -> SkaldResult<ProviderRequest> {
    let provider = if vertex_target {
        ProviderName::Vertex
    } else {
        ProviderName::Google
    };
    let settings: GoogleGenerateSettings = decode_settings(provider, settings_value)?;
    let contents = messages
        .into_iter()
        .map(|text| GoogleContent {
            role: "user".to_owned(),
            parts: vec![GooglePart::Text { text }],
        })
        .collect();
    let request = GoogleGenerateContentRequest {
        contents,
        system_instruction: system.map(|text| GoogleContent {
            role: "user".to_owned(),
            parts: vec![GooglePart::Text { text }],
        }),
        tools: None,
        tool_config: None,
        settings,
    };
    if vertex_target {
        Ok(ProviderRequest::Vertex(VertexGenerateContentRequest(
            request,
        )))
    } else {
        Ok(ProviderRequest::GeminiGenerateContent(request))
    }
}
