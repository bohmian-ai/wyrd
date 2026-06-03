//! Per-iteration provider-native request assembly helpers.

use skald_spec::{MessageNum, ProviderRequest};

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
        ProviderRequest::OpenAiChatCompletion(_) => Ok(PromptLoopSupport::OpenAiChat),
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
        (PromptLoopSupport::OpenAiChat, ProviderRequest::OpenAiChatCompletion(req)) => Ok(req
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

/// Replace the messages of a provider-native request with accumulated history.
pub fn reset_messages(
    mut template: ProviderRequest,
    new_messages: &[MessageNum],
) -> AgentResult<ProviderRequest> {
    let mismatch = |provider: &'static str| AgentError::Prompt {
        agent: "<run_prompt>".to_owned(),
        detail: format!("non-{provider} message in loop history"),
    };

    match &mut template {
        ProviderRequest::OpenAiChatCompletion(req) => {
            req.messages.clear();
            for msg in new_messages {
                match msg {
                    MessageNum::OpenAi(message) => req.messages.push(message.clone()),
                    _ => return Err(mismatch("OpenAI")),
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
                    _ => return Err(mismatch("Anthropic")),
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
                    _ => return Err(mismatch("Google")),
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
                    _ => return Err(mismatch("Vertex")),
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
