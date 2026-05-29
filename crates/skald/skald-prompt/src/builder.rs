//! Provider-shaped constructors that emit native Skald prompt requests.

use serde_json::value::RawValue;
use serde_json::{Map, Value};
use skald_spec::wire::anthropic_messages::{AnthropicMessage, AnthropicMessagesRequest};
use skald_spec::wire::google_generate::{
    GoogleContent, GoogleGenerateContentRequest, GoogleGenerationConfig,
};
use skald_spec::wire::openai_chat::{
    OpenAiChatRequest, OpenAiMessageContent, OpenAiResponseFormat, OpenAiStop,
};
use skald_spec::wire::openai_responses::{
    OpenAiResponseContentPart, OpenAiResponseItem, OpenAiResponsesRequest, OpenAiResponsesText,
    OpenAiTextResponseFormat,
};
use skald_spec::wire::vertex_generate::VertexGenerateContentRequest;
use skald_spec::{ProviderName, ProviderRequest, ResponseType};

use crate::coerce::{checked_model, schema_object};
use crate::error::{PromptBuilderError, PromptBuilderResult};
use crate::messages::{anthropic_text_block, google_text_part};
use crate::prompt::Prompt;
use crate::response_format::{ResponseFormat, ResponseFormatKind};

/// `OpenAI` Chat constructor options.
#[derive(Debug, Clone, Default)]
pub struct OpenAiChatOptions {
    /// Optional system message prepended to `messages`.
    pub system: Option<String>,
    /// User message strings to append as native `OpenAI` chat messages.
    pub messages: Vec<String>,
    /// Optional response format emitted into `response_format`.
    pub response_format: Option<ResponseFormat>,
    /// Optional sampling temperature.
    pub temperature: Option<f32>,
    /// Optional nucleus sampling value.
    pub top_p: Option<f32>,
    /// Optional maximum completion tokens.
    pub max_tokens: Option<u32>,
    /// Optional `OpenAI` stop configuration.
    pub stop: Option<OpenAiStop>,
    /// Optional `OpenAI` prompt cache key.
    pub prompt_cache_key: Option<String>,
    /// Explicit extra provider options for the flattened `extra` slot.
    pub provider_options: Map<String, Value>,
    /// Declared render variables.
    pub variables: Vec<String>,
    /// Optional prompt version.
    pub version: Option<String>,
}

/// `OpenAI` Responses constructor options.
#[derive(Debug, Clone, Default)]
pub struct OpenAiResponsesOptions {
    /// Optional instructions field.
    pub instructions: Option<String>,
    /// User message strings converted to native response input items.
    pub messages: Vec<String>,
    /// Optional response format emitted into `text.format`.
    pub response_format: Option<ResponseFormat>,
    /// Optional sampling temperature.
    pub temperature: Option<f32>,
    /// Optional nucleus sampling value.
    pub top_p: Option<f32>,
    /// Optional maximum output tokens.
    pub max_output_tokens: Option<u32>,
    /// Explicit extra provider options for the flattened `extra` slot.
    pub provider_options: Map<String, Value>,
    /// Declared render variables.
    pub variables: Vec<String>,
    /// Optional prompt version.
    pub version: Option<String>,
}

/// Anthropic Messages constructor options.
#[derive(Debug, Clone)]
pub struct AnthropicOptions {
    /// Optional top-level Anthropic system text.
    pub system: Option<String>,
    /// User message strings to append as native Anthropic messages.
    pub messages: Vec<String>,
    /// Required Anthropic maximum output tokens.
    pub max_tokens: u32,
    /// Optional response format recorded on the native Prompt metadata.
    pub response_format: Option<ResponseFormat>,
    /// Optional sampling temperature.
    pub temperature: Option<f32>,
    /// Optional nucleus sampling value.
    pub top_p: Option<f32>,
    /// Optional top-k sampling value.
    pub top_k: Option<u32>,
    /// Explicit extra provider options for the flattened `extra` slot.
    pub provider_options: Map<String, Value>,
    /// Declared render variables.
    pub variables: Vec<String>,
    /// Optional prompt version.
    pub version: Option<String>,
}

impl Default for AnthropicOptions {
    fn default() -> Self {
        Self {
            system: None,
            messages: Vec::new(),
            max_tokens: 1024,
            response_format: None,
            temperature: None,
            top_p: None,
            top_k: None,
            provider_options: Map::new(),
            variables: Vec::new(),
            version: None,
        }
    }
}

/// Google Gemini `GenerateContent` constructor options.
#[derive(Debug, Clone, Default)]
pub struct GeminiOptions {
    /// Optional system instruction text.
    pub system: Option<String>,
    /// User message strings to append as native Google contents.
    pub messages: Vec<String>,
    /// Optional response format emitted into `generation_config`.
    pub response_format: Option<ResponseFormat>,
    /// Optional sampling temperature.
    pub temperature: Option<f32>,
    /// Optional nucleus sampling value.
    pub top_p: Option<f32>,
    /// Optional top-k sampling value.
    pub top_k: Option<u32>,
    /// Optional maximum output tokens.
    pub max_output_tokens: Option<u32>,
    /// Declared render variables.
    pub variables: Vec<String>,
    /// Optional prompt version.
    pub version: Option<String>,
}

/// Vertex `GenerateContent` constructor options.
pub type VertexOptions = GeminiOptions;

/// Builds a native `OpenAI` Chat prompt.
pub fn openai_chat(
    model: impl Into<String>,
    options: OpenAiChatOptions,
) -> PromptBuilderResult<Prompt> {
    let model = checked_model(model)?;
    let response_type = response_type(options.response_format.as_ref());
    let mut messages = Vec::new();
    if let Some(system) = options.system {
        messages.push(skald_spec::OpenAiChatMessage {
            role: "system".to_owned(),
            content: Some(OpenAiMessageContent::Text(system)),
            name: None,
            tool_calls: None,
            tool_call_id: None,
            refusal: None,
        });
    }
    messages.extend(options.messages.into_iter().map(openai_user_message));

    finalize_prompt(skald_spec::Prompt {
        request: ProviderRequest::OpenAiChatCompletion(OpenAiChatRequest {
            model: model.clone(),
            messages,
            temperature: options.temperature,
            top_p: options.top_p,
            max_tokens: options.max_tokens,
            max_completion_tokens: None,
            n: None,
            stop: options.stop,
            presence_penalty: None,
            frequency_penalty: None,
            seed: None,
            logit_bias: None,
            user: None,
            reasoning_effort: None,
            modalities: None,
            audio: None,
            prediction: None,
            response_format: options
                .response_format
                .as_ref()
                .map(openai_chat_response_format)
                .transpose()?,
            stream: None,
            stream_options: None,
            tools: None,
            tool_choice: None,
            parallel_tool_calls: None,
            prompt_cache_key: options.prompt_cache_key,
            service_tier: None,
            safety_identifier: None,
            store: None,
            metadata: None,
            logprobs: None,
            top_logprobs: None,
            extra: options.provider_options,
        }),
        model,
        version: options.version,
        variables: options.variables,
        media_variables: Vec::new(),
        response_type,
    })
}

/// Builds a native `OpenAI` Responses prompt.
pub fn openai_responses(
    model: impl Into<String>,
    options: OpenAiResponsesOptions,
) -> PromptBuilderResult<Prompt> {
    let model = checked_model(model)?;
    let response_type = response_type(options.response_format.as_ref());
    let input = options
        .messages
        .into_iter()
        .map(openai_response_user_item)
        .collect();

    finalize_prompt(skald_spec::Prompt {
        request: ProviderRequest::OpenAiResponses(OpenAiResponsesRequest {
            model: model.clone(),
            input,
            instructions: options.instructions,
            temperature: options.temperature,
            top_p: options.top_p,
            max_output_tokens: options.max_output_tokens,
            reasoning: None,
            text: options
                .response_format
                .as_ref()
                .map(openai_responses_text)
                .transpose()?,
            tools: None,
            tool_choice: None,
            parallel_tool_calls: None,
            previous_response_id: None,
            store: None,
            stream: None,
            include: None,
            metadata: None,
            extra: options.provider_options,
        }),
        model,
        version: options.version,
        variables: options.variables,
        media_variables: Vec::new(),
        response_type,
    })
}

/// Builds a native Anthropic Messages prompt.
pub fn anthropic(
    model: impl Into<String>,
    options: AnthropicOptions,
) -> PromptBuilderResult<Prompt> {
    let model = checked_model(model)?;
    let response_type = response_type(options.response_format.as_ref());
    let messages = options
        .messages
        .into_iter()
        .map(|text| AnthropicMessage {
            role: "user".to_owned(),
            content: vec![anthropic_text_block(text)],
        })
        .collect();

    finalize_prompt(skald_spec::Prompt {
        request: ProviderRequest::AnthropicMessage(AnthropicMessagesRequest {
            model: model.clone(),
            messages,
            max_tokens: options.max_tokens,
            system: options
                .system
                .map(skald_spec::wire::anthropic_messages::AnthropicSystem::Text),
            temperature: options.temperature,
            top_p: options.top_p,
            top_k: options.top_k,
            stop_sequences: None,
            metadata: None,
            stream: None,
            tools: None,
            tool_choice: None,
            thinking: None,
            extra: options.provider_options,
        }),
        model,
        version: options.version,
        variables: options.variables,
        media_variables: Vec::new(),
        response_type,
    })
}

/// Builds a native Google Gemini `GenerateContent` prompt.
pub fn gemini(model: impl Into<String>, options: GeminiOptions) -> PromptBuilderResult<Prompt> {
    let model = checked_model(model)?;
    let prompt = google_prompt(model, options, false)?;
    Ok(prompt)
}

/// Builds a native Vertex `GenerateContent` prompt.
pub fn vertex(model: impl Into<String>, options: VertexOptions) -> PromptBuilderResult<Prompt> {
    let model = checked_model(model)?;
    google_prompt(model, options, true)
}

/// Builds a raw passthrough prompt with a durable provider target.
pub fn raw(
    provider: ProviderName,
    model: impl Into<String>,
    bytes: &[u8],
) -> PromptBuilderResult<Prompt> {
    let model = checked_model(model)?;
    let body = std::str::from_utf8(bytes)
        .map_err(|error| PromptBuilderError::InvalidRawJson(error.to_string()))?;
    let body = RawValue::from_string(body.to_owned())
        .map_err(|error| PromptBuilderError::InvalidRawJson(error.to_string()))?;
    finalize_prompt(skald_spec::Prompt {
        request: ProviderRequest::RawV1 { provider, body },
        model,
        version: None,
        variables: Vec::new(),
        media_variables: Vec::new(),
        response_type: ResponseType::Text,
    })
}

fn google_prompt(
    model: String,
    options: GeminiOptions,
    vertex_target: bool,
) -> PromptBuilderResult<Prompt> {
    let response_type = response_type(options.response_format.as_ref());
    let generation_config = google_generation_config(&options)?;
    let request = GoogleGenerateContentRequest {
        contents: options
            .messages
            .into_iter()
            .map(|text| GoogleContent {
                role: "user".to_owned(),
                parts: vec![google_text_part(text)],
            })
            .collect(),
        system_instruction: options.system.map(|text| GoogleContent {
            role: "user".to_owned(),
            parts: vec![google_text_part(text)],
        }),
        generation_config,
        safety_settings: None,
        tools: None,
        tool_config: None,
        cached_content: None,
        labels: None,
    };
    finalize_prompt(skald_spec::Prompt {
        request: if vertex_target {
            ProviderRequest::Vertex(VertexGenerateContentRequest(request))
        } else {
            ProviderRequest::GeminiGenerateContent(request)
        },
        model,
        version: options.version,
        variables: options.variables,
        media_variables: Vec::new(),
        response_type,
    })
}

fn finalize_prompt(mut prompt: skald_spec::Prompt) -> PromptBuilderResult<Prompt> {
    prompt.normalize_media_placeholders_mut()?;
    Ok(Prompt::from_native(prompt))
}

fn response_type(format: Option<&ResponseFormat>) -> ResponseType {
    format.map_or(ResponseType::Text, ResponseFormat::response_type)
}

fn openai_user_message(content: String) -> skald_spec::OpenAiChatMessage {
    skald_spec::OpenAiChatMessage {
        role: "user".to_owned(),
        content: Some(OpenAiMessageContent::Text(content)),
        name: None,
        tool_calls: None,
        tool_call_id: None,
        refusal: None,
    }
}

fn openai_response_user_item(content: String) -> OpenAiResponseItem {
    OpenAiResponseItem::Message {
        role: "user".to_owned(),
        content: vec![OpenAiResponseContentPart::InputText { text: content }],
    }
}

fn openai_chat_response_format(
    format: &ResponseFormat,
) -> PromptBuilderResult<OpenAiResponseFormat> {
    Ok(match format.kind() {
        ResponseFormatKind::Text => OpenAiResponseFormat::Text,
        ResponseFormatKind::JsonObject => OpenAiResponseFormat::JsonObject,
        ResponseFormatKind::JsonSchema { name, schema } => OpenAiResponseFormat::JsonSchema {
            json_schema: skald_spec::wire::openai_chat::OpenAiJsonSchema {
                name: name.clone(),
                description: None,
                schema: Some(schema_object(schema)?),
                strict: Some(true),
            },
        },
    })
}

fn openai_responses_text(format: &ResponseFormat) -> PromptBuilderResult<OpenAiResponsesText> {
    Ok(OpenAiResponsesText {
        format: Some(match format.kind() {
            ResponseFormatKind::Text => OpenAiTextResponseFormat::Text,
            ResponseFormatKind::JsonObject => OpenAiTextResponseFormat::JsonObject,
            ResponseFormatKind::JsonSchema { name, schema } => {
                OpenAiTextResponseFormat::JsonSchema {
                    name: name.clone(),
                    description: None,
                    schema: Some(schema_object(schema)?),
                    strict: Some(true),
                }
            }
        }),
    })
}

fn google_generation_config(
    options: &GeminiOptions,
) -> PromptBuilderResult<Option<GoogleGenerationConfig>> {
    let mut config = GoogleGenerationConfig {
        temperature: options.temperature,
        top_p: options.top_p,
        top_k: options.top_k,
        max_output_tokens: options.max_output_tokens,
        ..GoogleGenerationConfig::default()
    };

    if let Some(format) = &options.response_format {
        match format.kind() {
            ResponseFormatKind::Text => {}
            ResponseFormatKind::JsonObject => {
                config.response_mime_type = Some("application/json".to_owned());
            }
            ResponseFormatKind::JsonSchema { schema, .. } => {
                schema_object(schema)?;
                config.response_mime_type = Some("application/json".to_owned());
                config.response_schema = Some(schema.clone());
            }
        }
    }

    if config == GoogleGenerationConfig::default() {
        Ok(None)
    } else {
        Ok(Some(config))
    }
}
