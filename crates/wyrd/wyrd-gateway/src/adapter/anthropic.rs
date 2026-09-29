//! `OpenAI` Chat Completions translation to and from Anthropic Messages.

use serde_json::{Map, Value, json};
use skald_spec::wire::anthropic_messages::{
    AnthropicContentBlock, AnthropicImageSource, AnthropicMessage, AnthropicMessagesRequest,
    AnthropicMessagesResponse, AnthropicMessagesSettings, AnthropicOutputConfig,
    AnthropicOutputFormat, AnthropicStopReason, AnthropicSystem, AnthropicTool,
    AnthropicToolResultContent, AnthropicUsage,
};
use skald_spec::wire::openai_chat::{
    OpenAiChatChoice, OpenAiChatMessage, OpenAiChatRequest, OpenAiChatResponse, OpenAiChatSettings,
    OpenAiChatToolChoice, OpenAiContentPart, OpenAiFunction, OpenAiJsonSchema,
    OpenAiMessageContent, OpenAiPromptTokensDetails, OpenAiResponseFormat, OpenAiTool,
    OpenAiToolCall, OpenAiToolChoiceMode, OpenAiToolFunctionCall, OpenAiUsage,
};

use super::UnsupportedRequest;

/// Adapter name used in rejections.
const ADAPTER: &str = "anthropic";

/// Largest sampling temperature Anthropic accepts.
const MAX_TEMPERATURE: f32 = 1.0;

/// Translates a chat completion request into Anthropic Messages for `model`.
///
/// Leading system messages become `system`; consecutive same-role turns merge
/// because Anthropic requires alternation; tool results become user-side
/// `tool_result` blocks; `parallel_tool_calls: false` becomes
/// `disable_parallel_tool_use`; a JSON-schema response format becomes
/// `output_config`; `user` becomes `metadata.user_id`.
///
/// # Errors
///
/// Returns [`UnsupportedRequest`] for streamed tools or obfuscation, `n > 1`,
/// a temperature above 1, a missing output-token maximum (Anthropic requires
/// one), penalties,
/// seeds, log probabilities, audio or file parts, remote non-HTTP images,
/// custom or strict tools, allowed-tools choices, `json_object` output, and
/// any other member without an Anthropic equivalent.
pub(super) fn request(
    chat: OpenAiChatRequest,
    model: &str,
) -> Result<AnthropicMessagesRequest, UnsupportedRequest> {
    let OpenAiChatRequest {
        model: _,
        messages,
        response_format,
        stream,
        stream_options,
        tools,
        tool_choice,
        parallel_tool_calls,
        settings,
    } = chat;
    let OpenAiChatSettings {
        temperature,
        top_p,
        max_tokens,
        max_completion_tokens,
        n,
        stop,
        presence_penalty,
        frequency_penalty,
        seed,
        logit_bias,
        user,
        reasoning_effort,
        modalities,
        audio,
        prediction,
        prompt_cache_key,
        service_tier,
        safety_identifier,
        store,
        metadata,
        logprobs,
        top_logprobs,
        extra,
    } = settings;
    super::streamable(ADAPTER, stream, stream_options.as_ref(), tools.is_some())?;
    super::reject_present(
        ADAPTER,
        &[
            ("n", n.is_some_and(|n| n > 1)),
            ("presence_penalty", presence_penalty.is_some()),
            ("frequency_penalty", frequency_penalty.is_some()),
            ("seed", seed.is_some()),
            ("logit_bias", logit_bias.is_some()),
            ("reasoning_effort", reasoning_effort.is_some()),
            ("modalities", modalities.is_some()),
            ("audio", audio.is_some()),
            ("prediction", prediction.is_some()),
            ("prompt_cache_key", prompt_cache_key.is_some()),
            ("service_tier", service_tier.is_some()),
            ("safety_identifier", safety_identifier.is_some()),
            ("store", store == Some(true)),
            ("metadata", metadata.is_some()),
            ("logprobs", logprobs == Some(true)),
            ("top_logprobs", top_logprobs.is_some()),
            (
                "temperature",
                temperature.is_some_and(|t| t > MAX_TEMPERATURE),
            ),
        ],
    )?;
    super::reject_extra(ADAPTER, &extra)?;
    let max_tokens = max_completion_tokens.or(max_tokens).ok_or_else(|| {
        UnsupportedRequest::new(
            "max_completion_tokens",
            "is required by the anthropic adapter",
        )
    })?;
    let (system, offset) = super::system_prefix(&messages)?;
    Ok(AnthropicMessagesRequest {
        model: model.to_owned(),
        messages: conversation(&messages[offset..], offset)?,
        system: system.map(AnthropicSystem::Text),
        stream: stream.filter(|stream| *stream),
        tools: tools.map(translate_tools).transpose()?,
        tool_choice: choice(tool_choice, parallel_tool_calls)?,
        output_config: output(response_format)?,
        settings: AnthropicMessagesSettings {
            max_tokens,
            temperature,
            top_p,
            top_k: None,
            stop_sequences: super::stop_sequences(stop),
            metadata: user
                .map(|user| Map::from_iter([("user_id".to_owned(), Value::String(user))])),
            thinking: None,
            extra: Map::new(),
        },
    })
}

/// Translates the conversational messages that follow the system prefix.
///
/// # Errors
///
/// Returns [`UnsupportedRequest`] for an unknown role or unsupported content.
fn conversation(
    messages: &[OpenAiChatMessage],
    offset: usize,
) -> Result<Vec<AnthropicMessage>, UnsupportedRequest> {
    let mut translated: Vec<AnthropicMessage> = Vec::new();
    for (index, message) in messages.iter().enumerate() {
        let field = format!("messages[{}]", index + offset);
        super::plain_message(message, &field)?;
        let (role, blocks) = match message.role.as_str() {
            "user" => ("user", user_blocks(message.content.as_ref(), &field)?),
            "assistant" => ("assistant", assistant_blocks(message, &field)?),
            "tool" => ("user", vec![tool_result(message, &field)?]),
            _ => {
                return Err(UnsupportedRequest::new(
                    format!("{field}.role"),
                    "must be user, assistant, or tool",
                ));
            }
        };
        match translated.last_mut() {
            Some(last) if last.role == role => last.content.extend(blocks),
            _ => translated.push(AnthropicMessage {
                role: role.to_owned(),
                content: blocks,
            }),
        }
    }
    Ok(translated)
}

/// Text content block.
fn text(text: String) -> AnthropicContentBlock {
    AnthropicContentBlock::Text {
        text,
        cache_control: None,
        citations: None,
    }
}

/// Translates user content: text and HTTP or base64 `data:` images.
///
/// # Errors
///
/// Returns [`UnsupportedRequest`] for absent content, audio or file parts, a
/// detail hint, or an image URL that is neither HTTP(S) nor `data:`.
fn user_blocks(
    content: Option<&OpenAiMessageContent>,
    field: &str,
) -> Result<Vec<AnthropicContentBlock>, UnsupportedRequest> {
    match content {
        None => Err(UnsupportedRequest::new(
            format!("{field}.content"),
            "is required",
        )),
        Some(OpenAiMessageContent::Text(value)) => Ok(vec![text(value.clone())]),
        Some(OpenAiMessageContent::Parts(parts)) => parts
            .iter()
            .enumerate()
            .map(|(index, part)| {
                let field = format!("{field}.content[{index}]");
                match part {
                    OpenAiContentPart::Text { text: value } => Ok(text(value.clone())),
                    OpenAiContentPart::ImageUrl { image_url } => {
                        let source = match super::image(image_url, &field)? {
                            Some((media_type, data)) => {
                                AnthropicImageSource::Base64 { media_type, data }
                            }
                            None if image_url.url.starts_with("https://")
                                || image_url.url.starts_with("http://") =>
                            {
                                AnthropicImageSource::Url {
                                    url: image_url.url.clone(),
                                }
                            }
                            None => {
                                return Err(UnsupportedRequest::new(
                                    format!("{field}.image_url.url"),
                                    "must be an HTTP(S) or base64 data URL",
                                ));
                            }
                        };
                        Ok(AnthropicContentBlock::Image {
                            source,
                            cache_control: None,
                        })
                    }
                    OpenAiContentPart::InputAudio { .. } | OpenAiContentPart::File { .. } => {
                        Err(UnsupportedRequest::new(
                            field,
                            "audio and file parts are not supported by the anthropic adapter",
                        ))
                    }
                }
            })
            .collect(),
    }
}

/// Translates an assistant turn: its text, then its tool calls.
///
/// # Errors
///
/// Returns [`UnsupportedRequest`] for non-text content, invalid tool calls,
/// or a turn with neither.
fn assistant_blocks(
    message: &OpenAiChatMessage,
    field: &str,
) -> Result<Vec<AnthropicContentBlock>, UnsupportedRequest> {
    let mut blocks: Vec<_> = super::text_content(message.content.as_ref(), field)?
        .map(text)
        .into_iter()
        .collect();
    for call in message.tool_calls.iter().flatten() {
        blocks.push(AnthropicContentBlock::ToolUse {
            id: call.id.clone(),
            name: call.function.name.clone(),
            input: super::arguments(call, field)?,
        });
    }
    if blocks.is_empty() {
        return Err(UnsupportedRequest::new(
            format!("{field}.content"),
            "an assistant message needs content or tool calls",
        ));
    }
    Ok(blocks)
}

/// Translates a tool message into a `tool_result` block.
///
/// # Errors
///
/// Returns [`UnsupportedRequest`] for a missing `tool_call_id` or non-text
/// content.
fn tool_result(
    message: &OpenAiChatMessage,
    field: &str,
) -> Result<AnthropicContentBlock, UnsupportedRequest> {
    let tool_use_id = message
        .tool_call_id
        .clone()
        .ok_or_else(|| UnsupportedRequest::new(format!("{field}.tool_call_id"), "is required"))?;
    Ok(AnthropicContentBlock::ToolResult {
        tool_use_id,
        content: AnthropicToolResultContent::Text(
            super::text_content(message.content.as_ref(), field)?.unwrap_or_default(),
        ),
        is_error: None,
        cache_control: None,
    })
}

/// Translates function tools.
///
/// # Errors
///
/// Returns [`UnsupportedRequest`] for custom tools or strict functions.
fn translate_tools(tools: Vec<OpenAiTool>) -> Result<Vec<AnthropicTool>, UnsupportedRequest> {
    tools
        .into_iter()
        .enumerate()
        .map(|(index, tool)| match tool {
            OpenAiTool::Function {
                function:
                    OpenAiFunction {
                        name,
                        description,
                        parameters,
                        strict,
                    },
            } if strict != Some(true) => Ok(AnthropicTool {
                name,
                description,
                input_schema: super::parameters(parameters),
                cache_control: None,
                kind: None,
                display_width_px: None,
                display_height_px: None,
                display_number: None,
            }),
            _ => Err(UnsupportedRequest::new(
                format!("tools[{index}]"),
                "only non-strict function tools are supported by the anthropic adapter",
            )),
        })
        .collect()
}

/// Translates the tool choice and parallel tool-call switch.
///
/// # Errors
///
/// Returns [`UnsupportedRequest`] for allowed-tools and custom-tool choices.
fn choice(
    choice: Option<OpenAiChatToolChoice>,
    parallel: Option<bool>,
) -> Result<Option<Value>, UnsupportedRequest> {
    let mut translated = match choice {
        None => None,
        Some(OpenAiChatToolChoice::Mode(mode)) => Some(json!({"type": match mode {
            OpenAiToolChoiceMode::None => "none",
            OpenAiToolChoiceMode::Auto => "auto",
            OpenAiToolChoiceMode::Required => "any",
        }})),
        Some(OpenAiChatToolChoice::Function(named)) => {
            Some(json!({"type": "tool", "name": named.function.name}))
        }
        Some(OpenAiChatToolChoice::Allowed(_) | OpenAiChatToolChoice::Custom(_)) => {
            return Err(UnsupportedRequest::new(
                "tool_choice",
                "allowed-tools and custom-tool choices are not supported by the anthropic adapter",
            ));
        }
    };
    if parallel == Some(false) {
        translated.get_or_insert_with(|| json!({"type": "auto"}))["disable_parallel_tool_use"] =
            Value::Bool(true);
    }
    Ok(translated)
}

/// Translates a response format into `output_config`.
///
/// # Errors
///
/// Returns [`UnsupportedRequest`] for `json_object`, a schema-less or
/// described JSON schema.
fn output(
    format: Option<OpenAiResponseFormat>,
) -> Result<Option<AnthropicOutputConfig>, UnsupportedRequest> {
    match format {
        None | Some(OpenAiResponseFormat::Text) => Ok(None),
        Some(OpenAiResponseFormat::JsonSchema {
            json_schema:
                OpenAiJsonSchema {
                    schema: Some(schema),
                    description: None,
                    ..
                },
        }) => Ok(Some(AnthropicOutputConfig {
            format: AnthropicOutputFormat::JsonSchema {
                schema: Value::Object(schema),
            },
        })),
        Some(_) => Err(UnsupportedRequest::new(
            "response_format",
            "only a json_schema format with a schema and no description is supported by the anthropic adapter",
        )),
    }
}

/// Translates an Anthropic answer into a chat completion.
///
/// Text blocks join into the message content and `tool_use` blocks become
/// tool calls. Cache tokens count toward prompt tokens, with cache reads
/// reported as cached tokens.
///
/// Returns `None` for thinking, citation, or other blocks and for a pause or
/// missing stop reason, none of which a chat completion represents.
pub(super) fn response(
    native: AnthropicMessagesResponse,
    model: &str,
) -> Option<OpenAiChatResponse> {
    let mut content = String::new();
    let mut calls = Vec::new();
    for block in native.content {
        match block {
            AnthropicContentBlock::Text {
                text,
                citations: None,
                ..
            } => content.push_str(&text),
            AnthropicContentBlock::ToolUse { id, name, input } => calls.push(OpenAiToolCall {
                id,
                kind: "function".to_owned(),
                function: OpenAiToolFunctionCall {
                    name,
                    arguments: input.to_string(),
                },
            }),
            _ => return None,
        }
    }
    let finish = finish(&native.stop_reason?)?;
    Some(OpenAiChatResponse {
        id: native.id,
        object: "chat.completion".to_owned(),
        created: super::now(),
        model: model.to_owned(),
        choices: vec![OpenAiChatChoice {
            index: 0,
            message: OpenAiChatMessage {
                role: "assistant".to_owned(),
                content: (!content.is_empty()).then_some(OpenAiMessageContent::Text(content)),
                tool_calls: (!calls.is_empty()).then_some(calls),
                ..OpenAiChatMessage::default()
            },
            finish_reason: Some(finish.to_owned()),
            logprobs: None,
        }],
        usage: Some(chat_usage(&native.usage)),
        system_fingerprint: None,
        service_tier: None,
    })
}

/// Chat completion finish reason of an Anthropic stop reason, or `None` for a
/// pause, which a chat completion cannot represent.
pub(super) const fn finish(reason: &AnthropicStopReason) -> Option<&'static str> {
    match reason {
        AnthropicStopReason::EndTurn | AnthropicStopReason::StopSequence => Some("stop"),
        AnthropicStopReason::MaxTokens => Some("length"),
        AnthropicStopReason::ToolUse => Some("tool_calls"),
        AnthropicStopReason::Refusal => Some("content_filter"),
        AnthropicStopReason::PauseTurn => None,
    }
}

/// Chat completion usage of Anthropic token counts: cache tokens count toward
/// prompt tokens, with cache reads reported as cached tokens.
pub(super) const fn chat_usage(usage: &AnthropicUsage) -> OpenAiUsage {
    let prompt =
        usage.input_tokens + usage.cache_creation_input_tokens + usage.cache_read_input_tokens;
    OpenAiUsage {
        prompt_tokens: prompt,
        completion_tokens: usage.output_tokens,
        total_tokens: prompt + usage.output_tokens,
        prompt_tokens_details: Some(OpenAiPromptTokensDetails {
            audio_tokens: 0,
            cached_tokens: usage.cache_read_input_tokens,
        }),
        completion_tokens_details: None,
    }
}
