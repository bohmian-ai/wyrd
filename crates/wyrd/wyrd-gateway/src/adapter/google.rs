//! `OpenAI` Chat Completions translation to and from Google `GenerateContent`,
//! shared by the Gemini and Vertex adapters.

use std::collections::{HashMap, HashSet};

use serde_json::{Value, json};
use skald_spec::wire::google_generate::{
    GoogleContent, GoogleFinishReason, GoogleFunctionCall, GoogleFunctionCallingConfig,
    GoogleFunctionDeclaration, GoogleFunctionResponse, GoogleGenerateContentRequest,
    GoogleGenerateContentResponse, GoogleGenerateSettings, GoogleGenerationConfig,
    GoogleInlineData, GooglePart, GoogleTool, GoogleToolConfig, GoogleUsageMetadata,
};
use skald_spec::wire::openai_chat::{
    OpenAiChatChoice, OpenAiChatMessage, OpenAiChatMessageView, OpenAiChatRequest,
    OpenAiChatRequestView, OpenAiChatResponse, OpenAiChatSettings, OpenAiChatSettingsView,
    OpenAiChatToolChoice, OpenAiCompletionTokensDetails, OpenAiContentPart, OpenAiContentPartView,
    OpenAiFunction, OpenAiMessageContent, OpenAiMessageContentView, OpenAiPromptTokensDetails,
    OpenAiResponseFormat, OpenAiResponseFormatView, OpenAiTool, OpenAiToolCall,
    OpenAiToolChoiceMode, OpenAiToolFunctionCall, OpenAiUsage,
};

use super::UnsupportedRequest;

/// Adapter name used in rejections.
const ADAPTER: &str = "google";

/// Checks that a chat completion request has a faithful `GenerateContent`
/// form, on its borrowed projection, without building it.
///
/// # Errors
///
/// Returns [`UnsupportedRequest`] for streamed tools or obfuscation, `user`,
/// log probabilities,
/// remote image URLs (Google needs inline or uploaded media), audio or file
/// parts, custom or strict tools, allowed-tools choices, disabling parallel
/// tool calls, a tool result without a matching call, and any other member
/// without a `GenerateContent` equivalent.
pub(super) fn check(chat: &OpenAiChatRequestView<'_>) -> Result<(), UnsupportedRequest> {
    let OpenAiChatRequestView {
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
    let OpenAiChatSettingsView {
        temperature: _,
        top_p: _,
        max_tokens: _,
        max_completion_tokens: _,
        n: _,
        stop: _,
        presence_penalty: _,
        frequency_penalty: _,
        seed: _,
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
    super::streamable(ADAPTER, *stream, stream_options.as_ref(), tools.is_some())?;
    super::reject_present(
        ADAPTER,
        &[
            ("parallel_tool_calls", *parallel_tool_calls == Some(false)),
            ("logit_bias", logit_bias.is_some()),
            ("user", user.is_some()),
            ("reasoning_effort", reasoning_effort.is_some()),
            ("modalities", modalities.is_some()),
            ("audio", audio.is_some()),
            ("prediction", prediction.is_some()),
            ("prompt_cache_key", prompt_cache_key.is_some()),
            ("service_tier", service_tier.is_some()),
            ("safety_identifier", safety_identifier.is_some()),
            ("store", *store == Some(true)),
            ("metadata", metadata.is_some()),
            ("logprobs", *logprobs == Some(true)),
            ("top_logprobs", top_logprobs.is_some()),
        ],
    )?;
    super::reject_extra(ADAPTER, extra)?;
    let offset = super::system_prefix(messages)?;
    if let Some(OpenAiResponseFormatView::JsonSchema { json_schema }) = response_format
        && (json_schema.schema.is_none() || json_schema.description.is_some())
    {
        return Err(UnsupportedRequest::new(
            "response_format",
            "a json_schema format needs a schema and no description for the google adapter",
        ));
    }
    let mut calls: HashSet<&str> = HashSet::new();
    for (index, message) in messages.iter().enumerate().skip(offset) {
        conversation_message(message, &format!("messages[{index}]"), &mut calls)?;
    }
    super::function_tools(ADAPTER, tools.as_deref().unwrap_or_default())?;
    super::tool_choice(ADAPTER, tool_choice.as_ref())
}

/// Checks one conversational message that follows the system prefix,
/// recording the ids of its tool calls in `calls` for later tool results.
///
/// # Errors
///
/// Returns [`UnsupportedRequest`] for an unsupported member or role, absent
/// user content, remote image URLs, a detail hint, audio or file parts,
/// non-text assistant or tool content, invalid tool calls, a tool result
/// without a matching earlier call, or a message that translates to no parts.
fn conversation_message<'a>(
    message: &OpenAiChatMessageView<'a>,
    field: &str,
    calls: &mut HashSet<&'a str>,
) -> Result<(), UnsupportedRequest> {
    super::plain_message(message, field)?;
    let parts = match message.role {
        "user" => match &message.content {
            None => {
                return Err(UnsupportedRequest::new(
                    format!("{field}.content"),
                    "is required",
                ));
            }
            Some(OpenAiMessageContentView::Text(_)) => true,
            Some(OpenAiMessageContentView::Parts(parts)) => {
                for (index, part) in parts.iter().enumerate() {
                    let field = format!("{field}.content[{index}]");
                    match part {
                        OpenAiContentPartView::Text { .. } => {}
                        OpenAiContentPartView::ImageUrl { image_url } => {
                            if !super::image(image_url, &field)? {
                                return Err(UnsupportedRequest::new(
                                    format!("{field}.image_url.url"),
                                    "must be a base64 data URL for the google adapter",
                                ));
                            }
                        }
                        OpenAiContentPartView::InputAudio { .. }
                        | OpenAiContentPartView::File { .. } => {
                            return Err(UnsupportedRequest::new(
                                field,
                                "audio and file parts are not supported by the google adapter",
                            ));
                        }
                    }
                }
                !parts.is_empty()
            }
        },
        "assistant" => {
            let text = super::text_only(message.content.as_ref(), field)?;
            let tool_calls = message.tool_calls.as_deref().unwrap_or_default();
            for call in tool_calls {
                calls.insert(call.id);
                super::arguments(call, field)?;
            }
            text || !tool_calls.is_empty()
        }
        "tool" => {
            if !message.tool_call_id.is_some_and(|id| calls.contains(id)) {
                return Err(UnsupportedRequest::new(
                    format!("{field}.tool_call_id"),
                    "must name a tool call of an earlier assistant message",
                ));
            }
            super::text_only(message.content.as_ref(), field)?;
            true
        }
        _ => {
            return Err(UnsupportedRequest::new(
                format!("{field}.role"),
                "must be user, assistant, or tool",
            ));
        }
    };
    if parts {
        Ok(())
    } else {
        Err(UnsupportedRequest::new(
            format!("{field}.content"),
            "a message needs content",
        ))
    }
}

/// Translates a chat completion request [`check`] admitted into
/// `GenerateContent`.
///
/// Leading system messages become `system_instruction`; assistant turns use
/// the `model` role; consecutive same-role turns merge; tool results become
/// `function_response` parts named by the function of the matching earlier
/// call, carrying a JSON-object result as-is or text as `{"content": ...}`.
/// `n`, penalties, seed, stop sequences, and JSON output map onto the
/// generation config. Members [`check`] rejects are not represented.
pub(super) fn request(chat: OpenAiChatRequest) -> GoogleGenerateContentRequest {
    let OpenAiChatRequest {
        messages,
        response_format,
        tools,
        tool_choice,
        settings,
        ..
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
        ..
    } = settings;
    let (system, offset) = super::system_text(&messages);
    let (response_mime_type, response_json_schema) = output(response_format);
    let generation = GoogleGenerationConfig {
        temperature,
        top_p,
        candidate_count: n,
        max_output_tokens: max_completion_tokens.or(max_tokens),
        stop_sequences: super::stop_sequences(stop),
        response_mime_type,
        response_json_schema,
        presence_penalty,
        frequency_penalty,
        seed,
        ..GoogleGenerationConfig::default()
    };
    GoogleGenerateContentRequest {
        contents: conversation(&messages[offset..]),
        system_instruction: system.map(|text| GoogleContent {
            role: "user".to_owned(),
            parts: vec![GooglePart::Text { text }],
        }),
        tools: tools.map(translate_tools),
        tool_config: choice(tool_choice),
        settings: GoogleGenerateSettings {
            generation_config: (generation != GoogleGenerationConfig::default())
                .then_some(generation),
            ..GoogleGenerateSettings::default()
        },
    }
}

/// Translates the checked conversational messages that follow the system
/// prefix.
fn conversation(messages: &[OpenAiChatMessage]) -> Vec<GoogleContent> {
    let mut calls: HashMap<&str, &str> = HashMap::new();
    let mut contents: Vec<GoogleContent> = Vec::new();
    for message in messages {
        let (role, parts) = match message.role.as_str() {
            "user" => ("user", user_parts(message.content.as_ref())),
            "assistant" => {
                let mut parts: Vec<_> = super::text_content(message.content.as_ref())
                    .map(|text| GooglePart::Text { text })
                    .into_iter()
                    .collect();
                for call in message.tool_calls.iter().flatten() {
                    calls.insert(&call.id, &call.function.name);
                    parts.push(GooglePart::FunctionCall {
                        function_call: GoogleFunctionCall {
                            name: call.function.name.clone(),
                            args: super::arguments_value(call),
                        },
                        thought_signature: None,
                    });
                }
                ("model", parts)
            }
            _ => {
                let name = message
                    .tool_call_id
                    .as_deref()
                    .and_then(|id| calls.get(id))
                    .map_or_else(String::new, |name| (*name).to_owned());
                let text = super::text_content(message.content.as_ref()).unwrap_or_default();
                let response = match serde_json::from_str::<Value>(&text) {
                    Ok(object @ Value::Object(_)) => object,
                    _ => json!({"content": text}),
                };
                (
                    "user",
                    vec![GooglePart::FunctionResponse {
                        function_response: GoogleFunctionResponse { name, response },
                    }],
                )
            }
        };
        match contents.last_mut() {
            Some(last) if last.role == role => last.parts.extend(parts),
            _ => contents.push(GoogleContent {
                role: role.to_owned(),
                parts,
            }),
        }
    }
    contents
}

/// Translates checked user content: text and base64 `data:` images.
fn user_parts(content: Option<&OpenAiMessageContent>) -> Vec<GooglePart> {
    match content {
        None => Vec::new(),
        Some(OpenAiMessageContent::Text(text)) => vec![GooglePart::Text { text: text.clone() }],
        Some(OpenAiMessageContent::Parts(parts)) => parts
            .iter()
            .filter_map(|part| match part {
                OpenAiContentPart::Text { text } => Some(GooglePart::Text { text: text.clone() }),
                OpenAiContentPart::ImageUrl { image_url } => {
                    super::data_url(&image_url.url).map(|(mime_type, data)| {
                        GooglePart::InlineData {
                            inline_data: GoogleInlineData {
                                mime_type: mime_type.to_owned(),
                                data: data.to_owned(),
                            },
                        }
                    })
                }
                OpenAiContentPart::InputAudio { .. } | OpenAiContentPart::File { .. } => None,
            })
            .collect(),
    }
}

/// Translates checked non-strict function tools into one declaration list.
fn translate_tools(tools: Vec<OpenAiTool>) -> Vec<GoogleTool> {
    let declarations = tools
        .into_iter()
        .filter_map(|tool| match tool {
            OpenAiTool::Function {
                function:
                    OpenAiFunction {
                        name,
                        description,
                        parameters,
                        ..
                    },
            } => Some(GoogleFunctionDeclaration::json_schema(
                name,
                description,
                super::parameters(parameters),
            )),
            OpenAiTool::Custom { .. } => None,
        })
        .collect();
    vec![GoogleTool {
        function_declarations: Some(declarations),
        google_search: None,
        google_search_retrieval: None,
        code_execution: None,
        url_context: None,
    }]
}

/// Translates a checked tool choice into a function-calling config.
fn choice(choice: Option<OpenAiChatToolChoice>) -> Option<GoogleToolConfig> {
    let (mode, allowed) = match choice? {
        OpenAiChatToolChoice::Mode(OpenAiToolChoiceMode::None) => ("NONE", None),
        OpenAiChatToolChoice::Mode(OpenAiToolChoiceMode::Auto) => ("AUTO", None),
        OpenAiChatToolChoice::Mode(OpenAiToolChoiceMode::Required) => ("ANY", None),
        OpenAiChatToolChoice::Function(named) => ("ANY", Some(vec![named.function.name])),
        OpenAiChatToolChoice::Allowed(_) | OpenAiChatToolChoice::Custom(_) => return None,
    };
    Some(GoogleToolConfig {
        function_calling_config: GoogleFunctionCallingConfig {
            mode: mode.to_owned(),
            allowed_function_names: allowed,
        },
    })
}

/// Translates a checked response format into a response MIME type and
/// schema.
fn output(format: Option<OpenAiResponseFormat>) -> (Option<String>, Option<Value>) {
    let json = || Some("application/json".to_owned());
    match format {
        None | Some(OpenAiResponseFormat::Text) => (None, None),
        Some(OpenAiResponseFormat::JsonObject) => (json(), None),
        Some(OpenAiResponseFormat::JsonSchema { json_schema }) => {
            (json(), json_schema.schema.map(Value::Object))
        }
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
                GooglePart::FunctionCall { function_call, .. } => calls.push(OpenAiToolCall {
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
