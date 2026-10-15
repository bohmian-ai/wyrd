//! The authored Prompt contract and its one compilation path.
//!
//! A Prompt is authored once, with `provider` and `model`, in one of two
//! forms: **portable** (`system`, `messages`, `settings`, `response_format`)
//! or **native** (`format` plus a protocol `body`). [`AuthoredPrompt`] is the
//! durable form Cards store and hash; [`AuthoredPrompt::compile`] validates it
//! and builds the runtime [`Prompt`], whose serialized form is again the
//! authored form it came from.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::error::{SkaldError, SkaldResult};
use crate::media::{MediaKind, MediaRef, MediaSource};
use crate::prompt::{Prompt, check_model, check_output_schema, media_placeholder_regex};
use crate::request::{ProviderName, ProviderRequest};
use crate::wire::anthropic_messages::{
    AnthropicContentBlock, AnthropicMessage, AnthropicMessagesRequest, AnthropicMessagesSettings,
    AnthropicOutputConfig, AnthropicOutputFormat, AnthropicSystem,
};
use crate::wire::google_generate::{
    GoogleContent, GoogleGenerateContentRequest, GoogleGenerateSettings, GoogleGenerationConfig,
    GooglePart,
};
use crate::wire::openai_chat::{
    OpenAiChatMessage, OpenAiChatRequest, OpenAiChatSettings, OpenAiContentPart, OpenAiJsonSchema,
    OpenAiMessageContent, OpenAiResponseFormat, OpenAiStop,
};

/// Schema name every portable structured-output request carries.
///
/// Deterministic so inline and standalone Prompts compile identically; the
/// authored schema needs no separate name.
const RESPONSE_SCHEMA_NAME: &str = "response";

/// The authored Prompt: provider and model once, plus portable content or
/// one native protocol body.
///
/// Decoding refuses unknown fields; [`AuthoredPrompt::compile`] refuses a
/// mixed form, an invalid provider/format pair, and invalid settings, media,
/// or schemas. Neither performs IO.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct AuthoredPrompt {
    /// Inference family or configured native provider; never a deployment.
    pub provider: ProviderName,
    /// Provider model identifier, preserved verbatim including any slash.
    pub model: String,
    /// Native protocol format; present exactly when `body` is.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format: Option<PromptFormat>,
    /// Native protocol content and options, excluding `model`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<Map<String, Value>>,
    /// Portable system text.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub system: Option<String>,
    /// Portable ordered user and assistant turns.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub messages: Option<Vec<PortableMessage>>,
    /// Portable generation settings.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub settings: Option<PortableSettings>,
    /// Portable structured output: one JSON Schema object.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub response_format: Option<Map<String, Value>>,
}

/// Native protocol formats a Prompt `body` may take.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum PromptFormat {
    /// OpenAI Chat Completions.
    ChatCompletion,
    /// OpenAI Responses.
    Responses,
    /// Anthropic Messages.
    Messages,
    /// Google and Vertex GenerateContent.
    GenerateContent,
    /// Jev `systemone`.
    SystemOne,
}

/// One portable conversation turn.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct PortableMessage {
    /// Turn author; system text lives only in `system`.
    pub role: PortableRole,
    /// Text, or ordered typed content blocks.
    pub content: PortableContent,
}

/// Author of a portable turn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum PortableRole {
    /// End-user turn.
    User,
    /// Prior model turn.
    Assistant,
}

/// Portable turn content.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(untagged)]
pub enum PortableContent {
    /// Convenience form of one text block.
    Text(String),
    /// Ordered typed content blocks.
    Blocks(Vec<ContentBlock>),
}

/// One typed portable content block.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum ContentBlock {
    /// Text, which may hold `{{name}}` placeholders.
    Text {
        /// Block text.
        text: String,
    },
    /// An image.
    Image {
        /// Literal media or one `${media:name}` binding.
        source: BlockSource,
    },
    /// A document.
    Document {
        /// Literal media or one `${media:name}` binding.
        source: BlockSource,
    },
}

/// Where an image or document block's media comes from.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(untagged)]
pub enum BlockSource {
    /// One isolated `${media:name}` placeholder bound at run time.
    Binding(String),
    /// Literal media, mapped through the shared media helpers.
    Media(MediaSource),
}

/// The eight portable generation settings.
///
/// A setting the destination protocol cannot carry is refused, never dropped.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct PortableSettings {
    /// Sampling temperature.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,
    /// Nucleus sampling mass in `[0, 1]`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f64>,
    /// Positive output-token limit; required for Anthropic.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_output_tokens: Option<u32>,
    /// Positive top-k sampling limit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub top_k: Option<u32>,
    /// Nonempty stop strings.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stop_sequences: Option<Vec<String>>,
    /// Best-effort reproducibility seed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seed: Option<i64>,
    /// Presence penalty.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub presence_penalty: Option<f64>,
    /// Frequency penalty.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub frequency_penalty: Option<f64>,
}

/// One portable turn lowered to ordered text parts, media as isolated
/// `${media:name}` placeholders the shared binder later replaces.
struct Turn {
    /// Turn author.
    role: PortableRole,
    /// Ordered text parts.
    parts: Vec<String>,
    /// Whether the author wrote the content as one plain string.
    plain: bool,
}

impl AuthoredPrompt {
    /// Validates this authored Prompt and builds its runtime [`Prompt`].
    ///
    /// Native bodies get `model` inserted (except GenerateContent, whose model
    /// lives in the URL) and decode through the typed wire owner. Portable
    /// content lowers to the destination's typed request; literal media is
    /// then bound through the shared media binder, so per-provider source and
    /// MIME rules apply identically to both forms. The compiled Prompt keeps
    /// this authored form for serialization.
    ///
    /// # Errors
    ///
    /// Returns [`SkaldError::PromptInvalid`] naming the offending field for a
    /// model that is not literal configuration (blank, control characters, or
    /// placeholder syntax), a mixed or incomplete form, an invalid provider/format
    /// pair, `body.model`, an undecodable body, empty or misplaced content, a
    /// malformed binding, or an out-of-range or unsupported setting; and the
    /// media binder's error for a literal source its provider refuses.
    pub fn compile(&self) -> SkaldResult<Prompt> {
        check_model(&self.model)?;
        let mut literals = Vec::new();
        let mut media_kinds = Vec::new();
        let request = match (self.format, &self.body) {
            (Some(format), Some(body)) => self.native_request(format, body)?,
            (None, None) => self.portable_request(&mut literals, &mut media_kinds)?,
            (Some(_), None) => return Err(invalid("body", "native `format` requires a `body`")),
            (None, Some(_)) => return Err(invalid("format", "native `body` requires a `format`")),
        };
        let mut prompt = Prompt::new(request, self.model.clone())?;
        prompt.provider = Some(self.provider.clone());
        for (name, media) in &literals {
            prompt.bind_media_mut(name, media)?;
        }
        prompt.media_kinds = media_kinds;
        prompt.authored = Some(Box::new(self.clone()));
        Ok(prompt)
    }

    /// Derives the native authored form of a runtime-built request.
    ///
    /// # Errors
    ///
    /// Returns [`SkaldError::PromptInvalid`] for a request kind no Prompt
    /// format carries, such as embeddings or a raw body.
    pub(crate) fn native(
        provider: ProviderName,
        model: &str,
        request: &ProviderRequest,
    ) -> SkaldResult<Self> {
        let (format, body) = match request {
            ProviderRequest::OpenAiChatCompletion(r) => (PromptFormat::ChatCompletion, to_body(r)?),
            ProviderRequest::OpenAiResponses(r) => (PromptFormat::Responses, to_body(r)?),
            ProviderRequest::AnthropicMessage(r) => (PromptFormat::Messages, to_body(r)?),
            ProviderRequest::GeminiGenerateContent(r) => {
                (PromptFormat::GenerateContent, to_body(r)?)
            }
            ProviderRequest::JevSystemOne(r) => (PromptFormat::SystemOne, to_body(r)?),
            _ => {
                return Err(invalid(
                    "format",
                    "this request kind is not a Prompt format",
                ));
            }
        };
        Ok(Self {
            provider,
            model: model.to_owned(),
            format: Some(format),
            body: Some(body),
            system: None,
            messages: None,
            settings: None,
            response_format: None,
        })
    }

    /// Decodes the native body through the typed owner for `format`.
    ///
    /// The authored `model` is inserted into the body (GenerateContent keeps
    /// it in the URL instead) and the result decodes into the matching
    /// [`ProviderRequest`] variant; unknown native options survive decoding.
    ///
    /// # Errors
    ///
    /// Returns [`SkaldError::PromptInvalid`] for a portable field beside the
    /// body, a provider/format pair [`check_format`] refuses, `body.model`,
    /// or a body that does not decode as `format`.
    fn native_request(
        &self,
        format: PromptFormat,
        body: &Map<String, Value>,
    ) -> SkaldResult<ProviderRequest> {
        if let Some(field) = [
            ("system", self.system.is_some()),
            ("messages", self.messages.is_some()),
            ("settings", self.settings.is_some()),
            ("response_format", self.response_format.is_some()),
        ]
        .into_iter()
        .find_map(|(field, set)| set.then_some(field))
        {
            return Err(invalid(
                field,
                "portable fields cannot accompany a native `body`",
            ));
        }
        check_format(&self.provider, format)?;
        if body.contains_key("model") {
            return Err(invalid(
                "body.model",
                "the model is `model`; remove it from the native body",
            ));
        }
        let mut body = body.clone();
        if format != PromptFormat::GenerateContent {
            body.insert("model".to_owned(), Value::String(self.model.clone()));
        }
        let body = Value::Object(body);
        let decoded = match format {
            PromptFormat::ChatCompletion => {
                serde_json::from_value(body).map(ProviderRequest::OpenAiChatCompletion)
            }
            PromptFormat::Responses => {
                serde_json::from_value(body).map(ProviderRequest::OpenAiResponses)
            }
            PromptFormat::Messages => {
                serde_json::from_value(body).map(ProviderRequest::AnthropicMessage)
            }
            PromptFormat::GenerateContent => {
                serde_json::from_value(body).map(ProviderRequest::GeminiGenerateContent)
            }
            PromptFormat::SystemOne => {
                serde_json::from_value(body).map(ProviderRequest::JevSystemOne)
            }
        };
        decoded.map_err(|error| invalid("body", &error.to_string()))
    }

    /// Lowers portable content to the provider's default format.
    ///
    /// Literal media sources are recorded in `literals` under generated
    /// placeholder names for [`compile`](Self::compile) to bind, and each
    /// binding's block kind in `media_kinds`.
    ///
    /// # Errors
    ///
    /// Returns [`SkaldError::PromptInvalid`] for a jev provider, an invalid
    /// output schema, a turn [`PortableMessage::lower`] refuses, no nonempty
    /// content, or a setting the destination refuses.
    fn portable_request(
        &self,
        literals: &mut Vec<(String, MediaRef)>,
        media_kinds: &mut Vec<(String, MediaKind)>,
    ) -> SkaldResult<ProviderRequest> {
        let format = match &self.provider {
            ProviderName::OpenAi | ProviderName::Custom(_) => PromptFormat::ChatCompletion,
            ProviderName::Anthropic => PromptFormat::Messages,
            ProviderName::Google | ProviderName::Vertex => PromptFormat::GenerateContent,
            ProviderName::Jev => {
                return Err(invalid(
                    "format",
                    "jev Prompts are native only; declare `format: system_one` and a `body`",
                ));
            }
        };
        if let Some(schema) = &self.response_format {
            check_output_schema(
                schema,
                "response_format",
                format == PromptFormat::ChatCompletion,
            )?;
        }
        let turns = self
            .messages
            .iter()
            .flatten()
            .map(|message| message.lower(literals, media_kinds))
            .collect::<SkaldResult<Vec<_>>>()?;
        let has_content = self.system.as_deref().is_some_and(|text| !text.is_empty())
            || turns
                .iter()
                .any(|turn| turn.parts.iter().any(|part| !part.is_empty()));
        if !has_content {
            return Err(invalid(
                "messages",
                "a portable Prompt needs nonempty system or message content",
            ));
        }
        let settings = self.settings.clone().unwrap_or_default();
        let schema = self.response_format.clone();
        let system = self.system.clone();
        Ok(match format {
            PromptFormat::Messages => ProviderRequest::AnthropicMessage(anthropic(
                &self.model,
                system,
                turns,
                &settings,
                schema,
            )?),
            PromptFormat::GenerateContent => {
                ProviderRequest::GeminiGenerateContent(google(system, turns, &settings, schema)?)
            }
            _ => ProviderRequest::OpenAiChatCompletion(openai_chat(
                &self.model,
                system,
                turns,
                &settings,
                schema,
            )?),
        })
    }
}

impl PortableMessage {
    /// Lowers this turn to ordered text parts, recording literal media and
    /// each binding's block kind.
    ///
    /// # Errors
    ///
    /// Returns [`SkaldError::PromptInvalid`] for media in an assistant turn,
    /// a binding that is not exactly one `${media:name}` placeholder, or one
    /// binding name declared as both image and document.
    fn lower(
        &self,
        literals: &mut Vec<(String, MediaRef)>,
        media_kinds: &mut Vec<(String, MediaKind)>,
    ) -> SkaldResult<Turn> {
        let blocks = match &self.content {
            PortableContent::Text(text) => {
                return Ok(Turn {
                    role: self.role,
                    parts: vec![text.clone()],
                    plain: true,
                });
            }
            PortableContent::Blocks(blocks) => blocks,
        };
        let mut parts = Vec::with_capacity(blocks.len());
        for block in blocks {
            let (kind, source) = match block {
                ContentBlock::Text { text } => {
                    parts.push(text.clone());
                    continue;
                }
                ContentBlock::Image { source } => (MediaKind::Image, source),
                ContentBlock::Document { source } => (MediaKind::Document, source),
            };
            if self.role == PortableRole::Assistant {
                return Err(invalid(
                    "messages.content",
                    "image and document blocks belong in user turns",
                ));
            }
            parts.push(match source {
                BlockSource::Binding(binding) => {
                    let Some(name) = media_placeholder_regex()
                        .captures(binding)
                        .filter(|found| &found[0] == binding)
                        .map(|found| found[1].to_owned())
                    else {
                        return Err(invalid(
                            "messages.content.source",
                            "a media binding is exactly one `${media:name}` placeholder",
                        ));
                    };
                    match media_kinds.iter().find(|(declared, _)| *declared == name) {
                        Some((_, declared)) if *declared != kind => {
                            return Err(invalid(
                                "messages.content.source",
                                &format!(
                                    "media binding `{name}` is declared as both image and document"
                                ),
                            ));
                        }
                        Some(_) => {}
                        None => media_kinds.push((name, kind)),
                    }
                    binding.clone()
                }
                BlockSource::Media(source) => {
                    let name = format!("__wyrd_media_{}", literals.len());
                    let placeholder = format!("${{media:{name}}}");
                    literals.push((
                        name,
                        MediaRef {
                            kind,
                            source: source.clone(),
                        },
                    ));
                    placeholder
                }
            });
        }
        Ok(Turn {
            role: self.role,
            parts,
            plain: false,
        })
    }
}

/// Refuses a provider/format pair the provider does not speak.
///
/// Custom providers pass; their capabilities are validated at use.
///
/// # Errors
///
/// Returns [`SkaldError::PromptInvalid`] for field `format` when a built-in
/// provider does not accept `format`.
fn check_format(provider: &ProviderName, format: PromptFormat) -> SkaldResult<()> {
    let admitted = match provider {
        ProviderName::OpenAi => {
            matches!(
                format,
                PromptFormat::ChatCompletion | PromptFormat::Responses
            )
        }
        ProviderName::Anthropic => format == PromptFormat::Messages,
        ProviderName::Google | ProviderName::Vertex => format == PromptFormat::GenerateContent,
        ProviderName::Jev => format == PromptFormat::SystemOne,
        // Custom capabilities are validated at use by the registered provider.
        ProviderName::Custom(_) => true,
    };
    if admitted {
        Ok(())
    } else {
        Err(invalid(
            "format",
            &format!(
                "provider {} does not accept format {}",
                provider,
                format_label(format)
            ),
        ))
    }
}

/// Builds the portable OpenAI Chat Completions request.
///
/// System text becomes the first message, plain turns stay text, block turns
/// become ordered text parts, and a schema becomes strict `json_schema`
/// output named [`RESPONSE_SCHEMA_NAME`].
///
/// # Errors
///
/// Returns [`SkaldError::PromptInvalid`] for `top_k`, more than four stop
/// sequences, or an out-of-range sampling setting.
fn openai_chat(
    model: &str,
    system: Option<String>,
    turns: Vec<Turn>,
    settings: &PortableSettings,
    schema: Option<Map<String, Value>>,
) -> SkaldResult<OpenAiChatRequest> {
    let destination = "chat_completion";
    refuse(destination, "top_k", settings.top_k.is_some())?;
    let stop = stops(destination, settings.stop_sequences.as_ref(), 4)?;
    let mut messages = Vec::with_capacity(turns.len() + 1);
    if let Some(text) = system {
        messages.push(OpenAiChatMessage {
            role: "system".to_owned(),
            content: Some(OpenAiMessageContent::Text(text)),
            ..Default::default()
        });
    }
    messages.extend(turns.into_iter().map(|turn| {
        let content = if turn.plain {
            OpenAiMessageContent::Text(turn.parts.concat())
        } else {
            OpenAiMessageContent::Parts(
                turn.parts
                    .into_iter()
                    .map(|text| OpenAiContentPart::Text { text })
                    .collect(),
            )
        };
        OpenAiChatMessage {
            role: role_name(turn.role, "assistant").to_owned(),
            content: Some(content),
            ..Default::default()
        }
    }));
    Ok(OpenAiChatRequest {
        model: model.to_owned(),
        messages,
        response_format: schema.map(|schema| OpenAiResponseFormat::JsonSchema {
            json_schema: OpenAiJsonSchema {
                name: RESPONSE_SCHEMA_NAME.to_owned(),
                description: None,
                schema: Some(schema),
                strict: Some(true),
            },
        }),
        stream: None,
        stream_options: None,
        tools: None,
        tool_choice: None,
        parallel_tool_calls: None,
        settings: OpenAiChatSettings {
            temperature: within(destination, "temperature", settings.temperature, 0.0, 2.0)?,
            top_p: within(destination, "top_p", settings.top_p, 0.0, 1.0)?,
            max_completion_tokens: positive(
                destination,
                "max_output_tokens",
                settings.max_output_tokens,
            )?,
            stop: stop.map(OpenAiStop::Many),
            seed: settings.seed,
            presence_penalty: within(
                destination,
                "presence_penalty",
                settings.presence_penalty,
                -2.0,
                2.0,
            )?,
            frequency_penalty: within(
                destination,
                "frequency_penalty",
                settings.frequency_penalty,
                -2.0,
                2.0,
            )?,
            ..Default::default()
        },
    })
}

/// Builds the portable Anthropic Messages request.
///
/// Every turn becomes ordered text blocks and a schema becomes
/// `output_config` JSON-schema output.
///
/// # Errors
///
/// Returns [`SkaldError::PromptInvalid`] for `seed`, a presence or frequency
/// penalty, a missing or zero `max_output_tokens`, or an out-of-range
/// sampling setting.
fn anthropic(
    model: &str,
    system: Option<String>,
    turns: Vec<Turn>,
    settings: &PortableSettings,
    schema: Option<Map<String, Value>>,
) -> SkaldResult<AnthropicMessagesRequest> {
    let destination = "messages";
    refuse(destination, "seed", settings.seed.is_some())?;
    refuse(
        destination,
        "presence_penalty",
        settings.presence_penalty.is_some(),
    )?;
    refuse(
        destination,
        "frequency_penalty",
        settings.frequency_penalty.is_some(),
    )?;
    let max_tokens = positive(destination, "max_output_tokens", settings.max_output_tokens)?
        .ok_or_else(|| {
            invalid(
                "settings.max_output_tokens",
                "anthropic requires an explicit max_output_tokens",
            )
        })?;
    Ok(AnthropicMessagesRequest {
        model: model.to_owned(),
        messages: turns
            .into_iter()
            .map(|turn| AnthropicMessage {
                role: role_name(turn.role, "assistant").to_owned(),
                content: turn
                    .parts
                    .into_iter()
                    .map(|text| AnthropicContentBlock::Text {
                        text,
                        cache_control: None,
                        citations: None,
                    })
                    .collect(),
            })
            .collect(),
        system: system.map(AnthropicSystem::Text),
        stream: None,
        tools: None,
        tool_choice: None,
        output_config: schema.map(|schema| AnthropicOutputConfig {
            format: AnthropicOutputFormat::JsonSchema {
                schema: Value::Object(schema),
            },
        }),
        settings: AnthropicMessagesSettings {
            max_tokens,
            temperature: within(destination, "temperature", settings.temperature, 0.0, 1.0)?,
            top_p: within(destination, "top_p", settings.top_p, 0.0, 1.0)?,
            top_k: positive(destination, "top_k", settings.top_k)?,
            stop_sequences: stops(destination, settings.stop_sequences.as_ref(), usize::MAX)?,
            ..Default::default()
        },
    })
}

/// Builds the portable Google/Vertex GenerateContent request.
///
/// Assistant turns take the `model` role, system text becomes
/// `system_instruction`, and a schema becomes `response_schema` with a JSON
/// response MIME type. An all-default generation config is omitted.
///
/// # Errors
///
/// Returns [`SkaldError::PromptInvalid`] for a seed outside 32 bits, more
/// than five stop sequences, or an out-of-range sampling setting.
fn google(
    system: Option<String>,
    turns: Vec<Turn>,
    settings: &PortableSettings,
    schema: Option<Map<String, Value>>,
) -> SkaldResult<GoogleGenerateContentRequest> {
    let destination = "generate_content";
    if let Some(seed) = settings.seed
        && i32::try_from(seed).is_err()
    {
        return Err(invalid(
            "settings.seed",
            "generate_content seeds are 32-bit integers",
        ));
    }
    let text_parts = |parts: Vec<String>| {
        parts
            .into_iter()
            .map(|text| GooglePart::Text { text })
            .collect()
    };
    let config = GoogleGenerationConfig {
        temperature: within(destination, "temperature", settings.temperature, 0.0, 2.0)?,
        top_p: within(destination, "top_p", settings.top_p, 0.0, 1.0)?,
        top_k: positive(destination, "top_k", settings.top_k)?,
        max_output_tokens: positive(destination, "max_output_tokens", settings.max_output_tokens)?,
        stop_sequences: stops(destination, settings.stop_sequences.as_ref(), 5)?,
        seed: settings.seed,
        presence_penalty: within(
            destination,
            "presence_penalty",
            settings.presence_penalty,
            -2.0,
            2.0,
        )?,
        frequency_penalty: within(
            destination,
            "frequency_penalty",
            settings.frequency_penalty,
            -2.0,
            2.0,
        )?,
        response_mime_type: schema.as_ref().map(|_| "application/json".to_owned()),
        response_schema: schema.map(Value::Object),
        ..Default::default()
    };
    Ok(GoogleGenerateContentRequest {
        contents: turns
            .into_iter()
            .map(|turn| GoogleContent {
                role: role_name(turn.role, "model").to_owned(),
                parts: text_parts(turn.parts),
            })
            .collect(),
        system_instruction: system.map(|text| GoogleContent {
            role: "user".to_owned(),
            parts: vec![GooglePart::Text { text }],
        }),
        tools: None,
        tool_config: None,
        settings: GoogleGenerateSettings {
            generation_config: (config != GoogleGenerationConfig::default()).then_some(config),
            ..Default::default()
        },
    })
}

/// Wire role for a portable turn, given the protocol's assistant role name.
const fn role_name(role: PortableRole, assistant: &'static str) -> &'static str {
    match role {
        PortableRole::User => "user",
        PortableRole::Assistant => assistant,
    }
}

/// Refuses a setting the destination protocol cannot carry.
///
/// # Errors
///
/// Returns [`SkaldError::PromptInvalid`] for `settings.<setting>` when
/// `present` is true.
fn refuse(destination: &str, setting: &str, present: bool) -> SkaldResult<()> {
    if present {
        Err(invalid(
            &format!("settings.{setting}"),
            &format!("{destination} does not support {setting}"),
        ))
    } else {
        Ok(())
    }
}

/// Passes a finite setting within `[low, high]`, narrowed to the wire's `f32`.
///
/// # Errors
///
/// Returns [`SkaldError::PromptInvalid`] for `settings.<setting>` when the
/// value is not finite or lies outside the range.
#[expect(
    clippy::cast_possible_truncation,
    reason = "the value is range-checked to small protocol bounds before narrowing"
)]
fn within(
    destination: &str,
    setting: &str,
    value: Option<f64>,
    low: f64,
    high: f64,
) -> SkaldResult<Option<f32>> {
    match value {
        Some(value) if !value.is_finite() || value < low || value > high => Err(invalid(
            &format!("settings.{setting}"),
            &format!("{destination} accepts {setting} in [{low}, {high}]"),
        )),
        value => Ok(value.map(|value| value as f32)),
    }
}

/// Passes a positive integer setting.
///
/// # Errors
///
/// Returns [`SkaldError::PromptInvalid`] for `settings.<setting>` when the
/// value is zero.
fn positive(destination: &str, setting: &str, value: Option<u32>) -> SkaldResult<Option<u32>> {
    if value == Some(0) {
        return Err(invalid(
            &format!("settings.{setting}"),
            &format!("{destination} requires a positive {setting}"),
        ));
    }
    Ok(value)
}

/// Passes a nonempty list of at most `limit` nonempty stop strings.
///
/// # Errors
///
/// Returns [`SkaldError::PromptInvalid`] for `settings.stop_sequences` when
/// the list is empty, longer than `limit`, or holds an empty string.
fn stops(
    destination: &str,
    value: Option<&Vec<String>>,
    limit: usize,
) -> SkaldResult<Option<Vec<String>>> {
    match value {
        Some(list)
            if list.is_empty() || list.len() > limit || list.iter().any(String::is_empty) =>
        {
            Err(invalid(
                "settings.stop_sequences",
                &format!("{destination} accepts 1 to {limit} nonempty stop sequences"),
            ))
        }
        value => Ok(value.cloned()),
    }
}

/// Serializes a typed request as a native body without its model.
///
/// # Errors
///
/// Returns a serialization error when the request cannot be serialized and
/// [`SkaldError::PromptInvalid`] for field `body` when it is not a JSON
/// object.
fn to_body(request: &impl Serialize) -> SkaldResult<Map<String, Value>> {
    let Value::Object(mut body) = serde_json::to_value(request).map_err(SkaldError::serialize)?
    else {
        return Err(invalid("body", "a native body is a JSON object"));
    };
    body.remove("model");
    Ok(body)
}

/// Authored spelling of a format for messages.
fn format_label(format: PromptFormat) -> String {
    match serde_json::to_value(format) {
        Ok(Value::String(name)) => name,
        _ => format!("{format:?}"),
    }
}

/// Builds the field-specific authoring refusal shared by the compiler and the
/// Prompt construction checks.
pub(crate) fn invalid(field: &str, message: &str) -> SkaldError {
    SkaldError::PromptInvalid {
        field: field.to_owned(),
        message: message.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    //! Authored Prompt compilation: portable and native forms compile to the
    //! request, model, variables, and response type they describe, round-trip
    //! as authored, and refuse invalid fields at their authored path.

    use serde_json::{Value, json};

    use crate::authoring::AuthoredPrompt;
    use crate::{ProviderName, ProviderRequest, ResponseType, SkaldError, SkaldResult};

    /// Decodes and compiles one authored spec body; decode failures report
    /// the field `decode`.
    ///
    /// # Errors
    ///
    /// Returns [`SkaldError::PromptInvalid`] with field `decode` when the body
    /// is not an [`AuthoredPrompt`], and the compiler's own refusal otherwise.
    fn compile(body: Value) -> SkaldResult<crate::Prompt> {
        serde_json::from_value::<AuthoredPrompt>(body)
            .map_err(|error| SkaldError::PromptInvalid {
                field: "decode".to_owned(),
                message: error.to_string(),
            })?
            .compile()
    }

    /// Returns the refused field of an authored body that must not compile.
    ///
    /// # Panics
    ///
    /// Panics when the body compiles or fails with anything other than a
    /// field refusal.
    fn refused(body: Value) -> String {
        match compile(body) {
            Err(SkaldError::PromptInvalid { field, .. }) => field,
            other => panic!("expected a field refusal, got {other:?}"),
        }
    }

    /// Returns the provider-native body of a compiled Prompt.
    ///
    /// # Panics
    ///
    /// Panics when the request cannot be serialized.
    fn body(prompt: &crate::Prompt) -> Value {
        serde_json::to_value(&prompt.request).expect("request json")["body"].clone()
    }

    /// Proves portable text, image, and document blocks compile in order for
    /// every provider format; the eight settings map or refuse per the table;
    /// structured output takes the deterministic schema name; media bindings,
    /// slash-containing models, and invalid shapes behave as specified.
    ///
    /// # Panics
    ///
    /// Panics when any compile, refusal, or body assertion fails.
    #[test]
    fn portable_content_settings_and_schema() {
        let schema = json!({"type": "object", "properties": {"answer": {"type": "string"}}, "required": ["answer"]});
        let document =
            json!({"type": "base64", "mime_type": "application/pdf", "data": "JVBERi0="});
        let authored = |provider: &str, document: &Value| {
            json!({
                "provider": provider,
                "model": "org/model-1",
                "system": "Be brief.",
                "messages": [
                    {"role": "user", "content": [
                        {"type": "text", "text": "Compare {{question}}"},
                        {"type": "image", "source": "${media:photo}"},
                        {"type": "document", "source": document},
                    ]},
                    {"role": "assistant", "content": "ok"},
                ],
                "settings": {"temperature": 0.2, "top_p": 0.9, "max_output_tokens": 64, "stop_sequences": ["END"]},
                "response_format": schema,
            })
        };

        let openai = compile(authored("openai", &document)).expect("openai portable compiles");
        assert_eq!(openai.model, "org/model-1", "a slash model is preserved");
        assert_eq!(openai.variables, vec!["question"]);
        assert_eq!(openai.media_variables, vec!["photo"]);
        assert_eq!(
            openai.response_type,
            ResponseType::JsonSchema {
                name: "response".to_owned(),
                schema: schema.clone()
            }
        );
        assert!(matches!(
            openai.request,
            ProviderRequest::OpenAiChatCompletion(_)
        ));
        let chat = body(&openai);
        assert_eq!(
            chat["messages"][0],
            json!({"role": "system", "content": "Be brief."})
        );
        let parts = chat["messages"][1]["content"].as_array().expect("parts");
        assert_eq!(
            parts[0],
            json!({"type": "text", "text": "Compare {{question}}"})
        );
        assert_eq!(parts[1], json!({"type": "text", "text": "${media:photo}"}));
        assert_eq!(
            parts[2]["type"], "file",
            "the literal document is bound in place"
        );
        assert_eq!(
            chat["messages"][2],
            json!({"role": "assistant", "content": "ok"})
        );
        assert_eq!(chat["max_completion_tokens"], 64);
        assert_eq!(chat["stop"], json!(["END"]));
        assert_eq!(chat["response_format"]["json_schema"]["name"], "response");
        assert_eq!(chat["response_format"]["json_schema"]["strict"], true);

        let claude = body(&compile(authored("anthropic", &document)).expect("anthropic compiles"));
        assert_eq!(claude["system"], "Be brief.");
        assert_eq!(claude["max_tokens"], 64);
        assert_eq!(claude["stop_sequences"], json!(["END"]));
        assert_eq!(claude["output_config"]["format"]["schema"], schema);
        assert_eq!(claude["messages"][1]["role"], "assistant");
        let blocks = claude["messages"][0]["content"].as_array().expect("blocks");
        assert_eq!(
            blocks.len(),
            3,
            "text, binding, and literal document stay in order"
        );
        assert_eq!(blocks[2]["type"], "document");

        let gemini = body(&compile(authored("google", &document)).expect("google compiles"));
        assert_eq!(
            gemini["system_instruction"]["parts"][0]["text"],
            "Be brief."
        );
        assert_eq!(gemini["contents"][1]["role"], "model");
        assert_eq!(gemini["generation_config"]["max_output_tokens"], 64);
        assert_eq!(
            gemini["generation_config"]["response_mime_type"],
            "application/json"
        );
        assert_eq!(gemini["generation_config"]["response_schema"], schema);
        let vertex = compile(authored("vertex", &document)).expect("vertex compiles");
        assert_eq!(vertex.provider(), ProviderName::Vertex);

        // Existing per-provider source rules apply to literal media.
        let url = json!({"type": "url", "url": "https://example.com/a.pdf"});
        assert!(
            compile(authored("google", &url)).is_err(),
            "google refuses a plain url document"
        );

        // A setting outside the destination is refused, never dropped.
        let with = |provider: &str, settings: Value| json!({"provider": provider, "model": "m", "messages": [{"role": "user", "content": "hi"}], "settings": settings});
        assert_eq!(
            refused(with("openai", json!({"top_k": 3}))),
            "settings.top_k"
        );
        assert_eq!(
            refused(with(
                "anthropic",
                json!({"max_output_tokens": 8, "seed": 1})
            )),
            "settings.seed"
        );
        assert_eq!(
            refused(with("anthropic", json!({"temperature": 0.1}))),
            "settings.max_output_tokens"
        );
        assert_eq!(
            refused(with("openai", json!({"temperature": 2.5}))),
            "settings.temperature"
        );
        assert_eq!(
            refused(with("openai", json!({"top_p": 1.5}))),
            "settings.top_p"
        );
        assert_eq!(
            refused(with("google", json!({"seed": 4_294_967_296_i64}))),
            "settings.seed"
        );
        assert_eq!(
            refused(with("openai", json!({"stop_sequences": []}))),
            "settings.stop_sequences"
        );
        assert_eq!(refused(with("openai", json!({"temp": 1.0}))), "decode");
        let all = json!({"temperature": 1.0, "top_p": 0.5, "max_output_tokens": 9, "top_k": 4,
                         "stop_sequences": ["x"], "seed": 7, "presence_penalty": 0.5, "frequency_penalty": -0.5});
        let config = body(&compile(with("google", all)).expect("google carries all eight"))
            ["generation_config"]
            .clone();
        for field in [
            "temperature",
            "top_p",
            "max_output_tokens",
            "top_k",
            "stop_sequences",
            "seed",
            "presence_penalty",
            "frequency_penalty",
        ] {
            assert!(
                !config[field].is_null(),
                "{field} maps to generation config"
            );
        }

        // Shape refusals.
        let plain = |extra: Value| {
            let mut authored = json!({"provider": "openai", "model": "m", "messages": [{"role": "user", "content": "hi"}]});
            if let (Some(authored), Value::Object(extra)) = (authored.as_object_mut(), extra) {
                authored.extend(extra);
            }
            authored
        };
        assert_eq!(refused(plain(json!({"model": " "}))), "model");
        assert_eq!(
            refused(plain(
                json!({"messages": [{"role": "user", "content": ""}]})
            )),
            "messages"
        );
        assert_eq!(refused(plain(json!({"provider": "jev"}))), "format");
        assert_eq!(
            refused(plain(
                json!({"messages": [{"role": "user", "content": [{"type": "image", "source": "see ${media:x}"}]}]})
            )),
            "messages.content.source"
        );
        assert_eq!(
            refused(plain(
                json!({"messages": [{"role": "assistant", "content": [{"type": "image", "source": "${media:x}"}]}]})
            )),
            "messages.content"
        );
        assert_eq!(
            refused(plain(
                json!({"messages": [{"role": "system", "content": "x"}]})
            )),
            "decode"
        );
        assert_eq!(refused(plain(json!({"variables": ["x"]}))), "decode");
    }

    /// Proves the model is literal configuration in both forms: placeholder
    /// grammars and control characters are refused before variable
    /// derivation, so rendering can never rewrite the target, while content
    /// variables still render and a slash model stays exact.
    ///
    /// # Panics
    ///
    /// Panics when any compile, refusal, or body assertion fails.
    #[test]
    fn model_is_literal_configuration() {
        let portable = |model: &str| json!({"provider": "openai", "model": model, "messages": [{"role": "user", "content": "{{question}}"}]});
        let native = |model: &str| {
            json!({"provider": "anthropic", "model": model, "format": "messages",
                                          "body": {"max_tokens": 8, "messages": [{"role": "user", "content": [{"type": "text", "text": "{{question}}"}]}]}})
        };
        for model in [
            "{{target}}",
            "${target}",
            "org/{{target}}",
            "${media:x}",
            "gpt\n4o",
            "gpt\u{0}",
        ] {
            assert_eq!(refused(portable(model)), "model", "portable {model:?}");
            assert_eq!(refused(native(model)), "model", "native {model:?}");
        }
        assert!(matches!(
            crate::Prompt::new(crate::ProviderRequest::OpenAiChatCompletion(crate::common::openai_chat_request()), "${target}"),
            Err(SkaldError::PromptInvalid { field, .. }) if field == "model"
        ));

        for body in [portable("org/model-1"), native("org/model-1")] {
            let prompt = compile(body).expect("a slash literal compiles");
            assert_eq!(prompt.variables, vec!["question"]);
            let rendered = prompt
                .render(&[("question", "why?"), ("target", "evil")])
                .expect("content variables render");
            let rendered = serde_json::to_value(&rendered).expect("request json");
            assert_eq!(rendered["body"]["model"], "org/model-1");
        }
    }

    /// Proves output schemas are validated before compilation in both forms:
    /// invalid type names, non-string `required`, and non-object
    /// `properties` are refused at their exact field, at any depth;
    /// strict OpenAI output refuses a non-object root that Anthropic,
    /// GenerateContent, and non-strict native OpenAI accept; and valid nested
    /// schemas, references, type unions, and native options keep their
    /// authored bytes.
    ///
    /// # Panics
    ///
    /// Panics when any compile, refusal, or body assertion fails.
    #[test]
    fn output_schemas_are_validated() {
        let portable = |provider: &str, schema: Value| {
            json!({"provider": provider, "model": "m", "messages": [{"role": "user", "content": "hi"}],
                                                               "settings": {"max_output_tokens": 8}, "response_format": schema})
        };
        let object = |properties: Value| json!({"type": "object", "properties": properties});
        for (schema, field) in [
            (
                object(json!({"a": {"type": "not-a-json-schema-type"}})),
                "response_format.properties.a.type",
            ),
            (
                object(json!({"a": {"type": ["string", 3]}})),
                "response_format.properties.a.type",
            ),
            (
                json!({"type": "object", "required": "a"}),
                "response_format.required",
            ),
            (
                json!({"type": "object", "required": ["a", 1]}),
                "response_format.required",
            ),
            (
                json!({"type": "object", "properties": []}),
                "response_format.properties",
            ),
            (
                object(json!({"a": {"type": "array", "items": 3}})),
                "response_format.properties.a.items",
            ),
            (
                object(json!({"a": {"anyOf": {"type": "string"}}})),
                "response_format.properties.a.anyOf",
            ),
        ] {
            for provider in ["openai", "anthropic", "google"] {
                assert_eq!(
                    refused(portable(provider, schema.clone())),
                    field,
                    "{provider} {schema}"
                );
            }
        }

        let nested = json!({
            "type": "object",
            "properties": {
                "items": {"type": "array", "items": {"$ref": "#/$defs/item"}},
                "note": {"type": ["string", "null"], "description": "free text"},
            },
            "required": ["items", "note"],
            "additionalProperties": false,
            "$defs": {"item": {"type": "object", "properties": {"id": {"type": "integer"}}, "required": ["id"]}},
        });
        for (provider, path) in [
            ("openai", "/response_format/json_schema/schema"),
            ("anthropic", "/output_config/format/schema"),
            ("google", "/generation_config/response_schema"),
        ] {
            let compiled =
                body(&compile(portable(provider, nested.clone())).expect("nested schema compiles"));
            assert_eq!(
                compiled.pointer(path),
                Some(&nested),
                "{provider} keeps the schema bytes"
            );
        }

        let list = json!({"type": "array", "items": {"type": "string"}});
        assert_eq!(
            refused(portable("openai", list.clone())),
            "response_format.type",
            "strict chat_completion output needs an object root"
        );
        for provider in ["anthropic", "google"] {
            compile(portable(provider, list.clone())).expect("a list root compiles off OpenAI");
        }
        let inline = object(json!({"ids": {"type": "array", "items": {"type": "integer"}}}));

        let native = |format: Value| {
            json!({"provider": "openai", "model": "gpt-4o", "format": "chat_completion",
            "body": {"messages": [{"role": "user", "content": "hi"}], "x_custom": 1,
                     "response_format": {"type": "json_schema", "json_schema": {"name": "answer", "strict": false, "schema": format}}}})
        };
        assert_eq!(
            refused(native(object(json!({"a": {"type": "text"}})))),
            "body.response_format.json_schema.schema.properties.a.type"
        );
        let prompt = compile(native(inline.clone())).expect("valid native schema compiles");
        assert_eq!(
            body(&prompt)["response_format"]["json_schema"]["name"],
            "answer"
        );
        assert_eq!(
            body(&prompt)["response_format"]["json_schema"]["strict"],
            false
        );
        assert_eq!(body(&prompt)["x_custom"], 1);
        compile(native(list)).expect("non-strict native output accepts a list root");
        assert_eq!(
            refused(
                json!({"provider": "vertex", "model": "gemini", "format": "generate_content",
                "body": {"contents": [{"role": "user", "parts": [{"text": "hi"}]}],
                         "generation_config": {"response_schema": object(json!({"a": {"required": [1]}}))}}})
            ),
            "body.generation_config.response_schema.properties.a.required"
        );
    }

    /// Proves a portable binding keeps its block's media kind: binding the
    /// other kind is refused in both directions before the request changes,
    /// one name declared as both kinds is refused, matching bindings keep
    /// their order, and native untyped placeholders accept either kind.
    ///
    /// # Panics
    ///
    /// Panics when any compile, refusal, or body assertion fails.
    #[test]
    fn portable_media_bindings_keep_their_kind() {
        use crate::MediaRef;

        let authored = |provider: &str, blocks: Value| {
            json!({"provider": provider, "model": "m",
            "settings": {"max_output_tokens": 8}, "messages": [{"role": "user", "content": blocks}]})
        };
        let image = MediaRef::image_base64("image/png", "iVBORw0KGgo=");
        let document = MediaRef::document_base64("application/pdf", "JVBERi0=");
        for provider in ["openai", "anthropic", "google"] {
            let blocks = json!([
                {"type": "image", "source": "${media:photo}"},
                {"type": "text", "text": "and"},
                {"type": "document", "source": "${media:report}"},
                {"type": "image", "source": "${media:photo}"},
            ]);
            let mut prompt = compile(authored(provider, blocks)).expect("portable media compiles");
            let before = prompt.request.clone();
            for (name, media) in [("photo", &document), ("report", &image)] {
                assert!(
                    matches!(
                        prompt.bind_media_mut(name, media),
                        Err(SkaldError::InvalidMediaType(_))
                    ),
                    "{provider}: {name} refuses the other kind"
                );
                assert_eq!(
                    prompt.request, before,
                    "{provider}: refusal leaves the request untouched"
                );
            }
            prompt
                .bind_media_mut("photo", &image)
                .expect("matching image binds");
            prompt
                .bind_media_mut("report", &document)
                .expect("matching document binds");
            let text = serde_json::to_string(&prompt.request).expect("request json");
            let (first, report, last) = (
                text.find("iVBORw0KGgo=").expect("image bound"),
                text.find("JVBERi0=").expect("document bound"),
                text.rfind("iVBORw0KGgo=").expect("image bound twice"),
            );
            assert!(
                first < report && report < last,
                "{provider}: order is retained"
            );

            let conflict = json!([
                {"type": "image", "source": "${media:x}"},
                {"type": "document", "source": "${media:x}"},
            ]);
            assert_eq!(
                refused(authored(provider, conflict)),
                "messages.content.source",
                "{provider}: one name cannot be both kinds"
            );
        }

        let mut native = compile(
            json!({"provider": "openai", "model": "m", "format": "chat_completion",
            "body": {"messages": [{"role": "user", "content": "see ${media:x}"}]}}),
        )
        .expect("native compiles");
        native
            .clone()
            .bind_media_mut("x", &image)
            .expect("untyped binds an image");
        native
            .bind_media_mut("x", &document)
            .expect("untyped binds a document");
    }

    /// Proves literal and bound media sources are validated before the
    /// request changes: malformed or over-budget base64, MIME types that do
    /// not match the kind or the provider, and unsupported Google file URIs
    /// are refused, while supported sources and provider file ids bind as
    /// written.
    ///
    /// # Panics
    ///
    /// Panics when any compile, refusal, or body assertion fails.
    #[test]
    fn media_sources_are_validated() {
        use base64::Engine;

        use crate::{MediaRef, MediaSource};

        let authored = |provider: &str, kind: &str, source: Value| {
            json!({"provider": provider, "model": "m",
            "settings": {"max_output_tokens": 8},
            "messages": [{"role": "user", "content": [{"type": kind, "source": source}]}]})
        };
        let inline =
            |mime: &str, data: &str| json!({"type": "base64", "mime_type": mime, "data": data});
        let file = |uri: &str, mime: &str| json!({"type": "file", "uri": uri, "mime_type": mime});
        let media_error = |result: SkaldResult<crate::Prompt>| match result {
            Err(
                error @ (SkaldError::InvalidMediaType(_)
                | SkaldError::MediaTooLarge { .. }
                | SkaldError::UnsupportedMediaForProvider { .. }),
            ) => error.code(),
            other => panic!("expected a media refusal, got {other:?}"),
        };
        let oversized = base64::engine::general_purpose::STANDARD.encode(vec![
            0_u8;
            usize::try_from(
                crate::media::MAX_MEDIA_FILE_BYTES
            )
            .expect(
                "budget fits"
            ) + 1
        ]);
        for provider in ["openai", "anthropic", "google"] {
            for (kind, source, code) in [
                (
                    "image",
                    inline("image/png", "%%%"),
                    "SKALD_SPEC_400_INVALID_MEDIA_TYPE",
                ),
                (
                    "image",
                    inline("application/pdf", "JVBERi0="),
                    "SKALD_SPEC_400_INVALID_MEDIA_TYPE",
                ),
                (
                    "document",
                    inline("image/png", "iVBORw0KGgo="),
                    "SKALD_SPEC_400_INVALID_MEDIA_TYPE",
                ),
                (
                    "document",
                    inline(" ", "JVBERi0="),
                    "SKALD_SPEC_400_INVALID_MEDIA_TYPE",
                ),
                (
                    "document",
                    inline("pdf", "JVBERi0="),
                    "SKALD_SPEC_400_INVALID_MEDIA_TYPE",
                ),
                (
                    "document",
                    inline("application/pdf", &oversized),
                    "SKALD_SPEC_400_MEDIA_TOO_LARGE",
                ),
            ] {
                assert_eq!(
                    media_error(compile(authored(provider, kind, source.clone()))),
                    code,
                    "{provider} {kind} {source:.80}"
                );
            }
        }
        for (provider, kind, source) in [
            ("openai", "image", inline("image/bmp", "QUJD")),
            ("anthropic", "image", inline("image/svg+xml", "QUJD")),
            ("anthropic", "document", inline("text/plain", "QUJD")),
            (
                "google",
                "document",
                file("https://example.com/a.pdf", "application/pdf"),
            ),
        ] {
            assert!(
                media_error(compile(authored(provider, kind, source.clone())))
                    .starts_with("SKALD_SPEC_400"),
                "{provider} refuses {kind} {source}"
            );
        }
        for (provider, kind, source, needle) in [
            ("google", "image", inline("image/heic", "QUJD"), "QUJD"),
            ("google", "document", inline("text/plain", "QUJD"), "QUJD"),
            (
                "openai",
                "document",
                inline("application/pdf", "JVBERi0="),
                "JVBERi0=",
            ),
            (
                "google",
                "document",
                file("gs://bucket/a.pdf", "application/pdf"),
                "gs://bucket/a.pdf",
            ),
            (
                "google",
                "document",
                file(
                    "https://generativelanguage.googleapis.com/v1beta/files/abc",
                    "application/pdf",
                ),
                "v1beta/files/abc",
            ),
            (
                "openai",
                "document",
                json!({"type": "file", "uri": "file-abc123"}),
                "file-abc123",
            ),
            (
                "anthropic",
                "image",
                json!({"type": "file", "uri": "file_011CN"}),
                "file_011CN",
            ),
        ] {
            let prompt =
                compile(authored(provider, kind, source)).expect("supported media compiles");
            assert!(
                body(&prompt).to_string().contains(needle),
                "{provider} keeps {needle}"
            );
        }

        let mut bound = compile(authored("anthropic", "image", json!("${media:photo}")))
            .expect("binding compiles");
        let before = bound.request.clone();
        let bad = MediaRef {
            kind: crate::MediaKind::Image,
            source: MediaSource::Base64 {
                mime_type: "image/png".to_owned(),
                data: "not base64!".to_owned(),
            },
        };
        assert!(matches!(
            bound.bind_media_mut("photo", &bad),
            Err(SkaldError::InvalidMediaType(_))
        ));
        assert_eq!(
            bound.request, before,
            "a refused source leaves the request untouched"
        );
    }

    /// Proves the native form: model inserted at compile and refused in the
    /// body, provider/format pairs validated, portable fields refused beside a
    /// body, unknown native options preserved, and the authored form
    /// round-tripping through the runtime Prompt unchanged.
    ///
    /// # Panics
    ///
    /// Panics when any compile, refusal, or body assertion fails.
    #[test]
    fn native_body_pairs_and_round_trip() {
        let native = json!({
            "provider": "openai",
            "model": "gpt-4o",
            "format": "chat_completion",
            "body": {"messages": [{"role": "user", "content": "{{question}}"}], "temperature": 0.2, "x_custom": 1},
        });
        let prompt = compile(native.clone()).expect("native compiles");
        assert_eq!(body(&prompt)["model"], "gpt-4o");
        assert_eq!(
            body(&prompt)["x_custom"],
            1,
            "unknown native options survive"
        );
        assert_eq!(prompt.variables, vec!["question"]);
        assert_eq!(serde_json::to_value(&prompt).expect("prompt json"), native);

        let with = |extra: Value| {
            let mut authored = native.clone();
            if let (Some(authored), Value::Object(extra)) = (authored.as_object_mut(), extra) {
                authored.extend(extra);
            }
            authored
        };
        assert_eq!(
            refused(with(json!({"body": {"model": "gpt-4o", "messages": []}}))),
            "body.model"
        );
        assert_eq!(refused(with(json!({"format": "messages"}))), "format");
        assert_eq!(
            refused(with(json!({"format": "open_ai_chat_completion"}))),
            "decode"
        );
        assert_eq!(refused(with(json!({"system": "x"}))), "system");
        assert_eq!(refused(with(json!({"format": null}))), "format");
        assert_eq!(
            refused(json!({"provider": "anthropic", "model": "m", "format": "messages"})),
            "body"
        );
        let google = compile(json!({
            "provider": "vertex", "model": "gemini-2.5-flash", "format": "generate_content",
            "body": {"contents": [{"role": "user", "parts": [{"text": "hi"}]}]},
        }))
        .expect("vertex native compiles");
        assert!(
            body(&google).get("model").is_none(),
            "generate_content carries model in the URL"
        );
        assert_eq!(google.provider(), ProviderName::Vertex);

        let portable = json!({"provider": "anthropic", "model": "claude", "messages": [{"role": "user", "content": "hi"}],
                              "settings": {"max_output_tokens": 5}});
        let prompt = compile(portable.clone()).expect("portable compiles");
        assert_eq!(
            serde_json::to_value(&prompt).expect("prompt json"),
            portable
        );

        let mut edited = prompt;
        edited.model = "claude-next".to_owned();
        let authored = serde_json::to_value(&edited).expect("edited prompt json");
        assert_eq!(
            authored["format"], "messages",
            "a mutated Prompt serializes natively"
        );
        assert_eq!(authored["model"], "claude-next");
    }
}
