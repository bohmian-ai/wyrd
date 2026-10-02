//! Bounded tool loop for `skald-agent`.

use std::sync::Arc;

use futures::StreamExt;
use futures::stream;
use skald_prompt::Prompt;
use skald_runtime::{ProviderRegistry, dispatch};
use skald_spec::{ProviderName, ProviderRequest, ProviderResponse, ResponseType, ToolDescriptor};
use skald_tool::AgentTool;
use tracing::{Instrument, Span, field, info_span};

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
use crate::session::{Role, SessionId, SessionTurn, session_turn_to_conversation_turn};

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
    let providers = this.effective_providers(providers);
    let span = invoke_agent_span(this);
    async move {
        const MAX_JOURNAL_INPUT_CHARS: usize = 4096;

        let body = async {
            let journal_input = if input.len() > MAX_JOURNAL_INPUT_CHARS {
                format!("{}… [truncated]", &input[..MAX_JOURNAL_INPUT_CHARS])
            } else {
                input.to_owned()
            };
            this.journal
                .append(JournalEvent::AgentStart {
                    agent_id: this.id.clone(),
                    input: journal_input,
                    session_id: session_id.as_ref().map(|id| id.as_str().to_owned()),
                })
                .await
                .map_err(|source| AgentError::JournalAppendFailed { source })?;

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
                &this.callbacks.before_agent,
                &ctx,
                input.to_owned(),
                "before_agent",
            )? {
                ChainResult::Abort(error) => {
                    return Ok(AgentRun {
                        output: String::new(),
                        final_response: None,
                        iterations: 0,
                        finish_reason: FinishReason::CallbackAborted,
                        conversation,
                        error: Some(error),
                        errors: Vec::new(),
                        structured_output: None,
                        #[cfg(feature = "python")]
                        parsed: None,
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
                RunLoopInputs {
                    template: rendered,
                    seed_messages,
                    conversation,
                    session_id: session_id.as_ref(),
                },
            )
            .await
        };

        let result = match this.run_config.timeout {
            Some(duration) => match tokio::time::timeout(duration, body).await {
                Ok(result) => result,
                Err(_elapsed) => Err(AgentError::Timeout { duration }),
            },
            None => body.await,
        };

        if let Err(e) = append_terminal_journal_event(this, &result).await {
            tracing::warn!("failed to append terminal journal event: {e}");
        }
        record_agent_outcome(&result);
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
    let providers = this.effective_providers(providers);
    let span = invoke_agent_span(this);
    async move {
        let result = async {
            this.journal
                .append(JournalEvent::AgentStart {
                    agent_id: this.id.clone(),
                    input: String::new(),
                    session_id: None,
                })
                .await
                .map_err(|source| AgentError::JournalAppendFailed { source })?;

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
                RunLoopInputs {
                    template: rendered,
                    seed_messages,
                    conversation: Conversation::new(),
                    session_id: None,
                },
            )
            .await
        };

        let result = match this.run_config.timeout {
            Some(duration) => match tokio::time::timeout(duration, result).await {
                Ok(result) => result,
                Err(_elapsed) => Err(AgentError::Timeout { duration }),
            },
            None => result.await,
        };

        if let Err(e) = append_terminal_journal_event(this, &result).await {
            tracing::warn!("failed to append terminal journal event: {e}");
        }
        record_agent_outcome(&result);
        result
    }
    .instrument(span)
    .await
}

/// Prepared per-run state handed from `run_prompt` into the bounded loop.
///
/// Built once after Prompt rendering and session seeding so `run_loop` owns
/// the conversation it mutates and never re-reads the session.
struct RunLoopInputs<'a> {
    /// Rendered provider request whose messages are replaced each iteration.
    template: ProviderRequest,
    /// Native seed messages prepended to every dispatched conversation.
    seed_messages: Vec<skald_spec::MessageNum>,
    /// Conversation accumulated across model and tool turns.
    conversation: Conversation,
    /// Session whose turn is appended on success, when the run is session-bound.
    session_id: Option<&'a SessionId>,
}

/// Runs the bounded model/tool loop inside the caller's `invoke_agent` span.
///
/// Each iteration dispatches under a `chat` span, journals the call and
/// result, and runs requested tools concurrently under `execute_tool` spans
/// until the model stops calling tools or the iteration budget is spent.
///
/// # Errors
///
/// Returns `AgentError` for a zero or exhausted iteration budget, a provider,
/// tool, journal, session, or structured-output failure.
async fn run_loop(
    this: &Agent,
    providers: &ProviderRegistry,
    inputs: RunLoopInputs<'_>,
) -> AgentResult<AgentRun> {
    let RunLoopInputs {
        template,
        seed_messages,
        mut conversation,
        session_id,
    } = inputs;
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
            &this.callbacks.before_model,
            &ctx,
            request.clone(),
            "before_model",
        )? {
            ChainResult::Abort(error) => {
                append_model_journal_call_result(
                    this,
                    ModelJournalCallRecord {
                        iteration,
                        request: &request,
                        finish_reason: "callback_aborted".to_owned(),
                        synthetic: true,
                    },
                )
                .await?;
                return Ok(AgentRun {
                    output: String::new(),
                    final_response: None,
                    iterations: iteration + 1,
                    finish_reason: FinishReason::CallbackAborted,
                    conversation,
                    error: Some(error),
                    errors: Vec::new(),
                    structured_output: None,
                    #[cfg(feature = "python")]
                    parsed: None,
                });
            }
            ChainResult::Replaced(replacement) => replacement,
        };

        append_model_journal_call(this, iteration, &request).await?;
        let chat = chat_span(&request);
        let response = dispatch(providers, request).instrument(chat.clone()).await;
        match &response {
            Ok(response) => {
                let finish_reason = response_finish_reason(response);
                chat.record("gen_ai.response.finish_reasons", finish_reason.as_str());
                append_model_journal_result(this, iteration, finish_reason, false).await?;
            }
            Err(error) => {
                record_error(&chat, error.code());
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
        let response = match apply_chain_with_panic_catch(
            &this.callbacks.after_model,
            &ctx,
            response,
            "after_model",
        )? {
            ChainResult::Abort(_) => {
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
            let structured_output = parse_structured_output(this, &output)?;
            let run = AgentRun {
                output,
                final_response: Some(std::sync::Arc::new(response)),
                iterations: iteration + 1,
                finish_reason: FinishReason::ModelStopped,
                conversation,
                error: None,
                errors: Vec::new(),
                structured_output,
                #[cfg(feature = "python")]
                parsed: None,
            };
            return match apply_chain_with_panic_catch(
                &this.callbacks.after_agent,
                &ctx,
                run,
                "after_agent",
            )? {
                ChainResult::Abort(_) => {
                    unreachable!("after_agent callbacks cannot skip a completed agent run")
                }
                ChainResult::Replaced(replacement) => Ok(replacement),
            };
        }

        validate_tools_attached(this, &tool_calls)?;
        let tools_snapshot = this.tools.clone();
        let before_tool = this.callbacks.before_tool.clone();
        let after_tool = this.callbacks.after_tool.clone();
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
                let span = info_span!(
                    "execute_tool",
                    gen_ai.operation.name = "execute_tool",
                    gen_ai.tool.name = %call.name,
                    gen_ai.tool.call.id = %call.id,
                    error.r#type = field::Empty,
                    otel.status_code = field::Empty,
                );
                let tool_span = span.clone();
                async move {
                    let tool = tools_snapshot
                        .iter()
                        .find(|tool| tool.name() == call.name)
                        .cloned()
                        .ok_or_else(|| AgentError::ToolNotInAgent {
                            name: call.name.clone(),
                        })?;
                    journal
                        .append(JournalEvent::ToolCall {
                            iteration,
                            call_id: call.id.clone(),
                            tool_name: call.name.clone(),
                            args: redact_tool_args(call.args.clone()),
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
                        ChainResult::Abort(error) => {
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
                                    ok: false,
                                    content: serde_json::json!({
                                        "code": error.code(),
                                        "error": error.to_string(),
                                    }),
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
                        ChainResult::Abort(_) => {
                            unreachable!("after_tool callbacks cannot skip a completed tool call")
                        }
                        ChainResult::Replaced(replacement) => replacement,
                    };
                    let (ok, content) = match result {
                        Ok(value) => (true, value),
                        Err(error) => {
                            record_error(&tool_span, &error.code());
                            (
                                false,
                                serde_json::json!({
                                    "code": error.code(),
                                    "error": error.to_string(),
                                }),
                            )
                        }
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
                    if ok && let Some(session_id) = session_id.as_ref() {
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
                    Ok((
                        idx,
                        ConversationTurn::ToolResult {
                            call_id: call.id,
                            ok,
                            content,
                        },
                    ))
                }
                .instrument(span)
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
        let request = &self.prompt.native().request;
        for turn in recent {
            conversation.push(session_turn_to_conversation_turn(turn, request));
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

/// Open the `invoke_agent` span for one Agent run.
///
/// Carries the OpenTelemetry GenAI operation, agent, provider, and model
/// identifiers only; [`record_agent_outcome`] fills `error.type` with the
/// stable Agent error code when the run fails. No prompt or input payload is
/// recorded.
fn invoke_agent_span(this: &Agent) -> Span {
    let request = &this.prompt.native().request;
    info_span!(
        "invoke_agent",
        gen_ai.operation.name = "invoke_agent",
        gen_ai.agent.id = %this.id,
        gen_ai.provider.name = %provider_label(&request.provider()),
        gen_ai.request.model = request_model(request).unwrap_or_default(),
        error.r#type = field::Empty,
        otel.status_code = field::Empty,
    )
}

/// Open the `chat` span for one provider dispatch of `request`.
///
/// Records the provider and requested model; the caller records the finish
/// reason on success or the stable provider error code on failure.
fn chat_span(request: &ProviderRequest) -> Span {
    info_span!(
        "chat",
        gen_ai.operation.name = "chat",
        gen_ai.provider.name = %provider_label(&request.provider()),
        gen_ai.request.model = request_model(request).unwrap_or_default(),
        gen_ai.response.finish_reasons = field::Empty,
        error.r#type = field::Empty,
        otel.status_code = field::Empty,
    )
}

/// Record the run's stable error code on the current `invoke_agent` span.
fn record_agent_outcome(result: &AgentResult<AgentRun>) {
    if let Err(error) = result {
        record_error(&Span::current(), error.code());
    }
}

/// Mark `span` failed with the stable error `code` as its `error.type`.
fn record_error(span: &Span, code: &str) {
    span.record("error.type", code);
    span.record("otel.status_code", "ERROR");
}

/// One model call and its result, journaled together after dispatch.
struct ModelJournalCallRecord<'a> {
    /// Loop iteration that issued the call.
    iteration: u32,
    /// Request whose provider and model are journaled.
    request: &'a ProviderRequest,
    /// Provider finish reason recorded for the result.
    finish_reason: String,
    /// Whether the result was synthesized rather than returned by a provider.
    synthetic: bool,
}

/// Journals a `ModelCall` followed by its `ModelResult`.
///
/// # Errors
///
/// Returns `AgentError::JournalAppendFailed` when either append fails; the
/// result is not appended when the call append fails.
async fn append_model_journal_call_result(
    this: &Agent,
    record: ModelJournalCallRecord<'_>,
) -> AgentResult<()> {
    let ModelJournalCallRecord {
        iteration,
        request,
        finish_reason,
        synthetic,
    } = record;
    append_model_journal_call(this, iteration, request).await?;
    append_model_journal_result(this, iteration, finish_reason, synthetic).await
}

/// Journals the provider and model of one dispatched request.
///
/// # Errors
///
/// Returns `AgentError::JournalAppendFailed` when the journal rejects the event.
async fn append_model_journal_call(
    this: &Agent,
    iteration: u32,
    request: &ProviderRequest,
) -> AgentResult<()> {
    this.journal
        .append(JournalEvent::ModelCall {
            iteration,
            provider: provider_label(&request.provider()),
            model: request_model(request).unwrap_or_default().to_owned(),
        })
        .await
        .map_err(|source| AgentError::JournalAppendFailed { source })
}

/// Journals the finish reason of one model result.
///
/// # Errors
///
/// Returns `AgentError::JournalAppendFailed` when the journal rejects the event.
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
        ProviderRequest::OpenAiChatCompletion(request)
        | ProviderRequest::OpenAiChatCompatible { request, .. } => Some(&request.model),
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

fn parse_structured_output(
    this: &Agent,
    output: &str,
) -> AgentResult<Option<serde_json::Map<String, serde_json::Value>>> {
    match &this.prompt.native().response_type {
        ResponseType::Text => Ok(None),
        ResponseType::JsonSchema { .. } => {
            let value: serde_json::Value =
                serde_json::from_str(output.trim()).map_err(|error| {
                    AgentError::StructuredOutputDecode {
                        agent: this.id.clone(),
                        detail: error.to_string(),
                    }
                })?;
            let serde_json::Value::Object(map) = value else {
                return Err(AgentError::StructuredOutputDecode {
                    agent: this.id.clone(),
                    detail: "structured output must be a JSON object".to_owned(),
                });
            };
            Ok(Some(map))
        }
    }
}

const REDACTED: &str = "<REDACTED>";
const SENSITIVE_FIELDS: &[&str] = &["password", "api_key", "secret", "token", "authorization"];

fn redact_tool_args(mut value: serde_json::Value) -> serde_json::Value {
    redact_in_place(&mut value);
    value
}

fn redact_in_place(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(map) => {
            for (key, val) in map.iter_mut() {
                if SENSITIVE_FIELDS.contains(&key.to_lowercase().as_str()) {
                    *val = serde_json::Value::String(REDACTED.to_owned());
                } else {
                    redact_in_place(val);
                }
            }
        }
        serde_json::Value::Array(items) => {
            for item in items {
                redact_in_place(item);
            }
        }
        _ => {}
    }
}
