//! Live agent: identity and prompt-bound bounded loop configuration.
//!
//! Build an [`Agent`] directly once the resolved prompt is ready, then run the
//! bounded tool loop.

use std::fmt;
use std::sync::Arc;

use skald_prompt::Prompt;
use skald_runtime::ProviderRegistry;

use crate::error::AgentResult;
use crate::run::RunConfig;

/// Live agent ready to run the bounded tool loop.
#[derive(Clone)]
pub struct Agent {
    /// Stable id used for tracing and diagnostics.
    pub id: String,
    /// Resolved native prompt.
    ///
    /// Provider and model identities live in `prompt.request` and `prompt.model`.
    pub prompt: Arc<Prompt>,
    /// Loop execution settings.
    pub run_config: RunConfig,
}

impl fmt::Debug for Agent {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Agent")
            .field("id", &self.id)
            .field("prompt_model", &self.prompt.native().model)
            .field("run_config", &self.run_config)
            .finish()
    }
}

impl Agent {
    /// Builds a new runnable agent.
    pub fn new(id: impl Into<String>, prompt: Arc<Prompt>) -> Self {
        Self {
            id: id.into(),
            prompt,
            run_config: RunConfig::default(),
        }
    }

    /// Rebuilds the agent with a new prompt.
    pub fn with_prompt(mut self, prompt: Arc<Prompt>) -> Self {
        self.prompt = prompt;
        self
    }

    /// Rebuilds the agent with a new run configuration.
    pub fn with_run_config(mut self, run_config: RunConfig) -> Self {
        self.run_config = run_config;
        self
    }

    /// Run the bounded tool loop against a live provider registry.
    pub async fn run(
        &self,
        providers: &ProviderRegistry,
        input: &str,
    ) -> AgentResult<crate::run::AgentRun> {
        crate::loop_runtime::run(self, providers, input).await
    }

    /// Run the bounded tool loop driven by a rendered prompt with variable
    /// substitution.
    pub async fn run_prompt(
        &self,
        providers: &ProviderRegistry,
        prompt: &Prompt,
        vars: &[(&str, &str)],
    ) -> AgentResult<crate::run::AgentRun> {
        crate::loop_runtime::run_prompt(self, providers, prompt, vars).await
    }
}
