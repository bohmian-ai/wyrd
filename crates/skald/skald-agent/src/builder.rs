//! Programmatic agent construction for callers with a live provider.

use std::sync::Arc;

use skald_runtime::Provider;
use skald_spec::ProviderName;

use crate::Agent;
use crate::error::AgentResult;
use crate::observer::{NoopObserver, Observer};
use crate::registry::system_messages;
use crate::run::RunConfig;
use crate::tool::AgentTool;

/// Programmatic builder for callers who already hold an [`Arc<dyn Provider>`].
pub struct AgentBuilder {
    id: String,
    provider_name: ProviderName,
    provider: Arc<dyn Provider>,
    system_prompt: Option<String>,
    model_override: Option<String>,
    tools: Vec<Arc<dyn AgentTool>>,
    run_config: RunConfig,
    observer: Arc<dyn Observer>,
}

impl AgentBuilder {
    /// Starts a builder pointing at a live provider client.
    pub fn new(
        id: impl Into<String>,
        provider_name: ProviderName,
        provider: Arc<dyn Provider>,
    ) -> Self {
        Self {
            id: id.into(),
            provider_name,
            provider,
            system_prompt: None,
            model_override: None,
            tools: Vec::new(),
            run_config: RunConfig::default(),
            observer: Arc::new(NoopObserver),
        }
    }

    /// Sets the system prompt rendered to native messages on [`Self::build`].
    #[must_use]
    pub fn with_system_prompt(mut self, prompt: impl Into<String>) -> Self {
        self.system_prompt = Some(prompt.into());
        self
    }

    /// Overrides the model id used by the later loop.
    #[must_use]
    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model_override = Some(model.into());
        self
    }

    /// Adds a tool to the agent's executable tool set.
    #[must_use]
    pub fn with_tool(mut self, tool: Arc<dyn AgentTool>) -> Self {
        self.tools.push(tool);
        self
    }

    /// Replaces the run config.
    #[must_use]
    pub fn with_run_config(mut self, run_config: RunConfig) -> Self {
        self.run_config = run_config;
        self
    }

    /// Replaces the observer.
    #[must_use]
    pub fn with_observer(mut self, observer: Arc<dyn Observer>) -> Self {
        self.observer = observer;
        self
    }

    /// Builds the agent, shaping the system prompt for the bound provider.
    pub fn build(self) -> AgentResult<Agent> {
        let system_instruction = match self.system_prompt.as_deref() {
            Some(text) => system_messages(text, &self.provider_name)?,
            None => Vec::new(),
        };

        Ok(Agent {
            id: self.id,
            provider_name: self.provider_name,
            provider: self.provider,
            system_instruction,
            model_override: self.model_override,
            tools: self.tools,
            run_config: self.run_config,
            observer: self.observer,
        })
    }
}
