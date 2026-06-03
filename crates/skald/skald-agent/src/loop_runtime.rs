//! Bounded tool loop for `skald-agent`.

use std::sync::Arc;

use futures::StreamExt;
use futures::stream;
use skald_prompt::Prompt;
use skald_runtime::{ProviderRegistry, dispatch};
use skald_spec::{ProviderRequest, ToolDescriptor};
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
use crate::request_builder::{assistant_message, extract_messages, request_from_conversation};
use crate::run::{AgentRun, FinishReason};

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
        let mut conversation = Conversation::new();
        conversation.append_user(input);
        let ctx = agent_context(this, 0, &conversation);
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
        run_loop(this, providers, rendered, seed_messages, conversation).await
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
        )
        .await
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
) -> AgentResult<AgentRun> {
    if this.run_config.max_iterations == 0 {
        return Err(AgentError::max_iterations(
            &this.id,
            this.run_config.max_iterations,
        ));
    }

    let concurrency_cap = this.run_config.tool_concurrency_cap.unwrap_or(8).max(1);

    for iteration in 0..this.run_config.max_iterations {
        let ctx = agent_context(this, iteration, &conversation);
        let mut request =
            request_from_conversation(&this.id, template.clone(), &seed_messages, &conversation)?;
        if !this.tools.is_empty() {
            request = request.with_tools(tool_descriptors(&this.tools));
        }

        request = match apply_chain_with_panic_catch(
            &this.before_model,
            &ctx,
            request,
            "before_model",
        )? {
            ChainResult::Skip => {
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

        let response = dispatch(providers, request)
            .await
            .map_err(AgentError::Provider)?;
        let response =
            match apply_chain_with_panic_catch(&this.after_model, &ctx, response, "after_model")? {
                ChainResult::Skip => {
                    unreachable!("after_model callbacks cannot skip a completed provider call")
                }
                ChainResult::Replaced(replacement) => replacement,
            };
        let assistant = assistant_message(&this.id, &response)?;
        conversation.append_assistant(assistant);

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
        let indexed_calls = tool_calls.into_iter().enumerate();
        let mut results: Vec<(usize, ConversationTurn)> = stream::iter(indexed_calls)
            .map(|(idx, call)| {
                let tools_snapshot = tools_snapshot.clone();
                let before_tool = before_tool.clone();
                let after_tool = after_tool.clone();
                let ctx = ctx.clone();
                async move {
                    let tool = tools_snapshot
                        .iter()
                        .find(|tool| tool.name() == call.name)
                        .cloned()
                        .expect("tool presence pre-validated");
                    let args = match apply_chain_with_panic_catch_tool(
                        &before_tool,
                        &ctx,
                        tool.as_ref(),
                        call.args,
                        "before_tool",
                    )? {
                        ChainResult::Skip => {
                            return Ok((
                                idx,
                                ConversationTurn::ToolResult {
                                    call_id: call.id,
                                    ok: true,
                                    content: serde_json::json!({ "skipped": true }),
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

    Err(AgentError::max_iterations(
        &this.id,
        this.run_config.max_iterations,
    ))
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

fn agent_context(this: &Agent, iteration: u32, conversation: &Conversation) -> AgentContext {
    AgentContext {
        agent_id: this.id.clone(),
        session_id: None,
        iteration,
        conversation: Arc::new(conversation.clone()),
    }
}

fn replace_seed_user_turn(conversation: &mut Conversation, input: String) {
    if let Some(ConversationTurn::User { content }) = conversation.turns.get_mut(0) {
        *content = input;
    }
}
