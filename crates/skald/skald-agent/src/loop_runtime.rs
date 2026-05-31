//! Bounded tool loop for `skald-agent`.

use std::sync::Arc;

use futures::future::join_all;
use serde_json::value::RawValue;
use skald_runtime::SkaldRuntimeError;
use skald_spec::wire::anthropic_messages::{AnthropicContentBlock, AnthropicMessage};
use skald_spec::wire::google_generate::{GoogleContent, GooglePart};
use skald_spec::wire::openai_chat::{OpenAiChatMessage, OpenAiMessageContent};
use skald_spec::{MessageNum, ProviderName, ProviderRequest};
use tracing::{Instrument, debug_span};

use crate::agent::Agent;
use crate::error::{AgentError, AgentResult};
use crate::messages::{assistant_message_from_response, tool_result_message};
use crate::request_builder::{build_request, reset_messages};
use crate::run::AgentRun;
use crate::tool::AgentTool;

impl Agent {
    /// Run the bounded tool loop with the given user input.
    pub async fn run(&self, input: &str) -> AgentResult<AgentRun> {
        let span = debug_span!(
            "skald_agent.run",
            agent_id = self.id.as_str(),
            provider = ?self.provider_name,
            cap = self.run_config.max_iterations,
            entry = "input",
        );
        async move {
            self.observer
                .on_agent_start(&self.id, self.run_config.max_iterations);
            let outcome = async {
                let initial = initial_user_message(&self.provider_name, input)?;
                self.run_loop(None, vec![initial]).await
            }
            .await;
            self.emit_terminal_event(&outcome);
            outcome
        }
        .instrument(span)
        .await
    }

    /// Run the bounded tool loop driven by a rendered [`skald_spec::Prompt`].
    pub async fn run_prompt(
        &self,
        prompt: &skald_spec::Prompt,
        vars: &[(&str, &str)],
    ) -> AgentResult<AgentRun> {
        let span = debug_span!(
            "skald_agent.run",
            agent_id = self.id.as_str(),
            provider = ?self.provider_name,
            cap = self.run_config.max_iterations,
            entry = "prompt",
        );
        async move {
            self.observer
                .on_agent_start(&self.id, self.run_config.max_iterations);
            let outcome = async {
                let rendered = prompt.render(vars).map_err(|err| AgentError::Prompt {
                    agent: self.id.clone(),
                    detail: err.to_string(),
                })?;
                let request_provider = rendered.provider();
                if request_provider != self.provider_name {
                    return Err(AgentError::ProviderMismatch {
                        agent: self.id.clone(),
                        agent_provider: self.provider_name.clone(),
                        prompt_provider: request_provider,
                    });
                }
                let seed_messages = crate::request_builder::extract_messages(&self.id, &rendered)?;
                self.run_loop(Some(rendered), seed_messages).await
            }
            .await;
            self.emit_terminal_event(&outcome);
            outcome
        }
        .instrument(span)
        .await
    }

    fn emit_terminal_event(&self, outcome: &AgentResult<AgentRun>) {
        if let Err(err) = outcome {
            self.observer
                .on_agent_error(&self.id, err.code(), &err.to_string());
        }
    }

    async fn run_loop(
        &self,
        template: Option<ProviderRequest>,
        seed_messages: Vec<MessageNum>,
    ) -> AgentResult<AgentRun> {
        let mut messages = seed_messages;
        let mut iteration = 0;

        loop {
            if iteration >= self.run_config.max_iterations {
                return Err(AgentError::max_iterations(
                    &self.id,
                    self.run_config.max_iterations,
                ));
            }
            iteration += 1;
            self.observer.on_iteration(&self.id, iteration);

            let request = match &template {
                Some(template) if iteration == 1 => template.clone(),
                Some(template) => reset_messages(template.clone(), &messages)?,
                None => build_request(self, &messages)?,
            };
            let response = self.provider.send(request).await.map_err(|source| {
                AgentError::Provider(SkaldRuntimeError::from_provider(
                    self.provider_name.clone(),
                    source,
                ))
            })?;

            let view = response.adapter();
            let calls = view.tool_calls();
            if calls.is_empty() {
                let finish = view.finish_reason();
                self.observer
                    .on_agent_finish(&self.id, finish.clone(), iteration);
                return Ok(AgentRun {
                    output: response,
                    iterations: iteration,
                    finish,
                });
            }

            let assistant = assistant_message_from_response(&response, &self.provider_name)?;
            messages.push(assistant);

            let dispatched =
                join_all(calls.iter().map(|call| async {
                    let raw_args = RawValue::from_string(call.arguments.clone().into_owned())
                        .map_err(|err| AgentError::InvalidToolArgs {
                            tool: call.name.clone().into_owned(),
                            detail: err.to_string(),
                        })?;
                    self.observer.on_tool_call(&self.id, &call.name, &raw_args);
                    let tool = self.lookup_tool(call.name.as_ref())?;
                    let result = tool.call(&raw_args).await;
                    self.observer
                        .on_tool_result(&self.id, &call.name, result.is_ok());
                    tool_result_message(&self.provider_name, call, &result)
                }))
                .await;

            for outcome in dispatched {
                messages.push(outcome?);
            }
        }
    }

    fn lookup_tool(&self, name: &str) -> AgentResult<Arc<dyn AgentTool>> {
        self.tools
            .iter()
            .find(|tool| tool.name() == name)
            .cloned()
            .ok_or_else(|| AgentError::ToolNotFound {
                name: name.to_owned(),
            })
    }
}

fn initial_user_message(provider: &ProviderName, input: &str) -> AgentResult<MessageNum> {
    match provider {
        ProviderName::OpenAi => Ok(MessageNum::OpenAi(OpenAiChatMessage {
            role: "user".to_owned(),
            content: Some(OpenAiMessageContent::Text(input.to_owned())),
            name: None,
            tool_calls: None,
            tool_call_id: None,
            refusal: None,
        })),
        ProviderName::Anthropic => Ok(MessageNum::Anthropic(AnthropicMessage {
            role: "user".to_owned(),
            content: vec![AnthropicContentBlock::Text {
                text: input.to_owned(),
                cache_control: None,
                citations: None,
            }],
        })),
        ProviderName::Google | ProviderName::Vertex => Ok(MessageNum::Gemini(GoogleContent {
            role: "user".to_owned(),
            parts: vec![GooglePart::Text {
                text: input.to_owned(),
            }],
        })),
        ProviderName::Custom(_) => Err(AgentError::SystemPrompt {
            provider: provider.clone(),
            detail: "custom providers must construct initial messages directly".to_owned(),
        }),
    }
}
