//! Bounded tool loop for `skald-agent`.

use skald_prompt::Prompt;
use skald_runtime::{ProviderRegistry, dispatch};
use skald_spec::{MessageNum, ProviderRequest};
use tracing::{Instrument, debug_span};

use crate::agent::Agent;
use crate::error::{AgentError, AgentResult};
use crate::request_builder::reset_messages;
use crate::run::AgentRun;

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
        let prompt = this
            .prompt
            .with_user(input)
            .map_err(|err| AgentError::Prompt {
                agent: this.id.clone(),
                detail: err.to_string(),
            })?;
        let rendered = prompt
            .render_native(&[])
            .map_err(|err| AgentError::Prompt {
                agent: this.id.clone(),
                detail: err.to_string(),
            })?;
        let seed_messages = crate::request_builder::extract_messages(&this.id, &rendered)?;
        run_loop(this, providers, Some(rendered), seed_messages).await
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
        let seed_messages = crate::request_builder::extract_messages(&this.id, &rendered)?;
        run_loop(this, providers, Some(rendered), seed_messages).await
    }
    .instrument(span)
    .await
}

async fn run_loop(
    this: &Agent,
    providers: &ProviderRegistry,
    template: Option<ProviderRequest>,
    seed_messages: Vec<MessageNum>,
) -> AgentResult<AgentRun> {
    if this.run_config.max_iterations == 0 {
        return Err(AgentError::max_iterations(
            &this.id,
            this.run_config.max_iterations,
        ));
    }

    let request = match &template {
        Some(template) => reset_messages(template.clone(), &seed_messages)?,
        None => {
            return Err(AgentError::Prompt {
                agent: this.id.clone(),
                detail: "agent has no template request to run".to_owned(),
            });
        }
    };
    let response = dispatch(providers, request)
        .await
        .map_err(AgentError::Provider)?;

    let calls = response.adapter().tool_calls();
    if calls.is_empty() {
        let finish = response.adapter().finish_reason();
        return Ok(AgentRun {
            output: response,
            iterations: 1,
            finish,
        });
    }

    Err(AgentError::ToolNotFound {
        name: calls[0].name.clone().into_owned(),
    })
}
