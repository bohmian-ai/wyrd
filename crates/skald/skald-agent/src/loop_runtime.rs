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

impl Agent {
    /// Run the bounded tool loop for `input` against a live provider
    /// registry, optionally continuing session `session_id`.
    ///
    /// The run journals `AgentStart`, seeds recent session turns, appends the
    /// user turn to the session, applies `before_agent` callbacks, renders the
    /// Agent's own Prompt, and drives the bounded loop inside one
    /// `invoke_agent` span. The Agent's provider override, when set, replaces
    /// `providers`.
    ///
    /// When [`RunConfig::timeout`](crate::RunConfig) is set, everything up to
    /// the loop result is bounded by it; on expiry the in-flight provider or
    /// tool work is dropped and the run fails with [`AgentError::Timeout`].
    /// The terminal `AgentFinish` or `AgentError` journal append happens after
    /// that bound and is best effort: its failure is logged and never changes
    /// the result. A caller that must bound settlement too, such as a
    /// Workflow attempt, races this future against its own fixed deadline and
    /// drops it. Dropping the future cancels all outstanding work; events and
    /// session turns already appended stay recorded.
    ///
    /// # Errors
    ///
    /// Returns [`AgentError::JournalAppendFailed`] when a non-terminal journal
    /// append fails, session read or append failures, callback panics, Prompt
    /// rendering and loop-shape errors, provider and tool failures, structured
    /// output decode failures, an exhausted iteration budget, or
    /// [`AgentError::Timeout`].
    pub async fn run_with(
        &self,
        providers: &ProviderRegistry,
        session_id: Option<SessionId>,
        input: &str,
    ) -> AgentResult<AgentRun> {
        let providers = self.effective_providers(providers);
        let span = self.invoke_agent_span(&self.prompt);
        async move {
            const MAX_JOURNAL_INPUT_CHARS: usize = 4096;

            let body = async {
                let journal_input = if input.len() > MAX_JOURNAL_INPUT_CHARS {
                    format!("{}… [truncated]", &input[..MAX_JOURNAL_INPUT_CHARS])
                } else {
                    input.to_owned()
                };
                self.journal
                    .append(JournalEvent::AgentStart {
                        agent_id: self.id.clone(),
                        input: journal_input,
                        session_id: session_id.as_ref().map(|id| id.as_str().to_owned()),
                    })
                    .await
                    .map_err(|source| AgentError::JournalAppendFailed { source })?;

                let mut conversation = Conversation::new();
                if let Some(session_id) = session_id.as_ref() {
                    self.seed_session_recent(session_id, &mut conversation)
                        .await?;
                }
                conversation.append_user(input);
                self.append_session_turn(
                    session_id.as_ref(),
                    SessionTurn {
                        role: Role::User,
                        content: input.to_owned(),
                        call_id: None,
                    },
                )
                .await?;
                let ctx = self.agent_context_with_session(session_id.as_ref(), 0, &conversation);
                match apply_before_agent_chain_with_panic_catch(
                    &self.callbacks.before_agent,
                    &ctx,
                    input.to_owned(),
                    "before_agent",
                )? {
                    ChainResult::Abort(error, _) => {
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
                    ChainResult::Replaced(input) => {
                        replace_seed_user_turn(&mut conversation, input)
                    }
                }
                let rendered =
                    self.prompt
                        .render_native(&[])
                        .map_err(|err| AgentError::Prompt {
                            agent: self.id.clone(),
                            detail: err.to_string(),
                        })?;
                let seed_messages = extract_messages(&self.id, &rendered)?;
                self.run_loop(
                    providers,
                    RunLoopInputs {
                        template: rendered,
                        seed_messages,
                        conversation,
                        session_id: session_id.as_ref(),
                        model: &self.prompt.native().model,
                    },
                )
                .await
            };

            let result = match self.run_config.timeout {
                Some(duration) => match tokio::time::timeout(duration, body).await {
                    Ok(result) => result,
                    Err(_elapsed) => Err(AgentError::Timeout { duration }),
                },
                None => body.await,
            };

            if let Err(e) = self.append_terminal_journal_event(&result).await {
                tracing::warn!("failed to append terminal journal event: {e}");
            }
            record_agent_outcome(&result);
            result
        }
        .instrument(span)
        .await
    }

    /// Run the bounded tool loop driven by explicit `prompt` rendered with
    /// variable substitutions `vars`, with no session.
    ///
    /// The run journals `AgentStart`, renders `prompt`, refuses a provider
    /// that differs from the Agent's own Prompt, and drives
    /// the bounded loop inside one `invoke_agent` span whose model is the
    /// explicit Prompt's model. The Agent's provider override, when set,
    /// replaces `providers`.
    ///
    /// Timeout, cancellation, and terminal-journal behavior match
    /// [`Self::run_with`]: the optional run timeout bounds the loop, the
    /// terminal journal append afterward is best effort, and dropping the
    /// future cancels outstanding work.
    ///
    /// # Errors
    ///
    /// Returns [`AgentError::JournalAppendFailed`] when a non-terminal journal
    /// append fails, [`AgentError::Prompt`] when rendering fails,
    /// [`AgentError::ProviderMismatch`] when `prompt` targets another provider,
    /// loop-shape, provider, tool, and structured-output failures, an
    /// exhausted iteration budget, or [`AgentError::Timeout`].
    pub async fn run_prompt(
        &self,
        providers: &ProviderRegistry,
        prompt: &Prompt,
        vars: &[(&str, &str)],
    ) -> AgentResult<AgentRun> {
        let providers = self.effective_providers(providers);
        let span = self.invoke_agent_span(prompt);
        async move {
            let result = async {
                self.journal
                    .append(JournalEvent::AgentStart {
                        agent_id: self.id.clone(),
                        input: String::new(),
                        session_id: None,
                    })
                    .await
                    .map_err(|source| AgentError::JournalAppendFailed { source })?;

                let rendered = prompt
                    .render_native(vars)
                    .map_err(|err| AgentError::Prompt {
                        agent: self.id.clone(),
                        detail: err.to_string(),
                    })?;
                let request_provider = prompt.native().provider();
                let agent_provider = self.prompt.native().provider();
                if request_provider != agent_provider {
                    return Err(AgentError::ProviderMismatch {
                        agent: self.id.clone(),
                        agent_provider,
                        prompt_provider: request_provider,
                    });
                }
                let seed_messages = extract_messages(&self.id, &rendered)?;
                self.run_loop(
                    providers,
                    RunLoopInputs {
                        template: rendered,
                        seed_messages,
                        conversation: Conversation::new(),
                        session_id: None,
                        model: &prompt.native().model,
                    },
                )
                .await
            };

            let result = match self.run_config.timeout {
                Some(duration) => match tokio::time::timeout(duration, result).await {
                    Ok(result) => result,
                    Err(_elapsed) => Err(AgentError::Timeout { duration }),
                },
                None => result.await,
            };

            if let Err(e) = self.append_terminal_journal_event(&result).await {
                tracing::warn!("failed to append terminal journal event: {e}");
            }
            record_agent_outcome(&result);
            result
        }
        .instrument(span)
        .await
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
        &self,
        providers: &ProviderRegistry,
        inputs: RunLoopInputs<'_>,
    ) -> AgentResult<AgentRun> {
        let RunLoopInputs {
            template,
            seed_messages,
            mut conversation,
            session_id,
            model,
        } = inputs;
        if self.run_config.max_iterations == 0 {
            return Err(AgentError::max_iterations(
                &self.id,
                self.run_config.max_iterations,
            ));
        }

        let concurrency_cap = self.run_config.tool_concurrency_cap.unwrap_or(8).max(1);
        let max_iterations = self.run_config.max_iterations;

        for iteration in 0..max_iterations {
            self.journal
                .append(JournalEvent::Iteration { index: iteration })
                .await
                .map_err(|source| AgentError::JournalAppendFailed { source })?;

            let ctx = self.agent_context_with_session(session_id, iteration, &conversation);
            let mut request = request_from_conversation(
                &self.id,
                template.clone(),
                &seed_messages,
                &conversation,
            )?;
            if !self.tools.is_empty() {
                request = request.with_tools(tool_descriptors(&self.tools));
            }

            request = match apply_chain_with_panic_catch(
                &self.callbacks.before_model,
                &ctx,
                request.clone(),
                "before_model",
            )? {
                ChainResult::Abort(error, _) => {
                    self.append_model_journal_call_result(ModelJournalCallRecord {
                        iteration,
                        request: &request,
                        finish_reason: "callback_aborted".to_owned(),
                        synthetic: true,
                    })
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

            self.append_model_journal_call(iteration, &request).await?;
            let provider = self.prompt.native().provider();
            let chat = chat_span(&provider, &request, model);
            let response = dispatch(providers, provider, request)
                .instrument(chat.clone())
                .await;
            match &response {
                Ok(response) => {
                    let finish_reason = response_finish_reason(response);
                    self.append_model_journal_result(iteration, finish_reason, false)
                        .await?;
                }
                Err(error) => {
                    record_error(&chat, error.code());
                    self.append_model_journal_result(
                        iteration,
                        format!("provider_error:{error}"),
                        true,
                    )
                    .await?;
                }
            }
            let response = response.map_err(AgentError::Provider)?;
            let response = match apply_chain_with_panic_catch(
                &self.callbacks.after_model,
                &ctx,
                response,
                "after_model",
            )? {
                ChainResult::Abort(error, _) => {
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
            let assistant = assistant_message(&self.id, &response)?;
            conversation.append_assistant(assistant);
            self.append_session_turn(
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
                let structured_output = self.parse_structured_output(&output)?;
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
                    &self.callbacks.after_agent,
                    &ctx,
                    run,
                    "after_agent",
                )? {
                    ChainResult::Abort(error, held) => Ok(AgentRun {
                        output: String::new(),
                        final_response: None,
                        finish_reason: FinishReason::CallbackAborted,
                        error: Some(error),
                        structured_output: None,
                        #[cfg(feature = "python")]
                        parsed: None,
                        ..held
                    }),
                    ChainResult::Replaced(replacement) => Ok(replacement),
                };
            }

            self.validate_tools_attached(&tool_calls)?;
            let tools_snapshot = self.tools.clone();
            let before_tool = self.callbacks.before_tool.clone();
            let after_tool = self.callbacks.after_tool.clone();
            let journal = Arc::clone(&self.journal);
            let session = Arc::clone(&self.session);
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
                            ChainResult::Abort(error, _) => {
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
                        let (ok, content) = match apply_chain_with_panic_catch_tool_result(
                            &after_tool,
                            &ctx,
                            tool.as_ref(),
                            result,
                            "after_tool",
                        )? {
                            ChainResult::Replaced(Ok(value)) => (true, value),
                            ChainResult::Replaced(Err(error)) => {
                                record_error(&tool_span, &error.code());
                                (
                                    false,
                                    serde_json::json!({
                                        "code": error.code(),
                                        "error": error.to_string(),
                                    }),
                                )
                            }
                            ChainResult::Abort(error, _) => {
                                record_error(&tool_span, error.code());
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

        Err(AgentError::max_iterations(&self.id, max_iterations))
    }

    /// Seed `conversation` with the most recent turns of `session_id`.
    ///
    /// Reads at most `session_recent_limit` turns (default 50) and converts
    /// each into the Agent Prompt's provider dialect.
    ///
    /// # Errors
    ///
    /// Returns [`AgentError::SessionRecentFailed`] when the session store
    /// cannot be read.
    async fn seed_session_recent(
        &self,
        session_id: &SessionId,
        conversation: &mut Conversation,
    ) -> AgentResult<()> {
        let limit = self.run_config.session_recent_limit.unwrap_or(50);
        let recent = self
            .session
            .recent(session_id, limit)
            .await
            .map_err(|source| AgentError::SessionRecentFailed {
                session_id: session_id.as_str().to_owned(),
                source,
            })?;
        let request = &self.prompt.native().request;
        for turn in recent {
            conversation.push(session_turn_to_conversation_turn(turn, request));
        }
        Ok(())
    }

    /// Append `turn` to session `session_id` when the run is session-bound;
    /// a run without a session appends nothing.
    ///
    /// # Errors
    ///
    /// Returns [`AgentError::SessionAppendFailed`] when the session store
    /// rejects the turn.
    async fn append_session_turn(
        &self,
        session_id: Option<&SessionId>,
        turn: SessionTurn,
    ) -> AgentResult<()> {
        if let Some(session_id) = session_id {
            self.session
                .append(session_id, turn)
                .await
                .map_err(|source| AgentError::SessionAppendFailed {
                    session_id: session_id.as_str().to_owned(),
                    source,
                })?;
        }
        Ok(())
    }

    /// Journal the run's terminal event: `AgentFinish` with the finish reason
    /// and iteration count on success, or `AgentError` with the stable code
    /// and message on failure.
    ///
    /// Callers treat this append as best effort and log its failure; it is
    /// not bounded by the run timeout, so a caller that needs bounded
    /// settlement drops the run future.
    ///
    /// # Errors
    ///
    /// Returns [`AgentError::JournalAppendFailed`] when the journal rejects the
    /// event.
    async fn append_terminal_journal_event(
        &self,
        result: &AgentResult<AgentRun>,
    ) -> AgentResult<()> {
        let event = match result {
            Ok(run) => JournalEvent::AgentFinish {
                agent_id: self.id.clone(),
                finish_reason: format!("{:?}", run.finish_reason).to_lowercase(),
                iterations: run.iterations,
            },
            Err(error) => JournalEvent::AgentError {
                agent_id: self.id.clone(),
                code: error.code().to_owned(),
                message: error.to_string(),
            },
        };
        self.journal
            .append(event)
            .await
            .map_err(|source| AgentError::JournalAppendFailed { source })
    }

    /// Open the `invoke_agent` span for one Agent run driven by `prompt`.
    ///
    /// Carries the OpenTelemetry GenAI operation, agent ID, semantic provider
    /// name of the Prompt's effective dispatch target (`Prompt::provider`,
    /// not the request schema's default), and the Prompt's resolved model only; [`record_agent_outcome`] fills `error.type` with the stable
    /// Agent error code when the run fails. No prompt or input payload is
    /// recorded.
    fn invoke_agent_span(&self, prompt: &Prompt) -> Span {
        let prompt = prompt.native();
        info_span!(
            "invoke_agent",
            gen_ai.operation.name = "invoke_agent",
            gen_ai.agent.id = %self.id,
            gen_ai.provider.name = genai_provider_name(&prompt.provider()),
            gen_ai.request.model = prompt.model.as_str(),
            error.r#type = field::Empty,
            otel.status_code = field::Empty,
        )
    }

    /// Journals a `ModelCall` followed by its `ModelResult`.
    ///
    /// # Errors
    ///
    /// Returns `AgentError::JournalAppendFailed` when either append fails; the
    /// result is not appended when the call append fails.
    async fn append_model_journal_call_result(
        &self,
        record: ModelJournalCallRecord<'_>,
    ) -> AgentResult<()> {
        let ModelJournalCallRecord {
            iteration,
            request,
            finish_reason,
            synthetic,
        } = record;
        self.append_model_journal_call(iteration, request).await?;
        self.append_model_journal_result(iteration, finish_reason, synthetic)
            .await
    }

    /// Journals the provider and model of one dispatched request.
    ///
    /// # Errors
    ///
    /// Returns `AgentError::JournalAppendFailed` when the journal rejects the event.
    async fn append_model_journal_call(
        &self,
        iteration: u32,
        request: &ProviderRequest,
    ) -> AgentResult<()> {
        self.journal
            .append(JournalEvent::ModelCall {
                iteration,
                provider: provider_label(&self.prompt.native().provider()),
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
        &self,
        iteration: u32,
        finish_reason: String,
        synthetic: bool,
    ) -> AgentResult<()> {
        self.journal
            .append(JournalEvent::ModelResult {
                iteration,
                finish_reason,
                synthetic,
            })
            .await
            .map_err(|source| AgentError::JournalAppendFailed { source })
    }

    /// Check that every tool the model called is attached to this Agent.
    ///
    /// # Errors
    ///
    /// Returns [`AgentError::ToolNotInAgent`] for the first unattached tool.
    fn validate_tools_attached(&self, calls: &[ToolCall]) -> AgentResult<()> {
        for call in calls {
            if !self.tools.iter().any(|tool| tool.name() == call.name) {
                return Err(AgentError::ToolNotInAgent {
                    name: call.name.clone(),
                });
            }
        }
        Ok(())
    }

    /// Build the callback context for `iteration`, carrying the Agent ID,
    /// optional session ID, and a snapshot of `conversation`.
    fn agent_context_with_session(
        &self,
        session_id: Option<&SessionId>,
        iteration: u32,
        conversation: &Conversation,
    ) -> AgentContext {
        AgentContext {
            agent_id: self.id.clone(),
            session_id: session_id.map(|id| id.as_str().to_owned()),
            iteration,
            conversation: Arc::new(conversation.clone()),
        }
    }

    /// Decode the final `output` text as the structured object a JSON-schema
    /// Prompt declares; a text Prompt yields `None`.
    ///
    /// # Errors
    ///
    /// Returns [`AgentError::StructuredOutputDecode`] when the text is not
    /// JSON or not a JSON object.
    fn parse_structured_output(
        &self,
        output: &str,
    ) -> AgentResult<Option<serde_json::Map<String, serde_json::Value>>> {
        match &self.prompt.native().response_type {
            ResponseType::Text => Ok(None),
            ResponseType::JsonSchema { .. } => {
                let value: serde_json::Value =
                    serde_json::from_str(output.trim()).map_err(|error| {
                        AgentError::StructuredOutputDecode {
                            agent: self.id.clone(),
                            detail: error.to_string(),
                        }
                    })?;
                let serde_json::Value::Object(map) = value else {
                    return Err(AgentError::StructuredOutputDecode {
                        agent: self.id.clone(),
                        detail: "structured output must be a JSON object".to_owned(),
                    });
                };
                Ok(Some(map))
            }
        }
    }
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
    /// Resolved model of the Prompt driving the run; the `chat` span fallback
    /// for request wire shapes that carry no model.
    model: &'a str,
}

/// Open the `chat` span for one dispatch of the effective `request` to `provider`.
///
/// Records the semantic name of the Prompt's dispatch target and the model the
/// request is actually made to: the request's own model, which reflects any
/// `before_model` replacement, or the Prompt's resolved `fallback_model` for
/// wire shapes that omit it (Gemini, Vertex). The caller records the stable
/// provider error code on failure. The optional
/// `string[]` finish-reasons attribute is omitted because `tracing` fields are
/// scalar; finish reasons stay in the journal.
fn chat_span(provider: &ProviderName, request: &ProviderRequest, fallback_model: &str) -> Span {
    info_span!(
        "chat",
        gen_ai.operation.name = "chat",
        gen_ai.provider.name = genai_provider_name(provider),
        gen_ai.request.model = request_model(request).unwrap_or(fallback_model),
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
        | ProviderRequest::VertexPredict(_)
        | ProviderRequest::RawV1 { .. } => None,
        _ => None,
    }
}

/// OpenTelemetry GenAI `gen_ai.provider.name` value for `provider`.
///
/// Uses the semantic-convention well-known values; a custom provider keeps
/// its own name.
fn genai_provider_name(provider: &ProviderName) -> &str {
    match provider {
        ProviderName::OpenAi => "openai",
        ProviderName::Anthropic => "anthropic",
        ProviderName::Google => "gcp.gemini",
        ProviderName::Vertex => "gcp.vertex_ai",
        ProviderName::Custom(name) => name,
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
        ProviderResponse::GeminiGenerateContent(response) => response
            .candidates
            .first()
            .and_then(|candidate| candidate.finish_reason.as_ref())
            .map(|reason| format!("{reason:?}").to_lowercase())
            .unwrap_or_else(|| "other".to_owned()),
        _ => format!("{:?}", response.adapter().finish_reason()).to_lowercase(),
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
