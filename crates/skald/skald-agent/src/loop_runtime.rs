//! Bounded tool loop for `skald-agent`.

use std::sync::Arc;

use futures::StreamExt;
use futures::stream;
use skald_prompt::Prompt;
use skald_runtime::{ProviderRegistry, dispatch};
use skald_spec::{ProviderName, ProviderRequest, ProviderResponse, ToolDescriptor};
use skald_tool::AgentTool;
use tracing::{Instrument, debug_span};

use crate::agent::Agent;
use crate::callbacks::{
    AgentContext, ChainResult, apply_before_agent_chain_with_panic_catch,
    apply_chain_with_panic_catch, apply_chain_with_panic_catch_tool,
    apply_chain_with_panic_catch_tool_result,
};
use crate::conversation::{Conversation, ConversationTurn};
use crate::error::{AgentError, AgentResult};
use crate::journal::JournalEvent;
use crate::request_builder::{assistant_message, extract_messages, request_from_conversation};
use crate::run::{AgentRun, FinishReason};
use crate::session::{Role, SessionId, SessionTurn};

#[derive(Debug, Clone)]
struct ToolCall {
    id: String,
    name: String,
    args: serde_json::Value,
}

/// Runs the bounded loop for input text.
pub(crate) async fn run(
    this: &Agent,
    providers: &ProviderRegistry,
    session_id: Option<SessionId>,
    input: &str,
) -> AgentResult<AgentRun> {
    let span = debug_span!(
        "skald_agent.run",
        agent_id = this.id.as_str(),
        provider = ?this.prompt.native().request.provider(),
        cap = this.run_config.max_iterations,
        entry = "input",
    );
    async move {
        this.journal
            .append(JournalEvent::AgentStart {
                agent_id: this.id.clone(),
                input: input.to_owned(),
                session_id: session_id.as_ref().map(|id| id.as_str().to_owned()),
            })
            .await
            .map_err(|source| AgentError::JournalAppendFailed { source })?;

        let result = async {
            let mut conversation = Conversation::new();
            if let Some(session_id) = session_id.as_ref() {
                this.seed_session_recent(session_id, &mut conversation)
                    .await?;
            }
            conversation.append_user(input);
            append_session_turn(
                this,
                session_id.as_ref(),
                SessionTurn {
                    role: Role::User,
                    content: input.to_owned(),
                    call_id: None,
                },
            )
            .await?;
            let ctx = agent_context_with_session(this, session_id.as_ref(), 0, &conversation);
            match apply_before_agent_chain_with_panic_catch(
                &this.before_agent,
                &ctx,
                input.to_owned(),
                "before_agent",
            )? {
                ChainResult::Skip => {
                    return Ok(AgentRun {
                        output: String::new(),
                        final_response: None,
                        iterations: 0,
                        finish_reason: FinishReason::CallbackSkipped,
                        conversation,
                        errors: Vec::new(),
                    });
                }
                ChainResult::Replaced(input) => replace_seed_user_turn(&mut conversation, input),
            }
            let rendered = this
                .prompt
                .render_native(&[])
                .map_err(|err| AgentError::Prompt {
                    agent: this.id.clone(),
                    detail: err.to_string(),
                })?;
            let seed_messages = extract_messages(&this.id, &rendered)?;
            run_loop(
                this,
                providers,
                rendered,
                seed_messages,
                conversation,
                session_id.as_ref(),
            )
            .await
        }
        .await;

        append_terminal_journal_event(this, &result).await?;
        result
    }
    .instrument(span)
    .await
}

/// Runs the bounded loop for an explicit prompt.
pub(crate) async fn run_prompt(
    this: &Agent,
    providers: &ProviderRegistry,
    prompt: &Prompt,
    vars: &[(&str, &str)],
) -> AgentResult<AgentRun> {
    let span = debug_span!(
        "skald_agent.run_prompt",
        agent_id = this.id.as_str(),
        provider = ?this.prompt.native().request.provider(),
        cap = this.run_config.max_iterations,
        entry = "prompt",
    );
    async move {
        this.journal
            .append(JournalEvent::AgentStart {
                agent_id: this.id.clone(),
                input: String::new(),
                session_id: None,
            })
            .await
            .map_err(|source| AgentError::JournalAppendFailed { source })?;

        let result = async {
            let rendered = prompt
                .render_native(vars)
                .map_err(|err| AgentError::Prompt {
                    agent: this.id.clone(),
                    detail: err.to_string(),
                })?;
            let request_provider = rendered.provider();
            let agent_provider = this.prompt.native().request.provider();
            if request_provider != agent_provider {
                return Err(AgentError::ProviderMismatch {
                    agent: this.id.clone(),
                    agent_provider,
                    prompt_provider: request_provider,
                });
            }
            let seed_messages = extract_messages(&this.id, &rendered)?;
            run_loop(
                this,
                providers,
                rendered,
                seed_messages,
                Conversation::new(),
                None,
            )
            .await
        }
        .await;

        append_terminal_journal_event(this, &result).await?;
        result
    }
    .instrument(span)
    .await
}

async fn run_loop(
    this: &Agent,
    providers: &ProviderRegistry,
    template: ProviderRequest,
    seed_messages: Vec<skald_spec::MessageNum>,
    mut conversation: Conversation,
    session_id: Option<&SessionId>,
) -> AgentResult<AgentRun> {
    if this.run_config.max_iterations == 0 {
        return Err(AgentError::max_iterations(
            &this.id,
            this.run_config.max_iterations,
        ));
    }

    let concurrency_cap = this.run_config.tool_concurrency_cap.unwrap_or(8).max(1);
    let max_iterations = this.run_config.max_iterations;

    for iteration in 0..max_iterations {
        this.journal
            .append(JournalEvent::Iteration { index: iteration })
            .await
            .map_err(|source| AgentError::JournalAppendFailed { source })?;

        let ctx = agent_context_with_session(this, session_id, iteration, &conversation);
        let mut request =
            request_from_conversation(&this.id, template.clone(), &seed_messages, &conversation)?;
        if !this.tools.is_empty() {
            request = request.with_tools(tool_descriptors(&this.tools));
        }

        request = match apply_chain_with_panic_catch(
            &this.before_model,
            &ctx,
            request.clone(),
            "before_model",
        )? {
            ChainResult::Skip => {
                append_model_journal_call_result(
                    this,
                    iteration,
                    &request,
                    "callback_skipped".to_owned(),
                    true,
                )
                .await?;
                return Ok(AgentRun {
                    output: String::new(),
                    final_response: None,
                    iterations: iteration + 1,
                    finish_reason: FinishReason::CallbackSkipped,
                    conversation,
                    errors: Vec::new(),
                });
            }
            ChainResult::Replaced(replacement) => replacement,
        };

        append_model_journal_call(this, iteration, &request).await?;
        let response = dispatch(providers, request).await;
        match &response {
            Ok(response) => {
                append_model_journal_result(
                    this,
                    iteration,
                    response_finish_reason(response),
                    false,
                )
                .await?;
            }
            Err(error) => {
                append_model_journal_result(
                    this,
                    iteration,
                    format!("provider_error:{error}"),
                    true,
                )
                .await?;
            }
        }
        let response = response.map_err(AgentError::Provider)?;
        let response =
            match apply_chain_with_panic_catch(&this.after_model, &ctx, response, "after_model")? {
                ChainResult::Skip => {
                    unreachable!("after_model callbacks cannot skip a completed provider call")
                }
                ChainResult::Replaced(replacement) => replacement,
            };
        let assistant = assistant_message(&this.id, &response)?;
        conversation.append_assistant(assistant);
        append_session_turn(
            this,
            session_id,
            SessionTurn {
                role: Role::Assistant,
                content: response
                    .adapter()
                    .text()
                    .map(std::borrow::Cow::into_owned)
                    .unwrap_or_default(),
                call_id: None,
            },
        )
        .await?;

        let tool_calls = tool_calls_from_response(&response)?;
        if tool_calls.is_empty() {
            let output = response
                .adapter()
                .text()
                .map(std::borrow::Cow::into_owned)
                .unwrap_or_default();
            let run = AgentRun {
                output,
                final_response: Some(response),
                iterations: iteration + 1,
                finish_reason: FinishReason::ModelStopped,
                conversation,
                errors: Vec::new(),
            };
            return match apply_chain_with_panic_catch(&this.after_agent, &ctx, run, "after_agent")?
            {
                ChainResult::Skip => {
                    unreachable!("after_agent callbacks cannot skip a completed agent run")
                }
                ChainResult::Replaced(replacement) => Ok(replacement),
            };
        }

        validate_tools_attached(this, &tool_calls)?;
        let tools_snapshot = this.tools.clone();
        let before_tool = this.before_tool.clone();
        let after_tool = this.after_tool.clone();
        let journal = Arc::clone(&this.journal);
        let session = Arc::clone(&this.session);
        let session_id_owned = session_id.cloned();
        let indexed_calls = tool_calls.into_iter().enumerate();
        let mut results: Vec<(usize, ConversationTurn)> = stream::iter(indexed_calls)
            .map(|(idx, call)| {
                let tools_snapshot = tools_snapshot.clone();
                let before_tool = before_tool.clone();
                let after_tool = after_tool.clone();
                let ctx = ctx.clone();
                let journal = Arc::clone(&journal);
                let session = Arc::clone(&session);
                let session_id = session_id_owned.clone();
                async move {
                    let tool = tools_snapshot
                        .iter()
                        .find(|tool| tool.name() == call.name)
                        .cloned()
                        .expect("tool presence pre-validated");
                    journal
                        .append(JournalEvent::ToolCall {
                            iteration,
                            call_id: call.id.clone(),
                            tool_name: call.name.clone(),
                            args: call.args.clone(),
                        })
                        .await
                        .map_err(|source| AgentError::JournalAppendFailed { source })?;
                    let args = match apply_chain_with_panic_catch_tool(
                        &before_tool,
                        &ctx,
                        tool.as_ref(),
                        call.args,
                        "before_tool",
                    )? {
                        ChainResult::Skip => {
                            let content = serde_json::json!({ "skipped": true });
                            journal
                                .append(JournalEvent::ToolResult {
                                    iteration,
                                    call_id: call.id.clone(),
                                    ok: true,
                                    output: content.clone(),
                                })
                                .await
                                .map_err(|source| AgentError::JournalAppendFailed { source })?;
                            return Ok((
                                idx,
                                ConversationTurn::ToolResult {
                                    call_id: call.id,
                                    ok: true,
                                    content,
                                },
                            ));
                        }
                        ChainResult::Replaced(replacement) => replacement,
                    };
                    let result = tool.invoke(args).await;
                    let result = match apply_chain_with_panic_catch_tool_result(
                        &after_tool,
                        &ctx,
                        tool.as_ref(),
                        result,
                        "after_tool",
                    )? {
                        ChainResult::Skip => {
                            unreachable!("after_tool callbacks cannot skip a completed tool call")
                        }
                        ChainResult::Replaced(replacement) => replacement,
                    };
                    let (ok, content) = match result {
                        Ok(value) => (true, value),
                        Err(error) => (
                            false,
                            serde_json::json!({
                                "code": error.code(),
                                "error": error.to_string(),
                            }),
                        ),
                    };
                    journal
                        .append(JournalEvent::ToolResult {
                            iteration,
                            call_id: call.id.clone(),
                            ok,
                            output: content.clone(),
                        })
                        .await
                        .map_err(|source| AgentError::JournalAppendFailed { source })?;
                    if ok {
                        if let Some(session_id) = session_id.as_ref() {
                            session
                                .append(
                                    session_id,
                                    SessionTurn {
                                        role: Role::Tool,
                                        content: content.to_string(),
                                        call_id: Some(call.id.clone()),
                                    },
                                )
                                .await
                                .map_err(|source| AgentError::SessionAppendFailed {
                                    session_id: session_id.as_str().to_owned(),
                                    source,
                                })?;
                        }
                    }
                    Ok((
                        idx,
                        ConversationTurn::ToolResult {
                            call_id: call.id,
                            ok,
                            content,
                        },
                    ))
                }
            })
            .buffer_unordered(concurrency_cap)
            .collect::<Vec<AgentResult<(usize, ConversationTurn)>>>()
            .await
            .into_iter()
            .collect::<AgentResult<Vec<_>>>()?;

        results.sort_by_key(|(idx, _)| *idx);
        for (_, turn) in results {
            conversation.push(turn);
        }
    }

    Err(AgentError::max_iterations(&this.id, max_iterations))
}

impl Agent {
    #[rustfmt::skip]
    async fn seed_session_recent(
        &self,
        session_id: &SessionId,
        conversation: &mut Conversation,
    ) -> AgentResult<()> {
        let limit = self.run_config.session_recent_limit.unwrap_or(50);
        let recent = self.session.recent(session_id, limit).await.map_err(|source| AgentError::SessionRecentFailed {
            session_id: session_id.as_str().to_owned(),
            source,
        })?;
        for turn in recent {
            conversation.push(ConversationTurn::from(turn));
        }
        Ok(())
    }
}

async fn append_session_turn(
    this: &Agent,
    session_id: Option<&SessionId>,
    turn: SessionTurn,
) -> AgentResult<()> {
    if let Some(session_id) = session_id {
        this.session
            .append(session_id, turn)
            .await
            .map_err(|source| AgentError::SessionAppendFailed {
                session_id: session_id.as_str().to_owned(),
                source,
            })?;
    }
    Ok(())
}

async fn append_terminal_journal_event(
    this: &Agent,
    result: &AgentResult<AgentRun>,
) -> AgentResult<()> {
    let event = match result {
        Ok(run) => JournalEvent::AgentFinish {
            agent_id: this.id.clone(),
            finish_reason: format!("{:?}", run.finish_reason).to_lowercase(),
            iterations: run.iterations,
        },
        Err(error) => JournalEvent::AgentError {
            agent_id: this.id.clone(),
            code: error.code().to_owned(),
            message: error.to_string(),
        },
    };
    this.journal
        .append(event)
        .await
        .map_err(|source| AgentError::JournalAppendFailed { source })
}

async fn append_model_journal_call_result(
    this: &Agent,
    iteration: u32,
    request: &ProviderRequest,
    finish_reason: String,
    synthetic: bool,
) -> AgentResult<()> {
    append_model_journal_call(this, iteration, request).await?;
    append_model_journal_result(this, iteration, finish_reason, synthetic).await
}

async fn append_model_journal_call(
    this: &Agent,
    iteration: u32,
    request: &ProviderRequest,
) -> AgentResult<()> {
    let request_provider = request.provider();
    let request_model = request_model(request).unwrap_or_default().to_owned();
    this.journal
        .append(JournalEvent::ModelCall {
            iteration,
            provider: provider_label(&request_provider),
            model: request_model,
        })
        .await
        .map_err(|source| AgentError::JournalAppendFailed { source })
}

async fn append_model_journal_result(
    this: &Agent,
    iteration: u32,
    finish_reason: String,
    synthetic: bool,
) -> AgentResult<()> {
    this.journal
        .append(JournalEvent::ModelResult {
            iteration,
            finish_reason,
            synthetic,
        })
        .await
        .map_err(|source| AgentError::JournalAppendFailed { source })
}

fn tool_descriptors(tools: &[Arc<dyn AgentTool>]) -> Vec<ToolDescriptor> {
    tools
        .iter()
        .map(|tool| ToolDescriptor {
            name: tool.name().to_owned(),
            description: tool.description().to_owned(),
            parameters: tool.input_schema(),
        })
        .collect()
}

fn tool_calls_from_response(response: &skald_spec::ProviderResponse) -> AgentResult<Vec<ToolCall>> {
    response
        .adapter()
        .tool_calls()
        .into_iter()
        .map(|call| {
            let args = serde_json::from_str(&call.arguments).map_err(|error| {
                AgentError::InvalidToolArgs {
                    tool: call.name.clone().into_owned(),
                    detail: error.to_string(),
                }
            })?;
            Ok(ToolCall {
                id: call.id.into_owned(),
                name: call.name.into_owned(),
                args,
            })
        })
        .collect()
}

fn validate_tools_attached(this: &Agent, calls: &[ToolCall]) -> AgentResult<()> {
    for call in calls {
        if !this.tools.iter().any(|tool| tool.name() == call.name) {
            return Err(AgentError::ToolNotInAgent {
                name: call.name.clone(),
            });
        }
    }
    Ok(())
}

fn agent_context_with_session(
    this: &Agent,
    session_id: Option<&SessionId>,
    iteration: u32,
    conversation: &Conversation,
) -> AgentContext {
    AgentContext {
        agent_id: this.id.clone(),
        session_id: session_id.map(|id| id.as_str().to_owned()),
        iteration,
        conversation: Arc::new(conversation.clone()),
    }
}

fn replace_seed_user_turn(conversation: &mut Conversation, input: String) {
    if let Some(ConversationTurn::User { content }) = conversation
        .turns
        .iter_mut()
        .rev()
        .find(|turn| matches!(turn, ConversationTurn::User { .. }))
    {
        *content = input;
    }
}

fn request_model(request: &ProviderRequest) -> Option<&str> {
    match request {
        ProviderRequest::OpenAiChatCompletion(request) => Some(&request.model),
        ProviderRequest::OpenAiResponses(request) => Some(&request.model),
        ProviderRequest::OpenAiEmbeddings(request) => Some(&request.model),
        ProviderRequest::AnthropicMessage(request) => Some(&request.model),
        ProviderRequest::GeminiGenerateContent(_)
        | ProviderRequest::GoogleBatchEmbed(_)
        | ProviderRequest::Vertex(_)
        | ProviderRequest::VertexPredict(_)
        | ProviderRequest::RawV1 { .. } => None,
        _ => None,
    }
}

fn provider_label(provider: &ProviderName) -> String {
    match provider {
        ProviderName::OpenAi => "openai".to_owned(),
        ProviderName::Anthropic => "anthropic".to_owned(),
        ProviderName::Google => "google".to_owned(),
        ProviderName::Vertex => "vertex".to_owned(),
        ProviderName::Custom(name) => name.clone(),
    }
}

fn response_finish_reason(response: &ProviderResponse) -> String {
    match response {
        ProviderResponse::OpenAiChatCompletion(response) => response
            .choices
            .first()
            .and_then(|choice| choice.finish_reason.clone())
            .unwrap_or_else(|| "other".to_owned()),
        ProviderResponse::AnthropicMessage(response) => response
            .stop_reason
            .as_ref()
            .map(|reason| format!("{reason:?}").to_lowercase())
            .unwrap_or_else(|| "other".to_owned()),
        ProviderResponse::GeminiGenerateContent(response)
        | ProviderResponse::VertexGenerateContent(response) => response
            .candidates
            .first()
            .and_then(|candidate| candidate.finish_reason.as_ref())
            .map(|reason| format!("{reason:?}").to_lowercase())
            .unwrap_or_else(|| "other".to_owned()),
        _ => format!("{:?}", response.adapter().finish_reason()).to_lowercase(),
    }
}
