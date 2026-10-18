//! Provider-shaped constructors that emit native Skald prompt requests.

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
use skald_spec::{ProviderName, ProviderRequest};

use crate::coerce::{checked_model, schema_object};
use crate::error::PromptBuilderResult;
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
}

/// Vertex `GenerateContent` constructor options.
pub type VertexOptions = GeminiOptions;

/// Builds a native `OpenAI` Chat prompt.
///
/// # Errors
///
/// Returns [`PromptBuilderError::EmptyModel`](crate::error::PromptBuilderError::EmptyModel) for a blank model,
/// [`PromptBuilderError::InvalidResponseSchema`](crate::error::PromptBuilderError::InvalidResponseSchema) when a JSON-schema response
/// format is not a JSON object, and [`PromptBuilderError::Skald`](crate::error::PromptBuilderError::Skald) when the
/// model is not literal, the output schema is invalid for the destination, or
/// native Prompt construction otherwise rejects the request.
pub fn openai_chat(
    model: impl Into<String>,
    options: OpenAiChatOptions,
) -> PromptBuilderResult<Prompt> {
    let model = checked_model(model)?;
    let effective =
        effective_response_format(options.response_format.as_ref(), options.output.as_ref());
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

    finalize_prompt(
        ProviderRequest::OpenAiChatCompletion(OpenAiChatRequest {
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
        None,
    )
}

/// Builds a native `OpenAI` Responses prompt.
///
/// # Errors
///
/// Returns [`PromptBuilderError::EmptyModel`](crate::error::PromptBuilderError::EmptyModel) for a blank model,
/// [`PromptBuilderError::InvalidResponseSchema`](crate::error::PromptBuilderError::InvalidResponseSchema) when a JSON-schema response
/// format is not a JSON object, and [`PromptBuilderError::Skald`](crate::error::PromptBuilderError::Skald) when the
/// model is not literal, the output schema is invalid for the destination, or
/// native Prompt construction otherwise rejects the request.
pub fn openai_responses(
    model: impl Into<String>,
    options: OpenAiResponsesOptions,
) -> PromptBuilderResult<Prompt> {
    let model = checked_model(model)?;
    let effective =
        effective_response_format(options.response_format.as_ref(), options.output.as_ref());
    let text = effective.map(openai_responses_text).transpose()?;
    let input = options
        .messages
        .into_iter()
        .map(openai_response_user_item)
        .collect::<Vec<_>>()
        .into();

    finalize_prompt(
        ProviderRequest::OpenAiResponses(OpenAiResponsesRequest {
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
        None,
    )
}

/// Builds a native Anthropic Messages prompt.
///
/// # Errors
///
/// Returns [`PromptBuilderError::EmptyModel`](crate::error::PromptBuilderError::EmptyModel) for a blank model,
/// [`PromptBuilderError::InvalidResponseSchema`](crate::error::PromptBuilderError::InvalidResponseSchema) when a JSON-schema response
/// format is not a JSON object, and [`PromptBuilderError::Skald`](crate::error::PromptBuilderError::Skald) when the
/// model is not literal, the output schema is invalid for the destination, or
/// native Prompt construction otherwise rejects the request.
pub fn anthropic(
    model: impl Into<String>,
    options: AnthropicOptions,
) -> PromptBuilderResult<Prompt> {
    let model = checked_model(model)?;
    let effective =
        effective_response_format(options.response_format.as_ref(), options.output.as_ref());
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

    finalize_prompt(
        ProviderRequest::AnthropicMessage(AnthropicMessagesRequest {
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
        None,
    )
}

/// Builds a native Google Gemini `GenerateContent` prompt.
///
/// # Errors
///
/// Returns [`PromptBuilderError::EmptyModel`](crate::error::PromptBuilderError::EmptyModel) for a blank model,
/// [`PromptBuilderError::InvalidResponseSchema`](crate::error::PromptBuilderError::InvalidResponseSchema) when a JSON-schema response
/// format is not a JSON object, and [`PromptBuilderError::Skald`](crate::error::PromptBuilderError::Skald) when the
/// model is not literal, the output schema is invalid for the destination, or
/// native Prompt construction otherwise rejects the request.
pub fn gemini(model: impl Into<String>, options: GeminiOptions) -> PromptBuilderResult<Prompt> {
    let model = checked_model(model)?;
    let prompt = google_prompt(model, options, false)?;
    Ok(prompt)
}

/// Builds a native Vertex `GenerateContent` prompt.
///
/// Vertex shares the Gemini `GenerateContent` body schema, so the Prompt
/// carries a `GeminiGenerateContent` request and sets `Prompt.provider` to
/// Vertex as its dispatch destination.
///
/// # Errors
///
/// Returns [`PromptBuilderError::EmptyModel`](crate::error::PromptBuilderError::EmptyModel) for a blank model,
/// [`PromptBuilderError::InvalidResponseSchema`](crate::error::PromptBuilderError::InvalidResponseSchema) when a JSON-schema response
/// format is not a JSON object, and [`PromptBuilderError::Skald`](crate::error::PromptBuilderError::Skald) when the
/// model is not literal, the output schema is invalid for the destination, or
/// native Prompt construction otherwise rejects the request.
pub fn vertex(model: impl Into<String>, options: VertexOptions) -> PromptBuilderResult<Prompt> {
    let model = checked_model(model)?;
    google_prompt(model, options, true)
}

/// Builds the shared `GenerateContent` Prompt behind [`gemini`] and [`vertex`].
///
/// The request body is identical for both destinations; `vertex_target` only
/// selects whether `Prompt.provider` is set to Vertex or left unset so dispatch
/// uses the schema's Google default.
///
/// # Errors
///
/// Returns [`PromptBuilderError::InvalidResponseSchema`](crate::error::PromptBuilderError::InvalidResponseSchema) when a JSON-schema
/// response format is not a JSON object, and [`PromptBuilderError::Skald`](crate::error::PromptBuilderError::Skald)
/// when [`finalize_prompt`] rejects the request.
fn google_prompt(
    model: String,
    options: GeminiOptions,
    vertex_target: bool,
) -> PromptBuilderResult<Prompt> {
    let effective =
        effective_response_format(options.response_format.as_ref(), options.output.as_ref());
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
    finalize_prompt(
        ProviderRequest::GeminiGenerateContent(request),
        model,
        vertex_target.then_some(ProviderName::Vertex),
    )
}

/// Builds the Prompt through [`skald_spec::Prompt::new`], which derives
/// variables, media placeholders, and the response type from the request,
/// then sets the dispatch destination (for example Vertex for a shared
/// `GenerateContent` body) and wraps it for callers.
///
/// # Errors
///
/// Returns [`PromptBuilderError::Skald`](crate::error::PromptBuilderError::Skald) when the model is not literal, the
/// output schema is invalid for its destination, system content holds a
/// media placeholder, or the request cannot be serialized for variable
/// extraction.
pub(crate) fn finalize_prompt(
    request: ProviderRequest,
    model: String,
    provider: Option<ProviderName>,
) -> PromptBuilderResult<Prompt> {
    let mut prompt = skald_spec::Prompt::new(request, model)?;
    prompt.provider = provider;
    Ok(Prompt::from_native(prompt))
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

/// Projects the effective response format onto the caller's native Google
/// settings for [`google_prompt`].
///
/// Native settings are kept as authored. A JSON-object format sets the JSON
/// MIME type; a JSON-schema format also replaces any native `response_schema`
/// with `response_json_schema`, because Google refuses both together. An
/// all-default generation config is omitted.
///
/// # Errors
///
/// Returns [`PromptBuilderError::InvalidResponseSchema`](crate::error::PromptBuilderError::InvalidResponseSchema)
/// when the JSON-schema format is not a JSON object.
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
                // Google refuses a config carrying both schema fields.
                config.response_schema = None;
                config.response_json_schema = Some(schema.clone());
            }
        }
    }

    if config != GoogleGenerationConfig::default() {
        settings.generation_config = Some(config);
    }
    Ok(settings)
}

#[cfg(test)]
mod builder_tests {
    use serde_json::json;

    use crate::messages::{
        anthropic_image_base64_block, google_file_data_part, openai_file_id_part,
    };
    use crate::{
        AnthropicOptions, GeminiOptions, OpenAiChatOptions, OpenAiResponsesOptions, ResponseFormat,
        anthropic, gemini, openai_chat, openai_responses, vertex,
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

    /// Proves builders share the authored literal-model rule: placeholder
    /// and control-character models are refused as the `model` field while a
    /// slash model is kept exactly.
    ///
    /// # Panics
    /// Panics when a templated or control-character model builds, its refusal
    /// names another field, or the slash model is not kept.
    #[test]
    fn builders_refuse_templated_models() {
        let options = || AnthropicOptions {
            messages: vec!["{{question}}".to_owned()],
            ..AnthropicOptions::default()
        };
        for model in ["{{target}}", "${target}", "claude\t1"] {
            let error = anthropic(model, options()).expect_err("templated model is refused");
            assert!(
                matches!(&error, crate::PromptBuilderError::Skald(skald_spec::SkaldError::PromptInvalid { field, .. }) if field == "model"),
                "{model:?}: {error:?}"
            );
            assert!(openai_chat(model, OpenAiChatOptions::default()).is_err());
        }
        let prompt = anthropic("org/claude-1", options()).expect("slash model builds");
        assert_eq!(prompt.native().model, "org/claude-1");
        assert_eq!(prompt.native().variables, vec!["question"]);
    }

    /// Proves builder finalization shares the authored output-schema rule:
    /// an invalid type, a malformed nested keyword, and a non-object strict
    /// OpenAI root are refused at their native body path before any
    /// dispatch, while GenerateContent keeps a list root unchanged.
    ///
    /// # Panics
    /// Panics when an invalid schema builds, its refusal names another path or
    /// is not a field refusal, or the GenerateContent list root is not kept.
    #[test]
    fn builders_refuse_invalid_output_schemas() {
        let field = |error: crate::PromptBuilderError| match error {
            crate::PromptBuilderError::Skald(skald_spec::SkaldError::PromptInvalid {
                field,
                ..
            }) => field,
            other => panic!("expected a field refusal, got {other:?}"),
        };
        let bad = ResponseFormat::json_schema("answer", json!({"type": "text"})).unwrap();
        let error = openai_chat(
            "gpt-4o",
            OpenAiChatOptions {
                messages: vec!["hi".to_owned()],
                output: Some(bad),
                ..OpenAiChatOptions::default()
            },
        )
        .expect_err("invalid type is refused");
        assert_eq!(field(error), "body.response_format.json_schema.schema.type");
        let nested = ResponseFormat::json_schema(
            "answer",
            json!({"type": "object", "properties": {"n": {"type": "number", "minimum": "wrong"}}}),
        )
        .unwrap();
        let error = gemini(
            "gemini-2.5-pro",
            GeminiOptions {
                messages: vec!["hi".to_owned()],
                output: Some(nested),
                ..GeminiOptions::default()
            },
        )
        .expect_err("a malformed nested keyword is refused");
        assert_eq!(
            field(error),
            "body.generation_config.response_json_schema.properties.n.minimum"
        );

        let list = json!({"type": "array", "items": {"type": "string"}});
        let error = openai_responses(
            "gpt-4.1",
            OpenAiResponsesOptions {
                messages: vec!["hi".to_owned()],
                output: Some(ResponseFormat::json_schema("answer", list.clone()).unwrap()),
                ..OpenAiResponsesOptions::default()
            },
        )
        .expect_err("strict responses output needs an object root");
        assert_eq!(field(error), "body.text.format.schema.type");
        let prompt = gemini(
            "gemini-2.5-pro",
            GeminiOptions {
                messages: vec!["hi".to_owned()],
                output: Some(ResponseFormat::json_schema("answer", list.clone()).unwrap()),
                ..GeminiOptions::default()
            },
        )
        .expect("generate_content accepts a list root");
        assert!(matches!(
            &prompt.native().response_type,
            ResponseType::JsonSchema { schema, .. } if *schema == list
        ));
    }

    /// The Responses builder writes a JSON-object response format as the native
    /// `text.format` field rather than a Chat `response_format`.
    ///
    /// # Panics
    /// Panics when the Prompt does not build, is not a Responses request, or
    /// carries another text format.
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
            ProviderRequest::GeminiGenerateContent(_)
        ));
        assert_eq!(vertex_prompt.native().provider(), ProviderName::Vertex);
    }

    /// Proves an effective output schema replaces a native `response_schema`
    /// for both Google builders: Google refuses a request carrying both schema
    /// fields, so the serialized config must hold only `response_json_schema`
    /// while unrelated native settings survive.
    ///
    /// # Panics
    ///
    /// Panics when a builder refuses the request, emits a non-Google variant,
    /// serializes both schema fields, or drops the native temperature.
    #[test]
    fn google_output_schema_replaces_native_response_schema() {
        let schema = json!({
            "type": "object",
            "properties": {"summary": {"type": "string"}},
            "required": ["summary"]
        });
        let options = || GeminiOptions {
            messages: vec!["summarize".to_owned()],
            output: Some(ResponseFormat::json_schema("summary", schema.clone()).unwrap()),
            settings: skald_spec::wire::google_generate::GoogleGenerateSettings {
                generation_config: Some(
                    skald_spec::wire::google_generate::GoogleGenerationConfig {
                        temperature: Some(0.5),
                        response_schema: Some(json!({"type": "STRING"})),
                        ..Default::default()
                    },
                ),
                ..Default::default()
            },
            ..GeminiOptions::default()
        };
        for prompt in [
            gemini("gemini-2.5-pro", options()).unwrap(),
            vertex("gemini-2.5-pro", options()).unwrap(),
        ] {
            let ProviderRequest::GeminiGenerateContent(request) = &prompt.native().request else {
                panic!("expected Google request");
            };
            let config =
                serde_json::to_value(&request.settings).unwrap()["generation_config"].clone();
            assert!(config.get("response_schema").is_none(), "{config}");
            assert_eq!(config["response_json_schema"], schema);
            assert_eq!(config["temperature"], json!(0.5));
        }
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
