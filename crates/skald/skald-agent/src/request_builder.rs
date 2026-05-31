//! Per-iteration provider-native request assembly.

use serde_json::{Map, Value};
use skald_spec::wire::anthropic_messages::{
    AnthropicContentBlock, AnthropicMessage, AnthropicMessagesRequest, AnthropicMessagesSettings,
    AnthropicSystem, AnthropicTool,
};
use skald_spec::wire::google_generate::{
    GoogleContent, GoogleFunctionDeclaration, GoogleGenerateContentRequest, GoogleGenerateSettings,
    GoogleTool,
};
use skald_spec::wire::openai_chat::{
    OpenAiChatMessage, OpenAiChatRequest, OpenAiChatSettings, OpenAiFunction, OpenAiTool,
};
use skald_spec::wire::vertex_generate::VertexGenerateContentRequest;
use skald_spec::{MessageNum, ProviderName, ProviderRequest};

use crate::agent::Agent;
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
        ProviderRequest::OpenAiEmbeddings(_)
        | ProviderRequest::GoogleBatchEmbed(_)
        | ProviderRequest::VertexPredict(_) => Err(AgentError::Prompt {
            agent: agent.to_owned(),
            detail: "embedding-only request shapes cannot drive an agent tool loop".to_owned(),
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

/// Build the request the agent will send this iteration.
pub fn build_request(agent: &Agent, messages: &[MessageNum]) -> AgentResult<ProviderRequest> {
    match &agent.provider_name {
        ProviderName::OpenAi => build_openai_request(agent, messages),
        ProviderName::Anthropic => build_anthropic_request(agent, messages),
        ProviderName::Google | ProviderName::Vertex => build_google_request(agent, messages),
        ProviderName::Custom(_) => Err(AgentError::SystemPrompt {
            provider: agent.provider_name.clone(),
            detail: "custom provider request assembly is not supported by skald-agent".to_owned(),
        }),
    }
}

fn build_openai_request(agent: &Agent, messages: &[MessageNum]) -> AgentResult<ProviderRequest> {
    let model = agent
        .model_override
        .clone()
        .unwrap_or_else(|| "gpt-4o".to_owned());
    let mut chat_messages: Vec<OpenAiChatMessage> = Vec::new();

    for msg in agent.system_instruction() {
        if let MessageNum::OpenAi(message) = msg {
            chat_messages.push(message.clone());
        }
    }
    for msg in messages {
        match msg {
            MessageNum::OpenAi(message) => chat_messages.push(message.clone()),
            _ => {
                return Err(AgentError::SystemPrompt {
                    provider: ProviderName::OpenAi,
                    detail: "non-OpenAI message in loop history".to_owned(),
                });
            }
        }
    }

    let tools = if agent.tools().is_empty() {
        None
    } else {
        Some(
            agent
                .tools()
                .iter()
                .map(|tool| {
                    let parameters = tool.def().input_schema.as_object().cloned();
                    OpenAiTool::Function {
                        function: OpenAiFunction {
                            name: tool.def().name.clone(),
                            description: Some(tool.def().description.clone()),
                            parameters,
                            strict: None,
                        },
                    }
                })
                .collect(),
        )
    };

    Ok(ProviderRequest::OpenAiChatCompletion(OpenAiChatRequest {
        model,
        messages: chat_messages,
        response_format: None,
        stream: None,
        stream_options: None,
        tools,
        tool_choice: None,
        parallel_tool_calls: None,
        settings: OpenAiChatSettings::default(),
    }))
}

fn build_anthropic_request(agent: &Agent, messages: &[MessageNum]) -> AgentResult<ProviderRequest> {
    let model = agent
        .model_override
        .clone()
        .unwrap_or_else(|| "claude-3-5-sonnet-latest".to_owned());
    let system = agent.system_instruction().iter().find_map(|msg| match msg {
        MessageNum::Anthropic(message) if message.role == "system" => {
            Some(AnthropicSystem::Text(anthropic_system_text(message)))
        }
        _ => None,
    });
    let mut anthropic_messages: Vec<AnthropicMessage> = Vec::new();
    for msg in messages {
        match msg {
            MessageNum::Anthropic(message) if message.role != "system" => {
                anthropic_messages.push(message.clone());
            }
            MessageNum::Anthropic(_) => {}
            _ => {
                return Err(AgentError::SystemPrompt {
                    provider: ProviderName::Anthropic,
                    detail: "non-Anthropic message in loop history".to_owned(),
                });
            }
        }
    }
    let tools = if agent.tools().is_empty() {
        None
    } else {
        Some(
            agent
                .tools()
                .iter()
                .map(|tool| AnthropicTool {
                    name: tool.def().name.clone(),
                    description: Some(tool.def().description.clone()),
                    input_schema: tool.def().input_schema.clone(),
                    cache_control: None,
                    kind: None,
                    display_width_px: None,
                    display_height_px: None,
                    display_number: None,
                })
                .collect(),
        )
    };

    Ok(ProviderRequest::AnthropicMessage(
        AnthropicMessagesRequest {
            model,
            messages: anthropic_messages,
            system,
            stream: None,
            tools,
            tool_choice: None,
            settings: AnthropicMessagesSettings::default(),
        },
    ))
}

fn anthropic_system_text(message: &AnthropicMessage) -> String {
    message
        .content
        .iter()
        .filter_map(|block| match block {
            AnthropicContentBlock::Text { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn build_google_request(agent: &Agent, messages: &[MessageNum]) -> AgentResult<ProviderRequest> {
    let system_instruction = agent.system_instruction().iter().find_map(|msg| match msg {
        MessageNum::Gemini(content) if content.role == "system" => Some(content.clone()),
        _ => None,
    });
    let mut contents: Vec<GoogleContent> = Vec::new();
    for msg in messages {
        match msg {
            MessageNum::Gemini(content) if content.role != "system" => {
                contents.push(content.clone());
            }
            MessageNum::Gemini(_) => {}
            _ => {
                return Err(AgentError::SystemPrompt {
                    provider: ProviderName::Google,
                    detail: "non-Google message in loop history".to_owned(),
                });
            }
        }
    }
    let tools = if agent.tools().is_empty() {
        None
    } else {
        Some(vec![GoogleTool {
            function_declarations: Some(
                agent
                    .tools()
                    .iter()
                    .map(|tool| GoogleFunctionDeclaration {
                        name: tool.def().name.clone(),
                        description: Some(tool.def().description.clone()),
                        parameters: tool.def().input_schema.clone(),
                    })
                    .collect(),
            ),
            google_search: None,
            google_search_retrieval: None,
            code_execution: None,
            url_context: None,
        }])
    };

    let request = GoogleGenerateContentRequest {
        contents,
        system_instruction,
        tools,
        tool_config: None,
        settings: GoogleGenerateSettings {
            extra: Map::<String, Value>::new(),
            ..GoogleGenerateSettings::default()
        },
    };
    if matches!(agent.provider_name, ProviderName::Vertex) {
        Ok(ProviderRequest::Vertex(VertexGenerateContentRequest(
            request,
        )))
    } else {
        Ok(ProviderRequest::GeminiGenerateContent(request))
    }
}
