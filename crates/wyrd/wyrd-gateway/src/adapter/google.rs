//! `OpenAI` Chat Completions translation to and from Google `GenerateContent`,
//! shared by the Gemini and Vertex adapters.

use std::collections::HashMap;

use serde_json::{Value, json};
use skald_spec::wire::google_generate::{
    GoogleContent, GoogleFinishReason, GoogleFunctionCall, GoogleFunctionCallingConfig,
    GoogleFunctionDeclaration, GoogleFunctionResponse, GoogleGenerateContentRequest,
    GoogleGenerateContentResponse, GoogleGenerateSettings, GoogleGenerationConfig,
    GoogleInlineData, GooglePart, GoogleTool, GoogleToolConfig, GoogleUsageMetadata,
};
use skald_spec::wire::openai_chat::{
    OpenAiChatChoice, OpenAiChatMessage, OpenAiChatRequest, OpenAiChatResponse, OpenAiChatSettings,
    OpenAiChatToolChoice, OpenAiCompletionTokensDetails, OpenAiContentPart, OpenAiFunction,
    OpenAiJsonSchema, OpenAiMessageContent, OpenAiPromptTokensDetails, OpenAiResponseFormat,
    OpenAiTool, OpenAiToolCall, OpenAiToolChoiceMode, OpenAiToolFunctionCall, OpenAiUsage,
};

use super::UnsupportedRequest;

/// Adapter name used in rejections.
const ADAPTER: &str = "google";

/// Translates a chat completion request into `GenerateContent`.
///
/// Leading system messages become `system_instruction`; assistant turns use
/// the `model` role; consecutive same-role turns merge; tool results become
/// `function_response` parts named by the function of the matching earlier
/// call, carrying a JSON-object result as-is or text as `{"content": ...}`.
/// `n`, penalties, seed, stop sequences, and JSON output map onto the
/// generation config.
///
/// # Errors
///
/// Returns [`UnsupportedRequest`] for streamed tools or obfuscation, `user`,
/// log probabilities,
/// remote image URLs (Google needs inline or uploaded media), audio or file
/// parts, custom or strict tools, allowed-tools choices, disabling parallel
/// tool calls, a tool result without a matching call, and any other member
/// without a `GenerateContent` equivalent.
pub(super) fn request(
    chat: OpenAiChatRequest,
) -> Result<GoogleGenerateContentRequest, UnsupportedRequest> {
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
            ("parallel_tool_calls", parallel_tool_calls == Some(false)),
            ("logit_bias", logit_bias.is_some()),
            ("user", user.is_some()),
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
        ],
    )?;
    super::reject_extra(ADAPTER, &extra)?;
    let (system, offset) = super::system_prefix(&messages)?;
    let (response_mime_type, response_schema) = output(response_format)?;
    let generation = GoogleGenerationConfig {
        temperature,
        top_p,
        candidate_count: n,
        max_output_tokens: max_completion_tokens.or(max_tokens),
        stop_sequences: super::stop_sequences(stop),
        response_mime_type,
        response_schema,
        presence_penalty,
        frequency_penalty,
        seed,
        ..GoogleGenerationConfig::default()
    };
    Ok(GoogleGenerateContentRequest {
        contents: conversation(&messages[offset..], offset)?,
        system_instruction: system.map(|text| GoogleContent {
            role: "user".to_owned(),
            parts: vec![GooglePart::Text { text }],
        }),
        tools: tools.map(translate_tools).transpose()?,
        tool_config: choice(tool_choice)?,
        settings: GoogleGenerateSettings {
            generation_config: (generation != GoogleGenerationConfig::default())
                .then_some(generation),
            ..GoogleGenerateSettings::default()
        },
    })
}

/// Translates the conversational messages that follow the system prefix.
///
/// # Errors
///
/// Returns [`UnsupportedRequest`] for an unknown role, unsupported content, or
/// a tool result without a matching earlier call.
fn conversation(
    messages: &[OpenAiChatMessage],
    offset: usize,
) -> Result<Vec<GoogleContent>, UnsupportedRequest> {
    let mut calls: HashMap<&str, &str> = HashMap::new();
    let mut contents: Vec<GoogleContent> = Vec::new();
    for (index, message) in messages.iter().enumerate() {
        let field = format!("messages[{}]", index + offset);
        super::plain_message(message, &field)?;
        let (role, parts) = match message.role.as_str() {
            "user" => ("user", user_parts(message.content.as_ref(), &field)?),
            "assistant" => {
                let mut parts: Vec<_> = super::text_content(message.content.as_ref(), &field)?
                    .map(|text| GooglePart::Text { text })
                    .into_iter()
                    .collect();
                for call in message.tool_calls.iter().flatten() {
                    calls.insert(&call.id, &call.function.name);
                    parts.push(GooglePart::FunctionCall {
                        function_call: GoogleFunctionCall {
                            name: call.function.name.clone(),
                            args: super::arguments(call, &field)?,
                        },
                    });
                }
                ("model", parts)
            }
            "tool" => {
                let name = message
                    .tool_call_id
                    .as_deref()
                    .and_then(|id| calls.get(id))
                    .ok_or_else(|| {
                        UnsupportedRequest::new(
                            format!("{field}.tool_call_id"),
                            "must name a tool call of an earlier assistant message",
                        )
                    })?;
                let text =
                    super::text_content(message.content.as_ref(), &field)?.unwrap_or_default();
                let response = match serde_json::from_str::<Value>(&text) {
                    Ok(object @ Value::Object(_)) => object,
                    _ => json!({"content": text}),
                };
                (
                    "user",
                    vec![GooglePart::FunctionResponse {
                        function_response: GoogleFunctionResponse {
                            name: (*name).to_owned(),
                            response,
                        },
                    }],
                )
            }
            _ => {
                return Err(UnsupportedRequest::new(
                    format!("{field}.role"),
                    "must be user, assistant, or tool",
                ));
            }
        };
        if parts.is_empty() {
            return Err(UnsupportedRequest::new(
                format!("{field}.content"),
                "a message needs content",
            ));
        }
        match contents.last_mut() {
            Some(last) if last.role == role => last.parts.extend(parts),
            _ => contents.push(GoogleContent {
                role: role.to_owned(),
                parts,
            }),
        }
    }
    Ok(contents)
}

/// Translates user content: text and base64 `data:` images.
///
/// # Errors
///
/// Returns [`UnsupportedRequest`] for absent content, remote image URLs, a
/// detail hint, or audio and file parts.
fn user_parts(
    content: Option<&OpenAiMessageContent>,
    field: &str,
) -> Result<Vec<GooglePart>, UnsupportedRequest> {
    match content {
        None => Err(UnsupportedRequest::new(
            format!("{field}.content"),
            "is required",
        )),
        Some(OpenAiMessageContent::Text(text)) => Ok(vec![GooglePart::Text { text: text.clone() }]),
        Some(OpenAiMessageContent::Parts(parts)) => parts
            .iter()
            .enumerate()
            .map(|(index, part)| {
                let field = format!("{field}.content[{index}]");
                match part {
                    OpenAiContentPart::Text { text } => Ok(GooglePart::Text { text: text.clone() }),
                    OpenAiContentPart::ImageUrl { image_url } => {
                        match super::image(image_url, &field)? {
                            Some((mime_type, data)) => Ok(GooglePart::InlineData {
                                inline_data: GoogleInlineData { mime_type, data },
                            }),
                            None => Err(UnsupportedRequest::new(
                                format!("{field}.image_url.url"),
                                "must be a base64 data URL for the google adapter",
                            )),
                        }
                    }
                    OpenAiContentPart::InputAudio { .. } | OpenAiContentPart::File { .. } => {
                        Err(UnsupportedRequest::new(
                            field,
                            "audio and file parts are not supported by the google adapter",
                        ))
                    }
                }
            })
            .collect(),
    }
}

/// Translates function tools into one declaration list.
///
/// # Errors
///
/// Returns [`UnsupportedRequest`] for custom tools or strict functions.
fn translate_tools(tools: Vec<OpenAiTool>) -> Result<Vec<GoogleTool>, UnsupportedRequest> {
    let declarations = tools
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
            } if strict != Some(true) => Ok(GoogleFunctionDeclaration {
                name,
                description,
                parameters: super::parameters(parameters),
            }),
            _ => Err(UnsupportedRequest::new(
                format!("tools[{index}]"),
                "only non-strict function tools are supported by the google adapter",
            )),
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(vec![GoogleTool {
        function_declarations: Some(declarations),
        google_search: None,
        google_search_retrieval: None,
        code_execution: None,
        url_context: None,
    }])
}

/// Translates the tool choice into a function-calling config.
///
/// # Errors
///
/// Returns [`UnsupportedRequest`] for allowed-tools and custom-tool choices.
fn choice(
    choice: Option<OpenAiChatToolChoice>,
) -> Result<Option<GoogleToolConfig>, UnsupportedRequest> {
    let (mode, allowed) = match choice {
        None => return Ok(None),
        Some(OpenAiChatToolChoice::Mode(OpenAiToolChoiceMode::None)) => ("NONE", None),
        Some(OpenAiChatToolChoice::Mode(OpenAiToolChoiceMode::Auto)) => ("AUTO", None),
        Some(OpenAiChatToolChoice::Mode(OpenAiToolChoiceMode::Required)) => ("ANY", None),
        Some(OpenAiChatToolChoice::Function(named)) => ("ANY", Some(vec![named.function.name])),
        Some(OpenAiChatToolChoice::Allowed(_) | OpenAiChatToolChoice::Custom(_)) => {
            return Err(UnsupportedRequest::new(
                "tool_choice",
                "allowed-tools and custom-tool choices are not supported by the google adapter",
            ));
        }
    };
    Ok(Some(GoogleToolConfig {
        function_calling_config: GoogleFunctionCallingConfig {
            mode: mode.to_owned(),
            allowed_function_names: allowed,
        },
    }))
}

/// Translates a response format into a response MIME type and schema.
///
/// # Errors
///
/// Returns [`UnsupportedRequest`] for a schema-less or described JSON schema.
fn output(
    format: Option<OpenAiResponseFormat>,
) -> Result<(Option<String>, Option<Value>), UnsupportedRequest> {
    let json = || Some("application/json".to_owned());
    match format {
        None | Some(OpenAiResponseFormat::Text) => Ok((None, None)),
        Some(OpenAiResponseFormat::JsonObject) => Ok((json(), None)),
        Some(OpenAiResponseFormat::JsonSchema {
            json_schema:
                OpenAiJsonSchema {
                    schema: Some(schema),
                    description: None,
                    ..
                },
        }) => Ok((json(), Some(Value::Object(schema)))),
        Some(OpenAiResponseFormat::JsonSchema { .. }) => Err(UnsupportedRequest::new(
            "response_format",
            "a json_schema format needs a schema and no description for the google adapter",
        )),
    }
}

/// Translates a `GenerateContent` answer into a chat completion named `id`.
///
/// Each candidate becomes a choice: text parts join into content, function
/// calls become tool calls with synthesized ids, and thought summaries are
/// omitted. Thought tokens count as completion and reasoning tokens.
///
/// Returns `None` without candidates, for other part kinds, or for a missing
/// or unrepresentable finish reason.
pub(super) fn response(
    native: GoogleGenerateContentResponse,
    id: &str,
    model: &str,
) -> Option<OpenAiChatResponse> {
    if native.candidates.is_empty() {
        return None;
    }
    let mut choices = Vec::with_capacity(native.candidates.len());
    for (position, candidate) in native.candidates.into_iter().enumerate() {
        let index = candidate
            .index
            .unwrap_or_else(|| u32::try_from(position).unwrap_or(u32::MAX));
        let mut content = String::new();
        let mut calls = Vec::new();
        for part in candidate.content.parts {
            match part {
                GooglePart::Text { text } => content.push_str(&text),
                GooglePart::Thought { .. } => {}
                GooglePart::FunctionCall { function_call } => calls.push(OpenAiToolCall {
                    id: format!("call_{index}_{}", calls.len()),
                    kind: "function".to_owned(),
                    function: OpenAiToolFunctionCall {
                        name: function_call.name,
                        arguments: function_call.args.to_string(),
                    },
                }),
                _ => return None,
            }
        }
        let finish = finish(&candidate.finish_reason?, !calls.is_empty())?;
        choices.push(OpenAiChatChoice {
            index,
            message: OpenAiChatMessage {
                role: "assistant".to_owned(),
                content: (!content.is_empty()).then_some(OpenAiMessageContent::Text(content)),
                tool_calls: (!calls.is_empty()).then_some(calls),
                ..OpenAiChatMessage::default()
            },
            finish_reason: Some(finish.to_owned()),
            logprobs: None,
        });
    }
    Some(OpenAiChatResponse {
        id: id.to_owned(),
        object: "chat.completion".to_owned(),
        created: super::now(),
        model: model.to_owned(),
        choices,
        usage: native.usage_metadata.as_ref().map(chat_usage),
        system_fingerprint: None,
        service_tier: None,
    })
}

/// Chat completion finish reason of a Google finish reason for a candidate
/// that made function `calls` or not, or `None` for a reason a chat
/// completion cannot represent.
pub(super) const fn finish(reason: &GoogleFinishReason, calls: bool) -> Option<&'static str> {
    match reason {
        GoogleFinishReason::Stop if calls => Some("tool_calls"),
        GoogleFinishReason::Stop => Some("stop"),
        GoogleFinishReason::MaxTokens => Some("length"),
        GoogleFinishReason::Safety
        | GoogleFinishReason::Recitation
        | GoogleFinishReason::Blocklist
        | GoogleFinishReason::ProhibitedContent
        | GoogleFinishReason::Spii => Some("content_filter"),
        _ => None,
    }
}

/// Chat completion usage of Google token counts: thought tokens count as
/// completion and reasoning tokens.
pub(super) fn chat_usage(usage: &GoogleUsageMetadata) -> OpenAiUsage {
    let completion = usage.candidates_token_count + usage.thoughts_token_count;
    OpenAiUsage {
        prompt_tokens: usage.prompt_token_count,
        completion_tokens: completion,
        total_tokens: usage.prompt_token_count + completion,
        prompt_tokens_details: Some(OpenAiPromptTokensDetails {
            audio_tokens: 0,
            cached_tokens: usage.cached_content_token_count,
        }),
        completion_tokens_details: Some(OpenAiCompletionTokensDetails {
            reasoning_tokens: usage.thoughts_token_count,
            ..OpenAiCompletionTokensDetails::default()
        }),
    }
}
