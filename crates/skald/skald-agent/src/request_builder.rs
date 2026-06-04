//! Per-iteration provider-native request assembly helpers.

use skald_spec::wire::anthropic_messages::{
    AnthropicContentBlock, AnthropicMessage, AnthropicToolResultContent,
};
use skald_spec::wire::google_generate::{GoogleContent, GoogleFunctionResponse, GooglePart};
use skald_spec::wire::openai_chat::{OpenAiChatMessage, OpenAiMessageContent};
use skald_spec::{MessageNum, ProviderName, ProviderRequest, ProviderResponse};

use crate::conversation::{Conversation, ConversationTurn};
use crate::error::{AgentError, AgentResult};

/// Closed list of request shapes the agent tool loop currently supports.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptLoopSupport {
    /// `ProviderRequest::OpenAiChatCompletion`.
    OpenAiChat,
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
        ProviderRequest::OpenAiResponses(_) => Err(AgentError::Prompt {
            agent: agent.to_owned(),
            detail: "OpenAI Responses requests are not yet supported by the agent tool loop"
                .to_owned(),
        }),
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
pub fn extract_messages(agent: &str, request: &ProviderRequest) -> AgentResult<Vec<MessageNum>> {
    let kind = validate_prompt_loop_request(agent, request)?;
    match (kind, request) {
        (PromptLoopSupport::OpenAiChat, ProviderRequest::OpenAiChatCompletion(req))
        | (
            PromptLoopSupport::OpenAiChat,
            ProviderRequest::OpenAiChatCompatible { request: req, .. },
        ) => Ok(req
            .messages
            .iter()
            .cloned()
            .map(MessageNum::OpenAi)
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
            Ok(MessageNum::OpenAi(choice.message.clone()))
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
            Ok(MessageNum::Gemini(candidate.content.clone()))
        }
        ProviderResponse::OpenAiResponses(_)
        | ProviderResponse::OpenAiEmbeddings(_)
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
                    MessageNum::OpenAi(message) => req.messages.push(message.clone()),
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
        ProviderRequest::OpenAiResponses(_)
        | ProviderRequest::OpenAiEmbeddings(_)
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
            | (PromptLoopSupport::Anthropic, MessageNum::Anthropic(_))
            | (PromptLoopSupport::Gemini, MessageNum::Gemini(_))
            | (PromptLoopSupport::Vertex, MessageNum::Gemini(_))
    );
    if valid {
        return Ok(());
    }
    Err(AgentError::LoopMessageType {
        provider: match provider {
            PromptLoopSupport::OpenAiChat => skald_spec::ProviderName::OpenAi,
            PromptLoopSupport::Anthropic => skald_spec::ProviderName::Anthropic,
            PromptLoopSupport::Gemini => skald_spec::ProviderName::Google,
            PromptLoopSupport::Vertex => skald_spec::ProviderName::Vertex,
        },
        detail: format!("agent '{agent}' conversation contains a mismatched assistant message"),
    })
}

fn system_message(provider: PromptLoopSupport, content: &str) -> MessageNum {
    match provider {
        PromptLoopSupport::OpenAiChat => MessageNum::OpenAi(openai_message("system", content)),
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
        PromptLoopSupport::OpenAiChat => MessageNum::OpenAi(openai_message("user", content)),
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
        PromptLoopSupport::OpenAiChat => {
            let mut message = openai_message("tool", &content_text);
            message.tool_call_id = Some(call_id.to_owned());
            MessageNum::OpenAi(message)
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
        let new_messages = vec![MessageNum::OpenAi(openai_message("user", "wrong provider"))];

        let err = rebuild_request_messages(template, &new_messages)
            .expect_err("variant mismatch must fail");

        assert_eq!(err.code(), "SKALD_AGENT_422_LOOP_MESSAGE_TYPE");
    }
}
