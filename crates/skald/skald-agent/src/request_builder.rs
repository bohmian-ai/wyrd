//! Per-iteration provider-native request assembly helpers.

use skald_spec::wire::anthropic_messages::{
    AnthropicContentBlock, AnthropicMessage, AnthropicToolResultContent,
};
use skald_spec::wire::google_generate::{GoogleContent, GoogleFunctionResponse, GooglePart};
use skald_spec::wire::openai_chat::{
    OpenAiChatMessage, OpenAiMessageContent, OpenAiToolCall, OpenAiToolFunctionCall,
};
use skald_spec::wire::openai_responses::{OpenAiResponseContentPart, OpenAiResponseItem};
use skald_spec::{MessageNum, ProviderName, ProviderRequest, ProviderResponse};

use crate::conversation::{Conversation, ConversationTurn};
use crate::error::{AgentError, AgentResult};

/// Closed list of request shapes the agent tool loop currently supports.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptLoopSupport {
    /// `ProviderRequest::OpenAiChatCompletion`.
    OpenAiChat,
    /// `ProviderRequest::OpenAiResponses`. Loop turns are held as OpenAI Chat
    /// messages and lowered to Responses input items when the request is
    /// rebuilt.
    OpenAiResponses,
    /// `ProviderRequest::AnthropicMessage`.
    Anthropic,
    /// `ProviderRequest::GeminiGenerateContent`.
    Gemini,
    /// `ProviderRequest::Vertex`.
    Vertex,
}

/// Decide whether a rendered provider request can drive the agent tool loop.
pub fn validate_prompt_loop_request(
    agent: &str,
    request: &ProviderRequest,
) -> AgentResult<PromptLoopSupport> {
    match request {
        ProviderRequest::OpenAiChatCompletion(_) | ProviderRequest::OpenAiChatCompatible { .. } => {
            Ok(PromptLoopSupport::OpenAiChat)
        }
        ProviderRequest::AnthropicMessage(_) => Ok(PromptLoopSupport::Anthropic),
        ProviderRequest::GeminiGenerateContent(_) => Ok(PromptLoopSupport::Gemini),
        ProviderRequest::Vertex(_) => Ok(PromptLoopSupport::Vertex),
        ProviderRequest::OpenAiResponses(_) => Ok(PromptLoopSupport::OpenAiResponses),
        ProviderRequest::OpenAiEmbeddings(_) | ProviderRequest::GoogleBatchEmbed(_) => {
            Err(AgentError::Prompt {
                agent: agent.to_owned(),
                detail: "embedding-only request shapes cannot drive an agent tool loop".to_owned(),
            })
        }
        ProviderRequest::VertexPredict(_) => Err(AgentError::Prompt {
            agent: agent.to_owned(),
            detail: "vertex prediction requests cannot drive an agent tool loop".to_owned(),
        }),
        ProviderRequest::RawV1 { provider, .. } => Err(AgentError::Prompt {
            agent: agent.to_owned(),
            detail: format!(
                "raw {provider:?} passthrough request shape cannot drive an agent tool loop"
            ),
        }),
        _ => Err(AgentError::Prompt {
            agent: agent.to_owned(),
            detail: "request shape does not support tool-loop dispatch".to_owned(),
        }),
    }
}

/// Extract the messages embedded in a rendered provider request.
///
/// OpenAI Responses input items have no [`MessageNum`] form, so they stay in
/// the request template and this returns no seed messages for them;
/// [`rebuild_request_messages`] appends loop history after those items.
///
/// # Errors
///
/// Returns [`AgentError::Prompt`] when the request shape cannot drive the loop.
pub fn extract_messages(agent: &str, request: &ProviderRequest) -> AgentResult<Vec<MessageNum>> {
    let kind = validate_prompt_loop_request(agent, request)?;
    match (kind, request) {
        (PromptLoopSupport::OpenAiResponses, _) => Ok(Vec::new()),
        (PromptLoopSupport::OpenAiChat, ProviderRequest::OpenAiChatCompletion(req))
        | (
            PromptLoopSupport::OpenAiChat,
            ProviderRequest::OpenAiChatCompatible { request: req, .. },
        ) => Ok(req
            .messages
            .iter()
            .cloned()
            .map(|m| MessageNum::OpenAi(Box::new(m)))
            .collect()),
        (PromptLoopSupport::Anthropic, ProviderRequest::AnthropicMessage(req)) => Ok(req
            .messages
            .iter()
            .cloned()
            .map(MessageNum::Anthropic)
            .collect()),
        (PromptLoopSupport::Gemini, ProviderRequest::GeminiGenerateContent(req)) => Ok(req
            .contents
            .iter()
            .cloned()
            .map(MessageNum::Gemini)
            .collect()),
        (PromptLoopSupport::Vertex, ProviderRequest::Vertex(req)) => Ok(req
            .0
            .contents
            .iter()
            .cloned()
            .map(MessageNum::Gemini)
            .collect()),
        _ => unreachable!("validated request classification must match request variant"),
    }
}

/// Build a provider-native request from rendered prompt seed messages and conversation turns.
pub fn request_from_conversation(
    agent: &str,
    template: ProviderRequest,
    seed_messages: &[MessageNum],
    conversation: &Conversation,
) -> AgentResult<ProviderRequest> {
    let mut messages = seed_messages.to_vec();
    let provider = validate_prompt_loop_request(agent, &template)?;
    for turn in conversation.turns() {
        append_turn_message(agent, provider, &mut messages, turn)?;
    }
    rebuild_request_messages(template, &messages)
}

/// Extract the provider-native assistant message from one model response.
pub fn assistant_message(agent: &str, response: &ProviderResponse) -> AgentResult<MessageNum> {
    match response {
        ProviderResponse::OpenAiChatCompletion(response) => {
            let choice = response
                .choices
                .first()
                .ok_or_else(|| AgentError::LoopMessageType {
                    provider: skald_spec::ProviderName::OpenAi,
                    detail: format!("agent '{agent}' received OpenAI response with no choices"),
                })?;
            Ok(MessageNum::OpenAi(Box::new(choice.message.clone())))
        }
        ProviderResponse::AnthropicMessage(response) => {
            Ok(MessageNum::Anthropic(AnthropicMessage {
                role: response.role.clone(),
                content: response.content.clone(),
            }))
        }
        ProviderResponse::GeminiGenerateContent(response)
        | ProviderResponse::VertexGenerateContent(response) => {
            let candidate =
                response
                    .candidates
                    .first()
                    .ok_or_else(|| AgentError::LoopMessageType {
                        provider: skald_spec::ProviderName::Google,
                        detail: format!(
                            "agent '{agent}' received Google response with no candidates"
                        ),
                    })?;
            Ok(MessageNum::Gemini(candidate.content.clone().into()))
        }
        ProviderResponse::OpenAiResponses(_) => {
            Ok(MessageNum::OpenAi(Box::new(responses_assistant(response))))
        }
        ProviderResponse::OpenAiEmbeddings(_)
        | ProviderResponse::GoogleBatchEmbed(_)
        | ProviderResponse::VertexPredict(_)
        | ProviderResponse::RawV1(_)
        | _ => Err(AgentError::LoopMessageType {
            provider: skald_spec::ProviderName::Custom("unsupported".to_owned()),
            detail: "response shape does not support tool-loop conversation".to_owned(),
        }),
    }
}

/// Replace the messages of a provider-native request with accumulated history.
///
/// An OpenAI Responses template keeps its own input items, and the OpenAI
/// Chat-shaped history is lowered to Responses items appended after them.
///
/// # Errors
///
/// Returns [`AgentError::LoopMessageType`] when a history message does not
/// match the request dialect, and [`AgentError::Prompt`] when the request
/// shape cannot drive the loop.
pub fn rebuild_request_messages(
    mut template: ProviderRequest,
    new_messages: &[MessageNum],
) -> AgentResult<ProviderRequest> {
    let mismatch = |provider: ProviderName| AgentError::LoopMessageType {
        provider,
        detail: "provider-native loop history contains a mismatched message variant".to_owned(),
    };

    match &mut template {
        ProviderRequest::OpenAiChatCompletion(req)
        | ProviderRequest::OpenAiChatCompatible { request: req, .. } => {
            req.messages.clear();
            for msg in new_messages {
                match msg {
                    MessageNum::OpenAi(message) => req.messages.push((**message).clone()),
                    _ => return Err(mismatch(ProviderName::OpenAi)),
                }
            }
        }
        ProviderRequest::AnthropicMessage(req) => {
            req.messages.clear();
            for msg in new_messages {
                match msg {
                    MessageNum::Anthropic(message) if message.role != "system" => {
                        req.messages.push(message.clone());
                    }
                    MessageNum::Anthropic(_) => {}
                    _ => return Err(mismatch(ProviderName::Anthropic)),
                }
            }
        }
        ProviderRequest::GeminiGenerateContent(req) => {
            req.contents.clear();
            for msg in new_messages {
                match msg {
                    MessageNum::Gemini(message) if message.role != "system" => {
                        req.contents.push(message.clone());
                    }
                    MessageNum::Gemini(_) => {}
                    _ => return Err(mismatch(ProviderName::Google)),
                }
            }
        }
        ProviderRequest::Vertex(req) => {
            req.0.contents.clear();
            for msg in new_messages {
                match msg {
                    MessageNum::Gemini(message) if message.role != "system" => {
                        req.0.contents.push(message.clone());
                    }
                    MessageNum::Gemini(_) => {}
                    _ => return Err(mismatch(ProviderName::Vertex)),
                }
            }
        }
        ProviderRequest::OpenAiResponses(req) => {
            let items = req.input.items_mut();
            for msg in new_messages {
                match msg {
                    MessageNum::OpenAi(message) => items.extend(
                        responses_items(message).ok_or_else(|| mismatch(ProviderName::OpenAi))?,
                    ),
                    _ => return Err(mismatch(ProviderName::OpenAi)),
                }
            }
        }
        ProviderRequest::OpenAiEmbeddings(_)
        | ProviderRequest::GoogleBatchEmbed(_)
        | ProviderRequest::VertexPredict(_)
        | ProviderRequest::RawV1 { .. } => {
            return Err(AgentError::Prompt {
                agent: "<run_prompt>".to_owned(),
                detail: "request shape does not support tool-loop dispatch".to_owned(),
            });
        }
        _ => {
            return Err(AgentError::Prompt {
                agent: "<run_prompt>".to_owned(),
                detail: "request shape does not support tool-loop dispatch".to_owned(),
            });
        }
    }
    Ok(template)
}

fn append_turn_message(
    agent: &str,
    provider: PromptLoopSupport,
    messages: &mut Vec<MessageNum>,
    turn: &ConversationTurn,
) -> AgentResult<()> {
    match turn {
        ConversationTurn::System { content } => {
            messages.push(system_message(provider, content));
            Ok(())
        }
        ConversationTurn::User { content } => {
            messages.push(user_message(provider, content));
            Ok(())
        }
        ConversationTurn::Assistant { message } => {
            validate_message_provider(agent, provider, message)?;
            messages.push(message.clone());
            Ok(())
        }
        ConversationTurn::ToolResult {
            call_id,
            ok,
            content,
        } => {
            messages.push(tool_result_message(provider, call_id, *ok, content));
            Ok(())
        }
    }
}

fn validate_message_provider(
    agent: &str,
    provider: PromptLoopSupport,
    message: &MessageNum,
) -> AgentResult<()> {
    let valid = matches!(
        (provider, message),
        (
            PromptLoopSupport::OpenAiChat | PromptLoopSupport::OpenAiResponses,
            MessageNum::OpenAi(_)
        ) | (PromptLoopSupport::Anthropic, MessageNum::Anthropic(_))
            | (PromptLoopSupport::Gemini, MessageNum::Gemini(_))
            | (PromptLoopSupport::Vertex, MessageNum::Gemini(_))
    );
    if valid {
        return Ok(());
    }
    Err(AgentError::LoopMessageType {
        provider: match provider {
            PromptLoopSupport::OpenAiChat | PromptLoopSupport::OpenAiResponses => {
                skald_spec::ProviderName::OpenAi
            }
            PromptLoopSupport::Anthropic => skald_spec::ProviderName::Anthropic,
            PromptLoopSupport::Gemini => skald_spec::ProviderName::Google,
            PromptLoopSupport::Vertex => skald_spec::ProviderName::Vertex,
        },
        detail: format!("agent '{agent}' conversation contains a mismatched assistant message"),
    })
}

fn system_message(provider: PromptLoopSupport, content: &str) -> MessageNum {
    match provider {
        PromptLoopSupport::OpenAiChat | PromptLoopSupport::OpenAiResponses => {
            MessageNum::OpenAi(Box::new(openai_message("system", content)))
        }
        PromptLoopSupport::Anthropic => MessageNum::Anthropic(AnthropicMessage {
            role: "system".to_owned(),
            content: vec![AnthropicContentBlock::Text {
                text: content.to_owned(),
                cache_control: None,
                citations: None,
            }],
        }),
        PromptLoopSupport::Gemini | PromptLoopSupport::Vertex => {
            MessageNum::Gemini(GoogleContent {
                role: "system".to_owned(),
                parts: vec![GooglePart::Text {
                    text: content.to_owned(),
                }],
            })
        }
    }
}

fn user_message(provider: PromptLoopSupport, content: &str) -> MessageNum {
    match provider {
        PromptLoopSupport::OpenAiChat | PromptLoopSupport::OpenAiResponses => {
            MessageNum::OpenAi(Box::new(openai_message("user", content)))
        }
        PromptLoopSupport::Anthropic => MessageNum::Anthropic(AnthropicMessage {
            role: "user".to_owned(),
            content: vec![AnthropicContentBlock::Text {
                text: content.to_owned(),
                cache_control: None,
                citations: None,
            }],
        }),
        PromptLoopSupport::Gemini | PromptLoopSupport::Vertex => {
            MessageNum::Gemini(GoogleContent {
                role: "user".to_owned(),
                parts: vec![GooglePart::Text {
                    text: content.to_owned(),
                }],
            })
        }
    }
}

fn tool_result_message(
    provider: PromptLoopSupport,
    call_id: &str,
    ok: bool,
    content: &serde_json::Value,
) -> MessageNum {
    let content_text = content.to_string();
    match provider {
        PromptLoopSupport::OpenAiChat | PromptLoopSupport::OpenAiResponses => {
            let mut message = openai_message("tool", &content_text);
            message.tool_call_id = Some(call_id.to_owned());
            MessageNum::OpenAi(Box::new(message))
        }
        PromptLoopSupport::Anthropic => MessageNum::Anthropic(AnthropicMessage {
            role: "user".to_owned(),
            content: vec![AnthropicContentBlock::ToolResult {
                tool_use_id: call_id.to_owned(),
                content: AnthropicToolResultContent::Text(content_text),
                is_error: Some(!ok),
                cache_control: None,
            }],
        }),
        PromptLoopSupport::Gemini | PromptLoopSupport::Vertex => {
            MessageNum::Gemini(GoogleContent {
                role: "function".to_owned(),
                parts: vec![GooglePart::FunctionResponse {
                    function_response: GoogleFunctionResponse {
                        name: call_id.to_owned(),
                        response: serde_json::json!({ "content": content }),
                    },
                }],
            })
        }
    }
}

/// OpenAI Chat-shaped assistant turn for a Responses answer: its output text
/// and function calls.
///
/// ponytail: reasoning items are not replayed; the next request resends the
/// full history statelessly, as every other dialect does. Replay reasoning (or
/// chain `previous_response_id`) if reasoning-model tool loops need it.
fn responses_assistant(response: &ProviderResponse) -> OpenAiChatMessage {
    let adapter = response.adapter();
    let tool_calls: Vec<OpenAiToolCall> = adapter
        .tool_calls()
        .into_iter()
        .map(|call| OpenAiToolCall {
            id: call.id.into_owned(),
            kind: "function".to_owned(),
            function: OpenAiToolFunctionCall {
                name: call.name.into_owned(),
                arguments: call.arguments.into_owned(),
            },
        })
        .collect();
    OpenAiChatMessage {
        role: "assistant".to_owned(),
        content: adapter
            .text()
            .map(|text| OpenAiMessageContent::Text(text.into_owned())),
        tool_calls: (!tool_calls.is_empty()).then_some(tool_calls),
        ..Default::default()
    }
}

/// Lower one OpenAI Chat-shaped loop turn to Responses input items: a tool
/// result becomes a function-call output; other turns become a text message
/// followed by their function calls. `None` for multipart content, which loop
/// turns never carry.
fn responses_items(message: &OpenAiChatMessage) -> Option<Vec<OpenAiResponseItem>> {
    let text = match &message.content {
        Some(OpenAiMessageContent::Text(text)) => text.as_str(),
        Some(OpenAiMessageContent::Parts(_)) => return None,
        None => "",
    };
    if message.role == "tool" {
        return Some(vec![OpenAiResponseItem::FunctionCallOutput {
            call_id: message.tool_call_id.clone().unwrap_or_default(),
            output: text.to_owned(),
        }]);
    }
    let mut items = Vec::new();
    if !text.is_empty() {
        let text = text.to_owned();
        let part = if message.role == "assistant" {
            OpenAiResponseContentPart::OutputText { text }
        } else {
            OpenAiResponseContentPart::InputText { text }
        };
        items.push(OpenAiResponseItem::Message {
            role: message.role.clone(),
            content: vec![part],
        });
    }
    items.extend(message.tool_calls.iter().flatten().map(|call| {
        OpenAiResponseItem::FunctionCall {
            call_id: call.id.clone(),
            name: call.function.name.clone(),
            arguments: call.function.arguments.clone(),
        }
    }));
    Some(items)
}

fn openai_message(role: &str, content: &str) -> OpenAiChatMessage {
    OpenAiChatMessage {
        role: role.to_owned(),
        content: Some(OpenAiMessageContent::Text(content.to_owned())),
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use skald_spec::wire::anthropic_messages::{
        AnthropicContentBlock, AnthropicMessage, AnthropicMessagesRequest,
        AnthropicMessagesSettings,
    };
    use skald_spec::wire::google_generate::{
        GoogleContent, GoogleGenerateContentRequest, GoogleGenerateSettings, GooglePart,
    };
    use skald_spec::wire::openai_chat::{OpenAiChatMessage, OpenAiMessageContent};
    use skald_spec::wire::vertex_generate::VertexGenerateContentRequest;
    use skald_spec::{MessageNum, ProviderRequest};

    use super::{extract_messages, rebuild_request_messages};

    fn anthropic_request(messages: Vec<AnthropicMessage>) -> ProviderRequest {
        ProviderRequest::AnthropicMessage(AnthropicMessagesRequest {
            model: "claude-3-5-sonnet".to_owned(),
            messages,
            system: None,
            stream: None,
            tools: None,
            tool_choice: None,
            output_config: None,
            settings: AnthropicMessagesSettings::default(),
        })
    }

    fn anthropic_message(role: &str, text: &str) -> AnthropicMessage {
        AnthropicMessage {
            role: role.to_owned(),
            content: vec![AnthropicContentBlock::Text {
                text: text.to_owned(),
                cache_control: None,
                citations: None,
            }],
        }
    }

    fn google_request(contents: Vec<GoogleContent>) -> GoogleGenerateContentRequest {
        GoogleGenerateContentRequest {
            contents,
            system_instruction: None,
            tools: None,
            tool_config: None,
            settings: GoogleGenerateSettings::default(),
        }
    }

    fn gemini_request(contents: Vec<GoogleContent>) -> ProviderRequest {
        ProviderRequest::GeminiGenerateContent(google_request(contents))
    }

    fn vertex_request(contents: Vec<GoogleContent>) -> ProviderRequest {
        ProviderRequest::Vertex(VertexGenerateContentRequest(google_request(contents)))
    }

    fn google_message(role: &str, text: &str) -> GoogleContent {
        GoogleContent {
            role: role.to_owned(),
            parts: vec![GooglePart::Text {
                text: text.to_owned(),
            }],
        }
    }

    fn openai_message(role: &str, text: &str) -> OpenAiChatMessage {
        OpenAiChatMessage {
            role: role.to_owned(),
            content: Some(OpenAiMessageContent::Text(text.to_owned())),
            name: None,
            tool_calls: None,
            tool_call_id: None,
            refusal: None,
            annotations: Vec::new(),
            audio: None,
        }
    }

    #[test]
    fn extract_messages_gemini_returns_gemini_variants() {
        let request = gemini_request(vec![
            google_message("user", "hello"),
            google_message("model", "hi"),
        ]);

        let messages = extract_messages("agent", &request).expect("Gemini messages must extract");

        assert_eq!(messages.len(), 2);
        assert!(
            messages
                .iter()
                .all(|message| matches!(message, MessageNum::Gemini(_)))
        );
    }

    #[test]
    fn extract_messages_vertex_returns_gemini_variants() {
        let request = vertex_request(vec![
            google_message("user", "hello"),
            google_message("model", "hi"),
        ]);

        let messages = extract_messages("agent", &request).expect("Vertex messages must extract");

        assert_eq!(messages.len(), 2);
        assert!(
            messages
                .iter()
                .all(|message| matches!(message, MessageNum::Gemini(_)))
        );
    }

    #[test]
    fn rebuild_request_messages_anthropic_filters_out_system_role() {
        let template = anthropic_request(vec![]);
        let new_messages = vec![
            MessageNum::Anthropic(anthropic_message("system", "ignored")),
            MessageNum::Anthropic(anthropic_message("user", "kept")),
            MessageNum::Anthropic(anthropic_message("assistant", "kept")),
        ];

        let result = rebuild_request_messages(template, &new_messages)
            .expect("Anthropic rebuild must succeed");
        let ProviderRequest::AnthropicMessage(request) = result else {
            panic!("expected Anthropic request");
        };
        let roles = request
            .messages
            .iter()
            .map(|message| message.role.as_str())
            .collect::<Vec<_>>();

        assert_eq!(roles, vec!["user", "assistant"]);
    }

    #[test]
    fn rebuild_request_messages_gemini_filters_out_system_role() {
        let template = gemini_request(vec![]);
        let new_messages = vec![
            MessageNum::Gemini(google_message("system", "ignored")),
            MessageNum::Gemini(google_message("user", "kept")),
            MessageNum::Gemini(google_message("model", "kept")),
        ];

        let result =
            rebuild_request_messages(template, &new_messages).expect("Gemini rebuild must succeed");
        let ProviderRequest::GeminiGenerateContent(request) = result else {
            panic!("expected Gemini request");
        };
        let roles = request
            .contents
            .iter()
            .map(|message| message.role.as_str())
            .collect::<Vec<_>>();

        assert_eq!(roles, vec!["user", "model"]);
    }

    #[test]
    fn rebuild_request_messages_rejects_mismatched_variant() {
        let template = anthropic_request(vec![]);
        let new_messages = vec![MessageNum::OpenAi(Box::new(openai_message(
            "user",
            "wrong provider",
        )))];

        let err = rebuild_request_messages(template, &new_messages)
            .expect_err("variant mismatch must fail");

        assert_eq!(err.code(), "SKALD_AGENT_422_LOOP_MESSAGE_TYPE");
    }
}
