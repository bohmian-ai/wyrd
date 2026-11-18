//! Provider-shaped constructors that emit native Skald prompt requests.

use skald_spec::wire::anthropic_messages::{
    AnthropicMessage, AnthropicMessagesRequest, AnthropicMessagesSettings,
};
use skald_spec::wire::google_generate::{
    GoogleContent, GoogleGenerateContentRequest, GoogleGenerateSettings, GoogleGenerationConfig,
};
use skald_spec::wire::jev::{JevNoulCriteria, JevQuestion, JevRequest, JevRequestView};
use skald_spec::wire::openai_chat::{
    OpenAiChatRequest, OpenAiChatSettings, OpenAiMessageContent, OpenAiResponseFormat,
};
use skald_spec::wire::openai_responses::{
    OpenAiResponseContentPart, OpenAiResponseItem, OpenAiResponsesRequest, OpenAiResponsesSettings,
    OpenAiResponsesText, OpenAiTextResponseFormat,
};
use skald_spec::{ProviderName, ProviderRequest};

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

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
    let response_format = effective
        .map(openai_chat_response_format)
        .transpose()?
        .flatten();
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

    finish(
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
        effective,
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
    let text = effective.map(openai_responses_text).transpose()?.flatten();
    let input = options
        .messages
        .into_iter()
        .map(openai_response_user_item)
        .collect::<Vec<_>>()
        .into();

    finish(
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
        effective,
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

    finish(
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
        effective,
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
    finish(
        ProviderRequest::GeminiGenerateContent(request),
        model,
        vertex_target.then_some(ProviderName::Vertex),
        effective,
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

/// Builds the Prompt through [`finalize_prompt`], then applies a named
/// effective `format`, such as [`ResponseFormat::verdict`], which sets no
/// native field of its own.
///
/// # Errors
///
/// Returns [`finalize_prompt`]'s errors, and [`PromptBuilderError::Skald`](crate::error::PromptBuilderError::Skald)
/// for an invalid named format.
fn finish(
    request: ProviderRequest,
    model: String,
    provider: Option<ProviderName>,
    format: Option<&ResponseFormat>,
) -> PromptBuilderResult<Prompt> {
    let prompt = finalize_prompt(request, model, provider)?;
    match format.map(ResponseFormat::kind) {
        Some(ResponseFormatKind::Named(named)) => prompt.with_named_response_format(named.clone()),
        _ => Ok(prompt),
    }
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

/// The native `OpenAI` Chat `response_format`; a named format sets none and
/// is applied after the request is built.
///
/// # Errors
///
/// Returns [`PromptBuilderError::InvalidResponseSchema`](crate::error::PromptBuilderError::InvalidResponseSchema)
/// when a JSON Schema format's schema is not a JSON object.
fn openai_chat_response_format(
    format: &ResponseFormat,
) -> PromptBuilderResult<Option<OpenAiResponseFormat>> {
    Ok(Some(match format.kind() {
        ResponseFormatKind::Text => OpenAiResponseFormat::Text,
        ResponseFormatKind::JsonObject => OpenAiResponseFormat::JsonObject,
        ResponseFormatKind::JsonSchema { name, schema } => OpenAiResponseFormat::JsonSchema {
            json_schema: skald_spec::wire::openai_chat::OpenAiJsonSchema {
                name: name.clone(),
                description: None,
                schema: Some(schema_object(schema)?.into()),
                strict: Some(true),
            },
        },
        ResponseFormatKind::Named(_) => return Ok(None),
    }))
}

/// The native `OpenAI` Responses `text` setting; a named format sets none
/// and is applied after the request is built.
///
/// # Errors
///
/// Returns [`PromptBuilderError::InvalidResponseSchema`](crate::error::PromptBuilderError::InvalidResponseSchema)
/// when a JSON Schema format's schema is not a JSON object.
fn openai_responses_text(
    format: &ResponseFormat,
) -> PromptBuilderResult<Option<OpenAiResponsesText>> {
    Ok(Some(OpenAiResponsesText {
        format: Some(match format.kind() {
            ResponseFormatKind::Text => OpenAiTextResponseFormat::Text,
            ResponseFormatKind::JsonObject => OpenAiTextResponseFormat::JsonObject,
            ResponseFormatKind::JsonSchema { name, schema } => {
                OpenAiTextResponseFormat::JsonSchema {
                    name: name.clone(),
                    description: None,
                    schema: Some(schema_object(schema)?.into()),
                    strict: Some(true),
                }
            }
            ResponseFormatKind::Named(_) => return Ok(None),
        }),
    }))
}

/// Projects a JSON Schema response format onto Anthropic's native
/// `output_config`; text, JSON object, and named formats set no field.
///
/// # Errors
///
/// Returns [`PromptBuilderError::InvalidResponseSchema`](crate::error::PromptBuilderError::InvalidResponseSchema)
/// when the schema is not a JSON object.
fn anthropic_output_config(
    format: &ResponseFormat,
) -> PromptBuilderResult<Option<skald_spec::wire::anthropic_messages::AnthropicOutputConfig>> {
    use skald_spec::wire::anthropic_messages::{AnthropicOutputConfig, AnthropicOutputFormat};

    Ok(match format.kind() {
        ResponseFormatKind::Text
        | ResponseFormatKind::JsonObject
        | ResponseFormatKind::Named(_) => None,
        ResponseFormatKind::JsonSchema { schema, .. } => {
            schema_object(schema)?;
            Some(AnthropicOutputConfig {
                format: AnthropicOutputFormat::JsonSchema {
                    schema: schema.clone().into(),
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
            ResponseFormatKind::Text | ResponseFormatKind::Named(_) => {}
            ResponseFormatKind::JsonObject => {
                config.response_mime_type = Some("application/json".to_owned());
            }
            ResponseFormatKind::JsonSchema { schema, .. } => {
                schema_object(schema)?;
                config.response_mime_type = Some("application/json".to_owned());
                // Google refuses a config carrying both schema fields.
                config.response_schema = None;
                config.response_json_schema = Some(schema.clone().into());
            }
        }
    }

    if config != GoogleGenerationConfig::default() {
        settings.generation_config = Some(config);
    }
    Ok(settings)
}

/// A yes/no decision question, answered with the probability that it
/// holds.
///
/// A typed authoring helper shared by decision providers: each provider's
/// constructor lowers it to that provider's native question, so the stored
/// Prompt keeps only the native form.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NoulQuestion {
    /// What to decide about the state.
    pub instructions: String,
    /// Optional descriptions of the true and false outcomes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub criteria: Option<NoulCriteria>,
}

/// Descriptions of a [`NoulQuestion`]'s two outcomes.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NoulCriteria {
    /// When the answer is true.
    pub true_description: String,
    /// When the answer is false.
    pub false_description: String,
}

/// A question choosing exactly one of named options.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChoiceQuestion {
    /// What to choose about the state.
    pub instructions: String,
    /// Option name to its description; the names are the answer labels.
    pub criteria: BTreeMap<String, String>,
}

/// A question grading on an ordered scale whose levels are indexed from
/// zero, lowest first.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScoreQuestion {
    /// What to grade about the state.
    pub instructions: String,
    /// Level descriptions, lowest first.
    pub criteria: Vec<String>,
}

/// Any typed decision question, tagged by `type` as `noul`, `choice`, or
/// `score`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DecisionQuestion {
    /// A yes/no question.
    Noul(NoulQuestion),
    /// A choice among named options.
    Choice(ChoiceQuestion),
    /// A grade on an ordered scale.
    Score(ScoreQuestion),
}

impl From<NoulQuestion> for JevQuestion {
    /// Lowers the outcome descriptions to Jev's `{"true", "false"}` criteria.
    fn from(question: NoulQuestion) -> Self {
        Self::Noul {
            instructions: question.instructions,
            criteria: question.criteria.map(|criteria| JevNoulCriteria {
                when_true: criteria.true_description,
                when_false: criteria.false_description,
            }),
        }
    }
}

impl From<ChoiceQuestion> for JevQuestion {
    /// Keeps every option name and description unchanged.
    fn from(question: ChoiceQuestion) -> Self {
        Self::Choice {
            instructions: question.instructions,
            criteria: question.criteria,
        }
    }
}

impl From<ScoreQuestion> for JevQuestion {
    /// Keeps the level order unchanged.
    fn from(question: ScoreQuestion) -> Self {
        Self::Score {
            instructions: question.instructions,
            criteria: question.criteria,
        }
    }
}

impl From<DecisionQuestion> for JevQuestion {
    /// Lowers whichever question `question` holds.
    fn from(question: DecisionQuestion) -> Self {
        match question {
            DecisionQuestion::Noul(question) => question.into(),
            DecisionQuestion::Choice(question) => question.into(),
            DecisionQuestion::Score(question) => question.into(),
        }
    }
}

/// TypeSafe Jev `system_one` constructor options.
#[derive(Debug, Clone, Default)]
pub struct JevPromptOptions {
    /// The text the questions are about, usually a `${name}` placeholder
    /// bound per call.
    pub state: String,
    /// Native Jev questions keyed by the name their answer returns under;
    /// typed questions convert with `.into()`.
    pub questions: BTreeMap<String, JevQuestion>,
}

/// Builds a native TypeSafe Jev `system_one` prompt: one fixed decision
/// request whose answers a Verifier check's `verdict_transform` reads.
///
/// # Errors
///
/// Returns [`PromptBuilderError::EmptyModel`](crate::error::PromptBuilderError::EmptyModel) for a blank model,
/// [`PromptBuilderError::Validation`](crate::error::PromptBuilderError::Validation) naming the member that breaks a documented Jev
/// bound, such as no questions or too many Choice options, and
/// [`PromptBuilderError::Skald`](crate::error::PromptBuilderError::Skald) when native Prompt construction rejects the request.
pub fn jev(model: impl Into<String>, options: JevPromptOptions) -> PromptBuilderResult<Prompt> {
    let model = checked_model(model)?;
    let request = JevRequest {
        model: model.clone(),
        state: options.state,
        questions: options.questions,
    };
    // Construction-time only: the bounds live on the borrowed view the
    // gateway decodes, so check that same view of the built body.
    let body = serde_json::to_value(&request)
        .map_err(|error| PromptBuilderError::Validation(error.to_string()))?;
    JevRequestView::deserialize(&body)
        .map_err(|error| PromptBuilderError::Validation(error.to_string()))?
        .validate()
        .map_err(|error| PromptBuilderError::Validation(error.to_string()))?;
    finalize_prompt(ProviderRequest::JevSystemOne(request), model, None)
}

impl Prompt {
    /// Builds a native TypeSafe Jev `system_one` prompt; see [`jev`].
    ///
    /// # Errors
    ///
    /// Returns [`jev`]'s errors.
    pub fn jev(model: impl Into<String>, options: JevPromptOptions) -> PromptBuilderResult<Self> {
        jev(model, options)
    }
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

    /// `ResponseFormat::verdict` authors the named `verdict` format, plain
    /// or with custom data, and resolves it through the native Verdict
    /// schema builder; an invalid data schema is refused there.
    ///
    /// # Panics
    ///
    /// Panics when a valid format does not build or resolve to the Verdict
    /// schema, or an invalid one is accepted.
    #[test]
    fn verdict_format_resolves_through_the_native_builder() {
        let chat = |format| {
            openai_chat(
                "gpt-4o",
                OpenAiChatOptions {
                    messages: vec!["Judge {{report}}".into()],
                    response_format: Some(format),
                    ..Default::default()
                },
            )
        };
        let schema = |prompt: &crate::Prompt| {
            let ProviderRequest::OpenAiChatCompletion(request) = &prompt.native().request else {
                panic!("an OpenAI Chat request");
            };
            let Some(OpenAiResponseFormat::JsonSchema { json_schema }) = &request.response_format
            else {
                panic!(
                    "the Verdict schema is native: {:?}",
                    request.response_format
                );
            };
            json_schema.schema.clone().expect("a schema")
        };
        let plain = chat(ResponseFormat::verdict(None)).expect("plain verdict builds");
        assert_eq!(
            serde_json::to_value(&plain.native().authored().expect("authored").response_format)
                .expect("serializes"),
            json!("verdict")
        );
        let plain_schema = schema(&plain);
        assert!(plain_schema["properties"].get("data").is_none());

        let data = json!({
            "type": "object",
            "properties": {"score": {"type": "number"}},
            "required": ["score"],
            "additionalProperties": false,
        });
        let custom = chat(ResponseFormat::verdict(data.as_object().cloned()))
            .expect("custom verdict builds");
        let custom_schema = schema(&custom);
        assert_eq!(
            custom_schema["properties"]["outcome"],
            plain_schema["properties"]["outcome"]
        );
        assert!(
            custom_schema["required"]
                .as_array()
                .expect("required")
                .contains(&json!("data")),
            "custom data is required: {custom_schema:?}"
        );

        let refused = chat(ResponseFormat::verdict(
            json!({"$schema": "http://json-schema.org/draft-07/schema#", "type": "object"})
                .as_object()
                .cloned(),
        ))
        .expect_err("a non-2020-12 data schema is refused");
        assert!(refused.to_string().contains("2020-12"), "{refused}");
    }

    /// `ResponseFormat::code_review` authors the named `code_review` format,
    /// whose fixed report schema resolves into the native request.
    ///
    /// # Panics
    ///
    /// Panics when the format does not build, keep its name, or resolve to
    /// the report schema.
    #[test]
    fn code_review_format_resolves_to_the_report_schema() {
        let prompt = openai_chat(
            "gpt-4o",
            OpenAiChatOptions {
                messages: vec!["Review {{diff}}".into()],
                response_format: Some(ResponseFormat::code_review()),
                ..Default::default()
            },
        )
        .expect("code_review builds");
        assert_eq!(
            serde_json::to_value(
                &prompt
                    .native()
                    .authored()
                    .expect("authored")
                    .response_format
            )
            .expect("serializes"),
            json!("code_review")
        );
        let ProviderRequest::OpenAiChatCompletion(request) = &prompt.native().request else {
            panic!("an OpenAI Chat request");
        };
        let Some(OpenAiResponseFormat::JsonSchema { json_schema }) = &request.response_format
        else {
            panic!("the report schema is native: {:?}", request.response_format);
        };
        let schema = json_schema.schema.as_ref().expect("a schema");
        assert!(schema["properties"].get("summary").is_some(), "{schema:?}");
        assert!(schema["properties"].get("findings").is_some(), "{schema:?}");
    }

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
                schema: schema.into()
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
                schema: output_schema.into()
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
                        response_schema: Some(json!({"type": "STRING"}).into()),
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

    /// Typed questions lower to exactly the native Jev questions an
    /// equivalent native map declares: Noul descriptions become
    /// `{"true", "false"}`, omitted criteria stay omitted, Choice labels and
    /// Score order are unchanged, and the state is kept verbatim.
    ///
    /// # Panics
    ///
    /// Panics when the typed and native Prompts differ or do not build.
    #[test]
    fn jev_typed_questions_lower_to_native_questions() {
        use std::collections::BTreeMap;

        use skald_spec::wire::jev::{JevNoulCriteria, JevQuestion};

        use crate::Prompt;
        use crate::builder::{
            ChoiceQuestion, DecisionQuestion, JevPromptOptions, NoulCriteria, NoulQuestion,
            ScoreQuestion,
        };

        let typed: BTreeMap<String, JevQuestion> = [
            (
                "billing".to_owned(),
                NoulQuestion {
                    instructions: "About billing?".into(),
                    criteria: Some(NoulCriteria {
                        true_description: "A charge.".into(),
                        false_description: "Anything else.".into(),
                    }),
                }
                .into(),
            ),
            (
                "plain".to_owned(),
                DecisionQuestion::Noul(NoulQuestion {
                    instructions: "Polite?".into(),
                    criteria: None,
                })
                .into(),
            ),
            (
                "team".to_owned(),
                ChoiceQuestion {
                    instructions: "Which team?".into(),
                    criteria: BTreeMap::from([
                        ("zeta".into(), "Last.".into()),
                        ("alpha".into(), "First.".into()),
                    ]),
                }
                .into(),
            ),
            (
                "urgency".to_owned(),
                ScoreQuestion {
                    instructions: "How urgent?".into(),
                    criteria: vec!["Routine".into(), "Soon".into(), "Now".into()],
                }
                .into(),
            ),
        ]
        .into();
        let native: BTreeMap<String, JevQuestion> = serde_json::from_value(json!({
            "billing": {"type": "noul", "instructions": "About billing?",
                        "criteria": {"true": "A charge.", "false": "Anything else."}},
            "plain": {"type": "noul", "instructions": "Polite?"},
            "team": {"type": "choice", "instructions": "Which team?",
                     "criteria": {"zeta": "Last.", "alpha": "First."}},
            "urgency": {"type": "score", "instructions": "How urgent?",
                        "criteria": ["Routine", "Soon", "Now"]},
        }))
        .expect("native questions decode");
        assert_eq!(
            typed["billing"],
            JevQuestion::Noul {
                instructions: "About billing?".into(),
                criteria: Some(JevNoulCriteria {
                    when_true: "A charge.".into(),
                    when_false: "Anything else.".into(),
                }),
            }
        );
        let build = |questions| {
            Prompt::jev(
                "jev-latest",
                JevPromptOptions {
                    state: "He said \"hi\".\n${context}".into(),
                    questions,
                },
            )
            .expect("the Jev prompt builds")
        };
        let (typed, native) = (build(typed), build(native));
        assert_eq!(typed.native(), native.native());
        let ProviderRequest::JevSystemOne(request) = &typed.native().request else {
            panic!("a Jev system_one request");
        };
        assert_eq!(request.state, "He said \"hi\".\n${context}");
        assert_eq!(request.model, "jev-latest");
    }

    /// A Jev prompt breaking a documented bound is refused at construction,
    /// naming the offending member.
    ///
    /// # Panics
    ///
    /// Panics when an invalid prompt builds or the refusal names another
    /// member.
    #[test]
    fn jev_prompt_refuses_documented_bounds() {
        use std::collections::BTreeMap;

        use crate::builder::{ChoiceQuestion, JevPromptOptions, ScoreQuestion};
        use crate::{Prompt, PromptBuilderError};

        let refused =
            |questions: BTreeMap<String, skald_spec::wire::jev::JevQuestion>| match Prompt::jev(
                "jev-latest",
                JevPromptOptions {
                    state: "${context}".into(),
                    questions,
                },
            ) {
                Err(PromptBuilderError::Validation(reason)) => reason,
                other => panic!("expected a validation refusal, got {other:?}"),
            };
        assert!(refused(BTreeMap::new()).starts_with("questions"));
        let one_level = ScoreQuestion {
            instructions: "Grade".into(),
            criteria: vec!["Only".into()],
        };
        assert!(
            refused([("grade".to_owned(), one_level.into())].into())
                .starts_with("questions.grade.criteria")
        );
        let too_many = ChoiceQuestion {
            instructions: "Pick".into(),
            criteria: (0..300)
                .map(|i| (format!("o{i}"), "x".to_owned()))
                .collect(),
        };
        assert!(
            refused([("pick".to_owned(), too_many.into())].into())
                .starts_with("questions.pick.criteria")
        );
        assert!(matches!(
            Prompt::jev(" ", JevPromptOptions::default()),
            Err(PromptBuilderError::EmptyModel)
        ));
    }
}
