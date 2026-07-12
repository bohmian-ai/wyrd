//! Provider-shaped constructors that emit native Skald prompt requests.

use serde_json::value::RawValue;
use skald_spec::wire::anthropic_messages::{
    AnthropicMessage, AnthropicMessagesRequest, AnthropicMessagesSettings,
};
use skald_spec::wire::google_generate::{
    GoogleContent, GoogleGenerateContentRequest, GoogleGenerateSettings, GoogleGenerationConfig,
};
use skald_spec::wire::openai_chat::{
    OpenAiChatRequest, OpenAiChatSettings, OpenAiMessageContent, OpenAiResponseFormat,
};
use skald_spec::wire::openai_responses::{
    OpenAiResponseContentPart, OpenAiResponseItem, OpenAiResponsesRequest, OpenAiResponsesSettings,
    OpenAiResponsesText, OpenAiTextResponseFormat,
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
    /// Optional structured-output schema. Wins over `response_format` when set.
    pub output: Option<ResponseFormat>,
    /// Optional `OpenAI` prompt cache key.
    pub prompt_cache_key: Option<String>,
    /// Native `OpenAI` Chat generation settings.
    pub settings: OpenAiChatSettings,
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
    /// Optional structured-output schema. Wins over `response_format` when set.
    pub output: Option<ResponseFormat>,
    /// Native `OpenAI` Responses generation settings.
    pub settings: OpenAiResponsesSettings,
    /// Declared render variables.
    pub variables: Vec<String>,
    /// Optional prompt version.
    pub version: Option<String>,
}

/// Anthropic Messages constructor options.
#[derive(Debug, Clone, Default)]
pub struct AnthropicOptions {
    /// Optional top-level Anthropic system text.
    pub system: Option<String>,
    /// User message strings to append as native Anthropic messages.
    pub messages: Vec<String>,
    /// Optional response format recorded on the native Prompt metadata.
    pub response_format: Option<ResponseFormat>,
    /// Optional structured-output schema. Wins over `response_format` when set.
    pub output: Option<ResponseFormat>,
    /// Native Anthropic generation settings.
    pub settings: AnthropicMessagesSettings,
    /// Declared render variables.
    pub variables: Vec<String>,
    /// Optional prompt version.
    pub version: Option<String>,
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
    /// Optional structured-output schema. Wins over `response_format` when set.
    pub output: Option<ResponseFormat>,
    /// Native Google/Gemini request settings.
    pub settings: GoogleGenerateSettings,
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
    let effective =
        effective_response_format(options.response_format.as_ref(), options.output.as_ref());
    let response_type = response_type(effective);
    let response_format = effective.map(openai_chat_response_format).transpose()?;
    let mut settings = options.settings;
    if settings.prompt_cache_key.is_none() {
        settings.prompt_cache_key = options.prompt_cache_key;
    }
    let mut messages = Vec::new();
    if let Some(system) = options.system {
        messages.push(skald_spec::OpenAiChatMessage {
            role: "system".to_owned(),
            content: Some(OpenAiMessageContent::Text(system)),
            ..Default::default()
        });
    }
    messages.extend(options.messages.into_iter().map(openai_user_message));

    finalize_prompt(skald_spec::Prompt {
        request: ProviderRequest::OpenAiChatCompletion(OpenAiChatRequest {
            model: model.clone(),
            messages,
            response_format,
            stream: None,
            stream_options: None,
            tools: None,
            tool_choice: None,
            parallel_tool_calls: None,
            settings,
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
    let effective =
        effective_response_format(options.response_format.as_ref(), options.output.as_ref());
    let response_type = response_type(effective);
    let text = effective.map(openai_responses_text).transpose()?;
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
            text,
            tools: None,
            tool_choice: None,
            parallel_tool_calls: None,
            previous_response_id: None,
            stream: None,
            settings: options.settings,
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
    let effective =
        effective_response_format(options.response_format.as_ref(), options.output.as_ref());
    let response_type = response_type(effective);
    let output_config = effective
        .map(anthropic_output_config)
        .transpose()?
        .flatten();
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
            system: options
                .system
                .map(skald_spec::wire::anthropic_messages::AnthropicSystem::Text),
            stream: None,
            tools: None,
            tool_choice: None,
            output_config,
            settings: options.settings,
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
    let effective =
        effective_response_format(options.response_format.as_ref(), options.output.as_ref());
    let response_type = response_type(effective);
    let settings = google_settings(&options, effective)?;
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
        tools: None,
        tool_config: None,
        settings,
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
    if prompt.variables.is_empty() {
        prompt = skald_spec::Prompt::new(
            prompt.request,
            prompt.model,
            prompt.version,
            prompt.response_type,
        )?;
    } else {
        prompt.normalize_media_placeholders_mut()?;
    }
    Ok(Prompt::from_native(prompt))
}

fn response_type(format: Option<&ResponseFormat>) -> ResponseType {
    format.map_or(ResponseType::Text, ResponseFormat::response_type)
}

fn effective_response_format<'a>(
    response_format: Option<&'a ResponseFormat>,
    output: Option<&'a ResponseFormat>,
) -> Option<&'a ResponseFormat> {
    output.or(response_format)
}

fn openai_user_message(content: String) -> skald_spec::OpenAiChatMessage {
    skald_spec::OpenAiChatMessage {
        role: "user".to_owned(),
        content: Some(OpenAiMessageContent::Text(content)),
        ..Default::default()
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

fn anthropic_output_config(
    format: &ResponseFormat,
) -> PromptBuilderResult<Option<skald_spec::wire::anthropic_messages::AnthropicOutputConfig>> {
    use skald_spec::wire::anthropic_messages::{AnthropicOutputConfig, AnthropicOutputFormat};

    Ok(match format.kind() {
        ResponseFormatKind::Text | ResponseFormatKind::JsonObject => None,
        ResponseFormatKind::JsonSchema { schema, .. } => {
            schema_object(schema)?;
            Some(AnthropicOutputConfig {
                format: AnthropicOutputFormat::JsonSchema {
                    schema: schema.clone(),
                },
            })
        }
    })
}

fn google_settings(
    options: &GeminiOptions,
    format: Option<&ResponseFormat>,
) -> PromptBuilderResult<GoogleGenerateSettings> {
    let mut settings = options.settings.clone();
    let mut config = settings.generation_config.take().unwrap_or_default();
    if let Some(format) = format {
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

    if config != GoogleGenerationConfig::default() {
        settings.generation_config = Some(config);
    }
    Ok(settings)
}

#[cfg(test)]
mod builder {
    use serde_json::json;

    use crate::messages::{
        anthropic_image_base64_block, google_file_data_part, openai_file_id_part,
    };
    use crate::{
        AnthropicOptions, GeminiOptions, OpenAiChatOptions, OpenAiResponsesOptions, ResponseFormat,
        anthropic, gemini, openai_chat, openai_responses, raw, vertex,
    };
    use skald_spec::wire::anthropic_messages::{AnthropicContentBlock, AnthropicImageSource};
    use skald_spec::wire::google_generate::GooglePart;
    use skald_spec::wire::openai_chat::{OpenAiContentPart, OpenAiResponseFormat};
    use skald_spec::wire::openai_responses::OpenAiTextResponseFormat;
    use skald_spec::{ProviderName, ProviderRequest, ResponseType};

    #[test]
    fn openai_chat_emits_native_variant_and_response_format() {
        let schema = json!({
            "type": "object",
            "properties": {
                "answer": { "type": "string" }
            },
            "required": ["answer"]
        });
        let prompt = openai_chat(
            "gpt-4o",
            OpenAiChatOptions {
                system: Some("Answer exactly.".to_owned()),
                messages: vec!["Hello {{name}}".to_owned()],
                response_format: Some(
                    ResponseFormat::json_schema("answer", schema.clone()).unwrap(),
                ),
                variables: vec!["name".to_owned()],
                version: Some("v1".to_owned()),
                ..OpenAiChatOptions::default()
            },
        )
        .unwrap();

        assert_eq!(
            prompt.native().response_type,
            ResponseType::JsonSchema {
                name: "answer".to_owned(),
                schema
            }
        );
        let ProviderRequest::OpenAiChatCompletion(request) = &prompt.native().request else {
            panic!("expected OpenAI chat request");
        };
        assert_eq!(request.messages.len(), 2);
        assert!(matches!(
            request.response_format,
            Some(OpenAiResponseFormat::JsonSchema { .. })
        ));
    }

    #[test]
    fn openai_responses_emits_native_text_format() {
        let prompt = openai_responses(
            "gpt-4.1",
            OpenAiResponsesOptions {
                messages: vec!["Summarize this".to_owned()],
                response_format: Some(ResponseFormat::json_object()),
                ..OpenAiResponsesOptions::default()
            },
        )
        .unwrap();

        let ProviderRequest::OpenAiResponses(request) = &prompt.native().request else {
            panic!("expected OpenAI Responses request");
        };
        let format = request.text.as_ref().and_then(|text| text.format.as_ref());
        assert!(matches!(format, Some(OpenAiTextResponseFormat::JsonObject)));
    }

    #[test]
    fn output_wins_over_response_format_for_openai_chat() {
        let output_schema = json!({
            "type": "object",
            "properties": {"summary": {"type": "string"}},
            "required": ["summary"],
            "additionalProperties": false
        });
        let prompt = openai_chat(
            "gpt-4o",
            OpenAiChatOptions {
                messages: vec!["summarize".to_owned()],
                response_format: Some(ResponseFormat::json_object()),
                output: Some(
                    ResponseFormat::json_schema("summary", output_schema.clone()).unwrap(),
                ),
                ..OpenAiChatOptions::default()
            },
        )
        .unwrap();

        assert_eq!(
            prompt.native().response_type,
            ResponseType::JsonSchema {
                name: "summary".to_owned(),
                schema: output_schema
            }
        );
        let ProviderRequest::OpenAiChatCompletion(request) = &prompt.native().request else {
            panic!("expected OpenAI chat request");
        };
        assert!(matches!(
            request.response_format,
            Some(OpenAiResponseFormat::JsonSchema { .. })
        ));
    }

    #[test]
    fn anthropic_output_emits_output_config() {
        let schema = json!({
            "type": "object",
            "properties": {"summary": {"type": "string"}},
            "required": ["summary"],
            "additionalProperties": false
        });
        let prompt = anthropic(
            "claude-sonnet-4",
            AnthropicOptions {
                messages: vec!["summarize".to_owned()],
                output: Some(ResponseFormat::json_schema("summary", schema).unwrap()),
                ..AnthropicOptions::default()
            },
        )
        .unwrap();

        let ProviderRequest::AnthropicMessage(request) = &prompt.native().request else {
            panic!("expected Anthropic request");
        };
        assert!(request.output_config.is_some());
    }

    #[test]
    fn role_helpers_append_native_messages_without_neutral_storage() {
        let prompt = anthropic(
            "claude-sonnet-4",
            AnthropicOptions {
                messages: vec!["first".to_owned()],
                ..AnthropicOptions::default()
            },
        )
        .unwrap()
        .with_assistant("second")
        .unwrap()
        .with_tool_result("toolu_1", "done", false)
        .unwrap();

        let ProviderRequest::AnthropicMessage(request) = &prompt.native().request else {
            panic!("expected Anthropic request");
        };
        assert_eq!(request.messages.len(), 3);
        assert!(matches!(
            request.messages[2].content[0],
            AnthropicContentBlock::ToolResult { .. }
        ));
    }

    #[test]
    fn google_and_vertex_emit_distinct_native_variants() {
        let gemini_prompt = gemini(
            "gemini-2.5-pro",
            GeminiOptions {
                messages: vec!["hello".to_owned()],
                ..GeminiOptions::default()
            },
        )
        .unwrap();
        assert!(matches!(
            gemini_prompt.native().request,
            ProviderRequest::GeminiGenerateContent(_)
        ));

        let vertex_prompt = vertex(
            "gemini-2.5-pro",
            GeminiOptions {
                messages: vec!["hello".to_owned()],
                ..GeminiOptions::default()
            },
        )
        .unwrap();
        assert!(matches!(
            vertex_prompt.native().request,
            ProviderRequest::Vertex(_)
        ));
    }

    #[test]
    fn raw_preserves_provider_and_body_bytes() {
        let prompt = raw(
            ProviderName::Custom("local".to_owned()),
            "local-model",
            br#"{"a":1}"#,
        )
        .unwrap();

        let ProviderRequest::RawV1 { provider, body } = &prompt.native().request else {
            panic!("expected raw request");
        };
        assert_eq!(provider, &ProviderName::Custom("local".to_owned()));
        assert_eq!(body.get(), r#"{"a":1}"#);
    }

    #[test]
    fn native_content_helpers_build_provider_wire_parts() {
        assert!(matches!(
            openai_file_id_part("file_123"),
            OpenAiContentPart::File { .. }
        ));
        assert!(matches!(
            anthropic_image_base64_block("image/png", "AAAA"),
            AnthropicContentBlock::Image {
                source: AnthropicImageSource::Base64 { .. },
                ..
            }
        ));
        assert!(matches!(
            google_file_data_part("application/pdf", "gs://bucket/file.pdf"),
            GooglePart::FileData { .. }
        ));
    }
}
