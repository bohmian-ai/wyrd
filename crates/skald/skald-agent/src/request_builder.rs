//! Per-iteration provider-native request assembly helpers.

use skald_spec::wire::anthropic_messages::{
    AnthropicContentBlock, AnthropicMessage, AnthropicToolResultContent,
};
use skald_spec::wire::google_generate::{GoogleContent, GoogleFunctionResponse, GooglePart};
use skald_spec::wire::openai_chat::{OpenAiChatMessage, OpenAiMessageContent};
use skald_spec::wire::openai_responses::{OpenAiResponseContentPart, OpenAiResponseItem};
use skald_spec::{MessageNum, ProviderName, ProviderRequest, ProviderResponse};

use crate::conversation::{Conversation, ConversationTurn};
use crate::error::{AgentError, AgentResult};

/// Closed list of request shapes the agent tool loop currently supports.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptLoopSupport {
    /// `ProviderRequest::OpenAiChatCompletion`.
    OpenAiChat,
    /// `ProviderRequest::OpenAiResponses`, held as native `input` items.
    OpenAiResponses,
    /// `ProviderRequest::AnthropicMessage`.
    Anthropic,
    /// `ProviderRequest::GeminiGenerateContent`, for Gemini or Vertex.
    Gemini,
}

/// Decide whether a rendered provider request can drive the agent tool loop.
///
/// Classifies the request into the closed [`PromptLoopSupport`] set that the
/// loop knows how to seed, extend with conversation history, and replay. The
/// check is pure; `agent` only names the Agent in the error.
///
/// # Errors
///
/// Returns [`AgentError::Prompt`] for embedding, Vertex prediction, raw
/// passthrough, and any other request shape that has no conversation to
/// drive.
pub fn validate_prompt_loop_request(
    agent: &str,
    request: &ProviderRequest,
) -> AgentResult<PromptLoopSupport> {
    match request {
        ProviderRequest::OpenAiChatCompletion(_) => Ok(PromptLoopSupport::OpenAiChat),
        ProviderRequest::AnthropicMessage(_) => Ok(PromptLoopSupport::Anthropic),
        ProviderRequest::GeminiGenerateContent(_) => Ok(PromptLoopSupport::Gemini),
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

/// Extract the messages embedded in a rendered provider request; each
/// OpenAI Responses `input` item becomes one native Responses message.
///
/// # Errors
///
/// Returns [`AgentError::Prompt`] when the request shape cannot drive the loop.
pub fn extract_messages(agent: &str, request: &ProviderRequest) -> AgentResult<Vec<MessageNum>> {
    let kind = validate_prompt_loop_request(agent, request)?;
    match (kind, request) {
        (PromptLoopSupport::OpenAiResponses, ProviderRequest::OpenAiResponses(req)) => Ok(req
            .input
            .items()
            .iter()
            .cloned()
            .map(|item| MessageNum::OpenAiResponses(vec![item]))
            .collect()),
        (PromptLoopSupport::OpenAiChat, ProviderRequest::OpenAiChatCompletion(req)) => Ok(req
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
        _ => unreachable!("validated request classification must match request variant"),
    }
}

/// Build a provider-native request from rendered prompt seed messages and conversation turns.
///
/// Starts from `seed_messages`, appends each conversation turn in the
/// request's own dialect (OpenAI Responses turns as native `input` items,
/// including replayed reasoning and function-call items), and replaces the
/// `template` messages with the result. The template's model, settings, and
/// tools are kept unchanged.
///
/// # Errors
///
/// Returns [`AgentError::Prompt`] when the template cannot drive the loop,
/// and [`AgentError::LoopMessageType`] when a turn or seed message does not
/// match the template's dialect.
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
///
/// OpenAI Chat keeps the first choice's message, Anthropic its role and
/// content blocks, Gemini and Vertex the first candidate's content, and
/// OpenAI Responses every output item in provider order, so reasoning
/// identity, summaries, encrypted state, and function calls are replayed on
/// the next stateless request.
///
/// # Errors
///
/// Returns [`AgentError::LoopMessageType`] when an OpenAI Chat response has
/// no choices, a Google response has no candidates, or the response shape
/// has no conversation message.
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
        ProviderResponse::GeminiGenerateContent(response) => {
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
        // Every output item, reasoning included, is replayed in provider order
        // so a stateless next request carries native continuation state.
        ProviderResponse::OpenAiResponses(response) => {
            Ok(MessageNum::OpenAiResponses(response.output.clone()))
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
/// An OpenAI Responses request's `input` becomes the concatenated items of
/// its native Responses messages.
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
        ProviderRequest::OpenAiChatCompletion(req) => {
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
        ProviderRequest::OpenAiResponses(req) => {
            let mut items = Vec::new();
            for msg in new_messages {
                match msg {
                    MessageNum::OpenAiResponses(message) => items.extend(message.iter().cloned()),
                    _ => return Err(mismatch(ProviderName::OpenAi)),
                }
            }
            req.input = items.into();
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
        (PromptLoopSupport::OpenAiChat, MessageNum::OpenAi(_))
            | (
                PromptLoopSupport::OpenAiResponses,
                MessageNum::OpenAiResponses(_)
            )
            | (PromptLoopSupport::Anthropic, MessageNum::Anthropic(_))
            | (PromptLoopSupport::Gemini, MessageNum::Gemini(_))
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
        },
        detail: format!("agent '{agent}' conversation contains a mismatched assistant message"),
    })
}

fn system_message(provider: PromptLoopSupport, content: &str) -> MessageNum {
    match provider {
        PromptLoopSupport::OpenAiChat => {
            MessageNum::OpenAi(Box::new(openai_message("system", content)))
        }
        PromptLoopSupport::OpenAiResponses => responses_message("system", content),
        PromptLoopSupport::Anthropic => MessageNum::Anthropic(AnthropicMessage {
            role: "system".to_owned(),
            content: vec![AnthropicContentBlock::Text {
                text: content.to_owned(),
                cache_control: None,
                citations: None,
            }],
        }),
        PromptLoopSupport::Gemini => MessageNum::Gemini(GoogleContent {
            role: "system".to_owned(),
            parts: vec![GooglePart::Text {
                text: content.to_owned(),
            }],
        }),
    }
}

fn user_message(provider: PromptLoopSupport, content: &str) -> MessageNum {
    match provider {
        PromptLoopSupport::OpenAiChat => {
            MessageNum::OpenAi(Box::new(openai_message("user", content)))
        }
        PromptLoopSupport::OpenAiResponses => responses_message("user", content),
        PromptLoopSupport::Anthropic => MessageNum::Anthropic(AnthropicMessage {
            role: "user".to_owned(),
            content: vec![AnthropicContentBlock::Text {
                text: content.to_owned(),
                cache_control: None,
                citations: None,
            }],
        }),
        PromptLoopSupport::Gemini => MessageNum::Gemini(GoogleContent {
            role: "user".to_owned(),
            parts: vec![GooglePart::Text {
                text: content.to_owned(),
            }],
        }),
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
        PromptLoopSupport::OpenAiResponses => {
            MessageNum::OpenAiResponses(vec![OpenAiResponseItem::FunctionCallOutput {
                call_id: call_id.to_owned(),
                output: content_text,
            }])
        }
        PromptLoopSupport::OpenAiChat => {
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
        PromptLoopSupport::Gemini => MessageNum::Gemini(GoogleContent {
            role: "function".to_owned(),
            parts: vec![GooglePart::FunctionResponse {
                function_response: GoogleFunctionResponse {
                    name: call_id.to_owned(),
                    response: serde_json::json!({ "content": content }),
                },
            }],
        }),
    }
}

/// Native Responses message item with one `input_text` part.
fn responses_message(role: &str, content: &str) -> MessageNum {
    MessageNum::OpenAiResponses(vec![OpenAiResponseItem::Message {
        role: role.to_owned(),
        content: vec![OpenAiResponseContentPart::InputText {
            text: content.to_owned(),
        }],
    }])
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
