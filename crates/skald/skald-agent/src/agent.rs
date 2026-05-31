//! Live agent: identity, bound provider, system instruction, tools, run config.
//!
//! Built once via [`Agent::from_def`] or [`crate::AgentBuilder`], then immutable.
//! The bounded tool loop is added in a later stage.

use std::fmt;
use std::sync::Arc;

use skald_runtime::{Provider, ProviderRegistry};
use skald_spec::{MessageNum, ProviderName};

use crate::AgentDef;
use crate::error::{AgentError, AgentResult};
use crate::observer::{NoopObserver, Observer};
use crate::registry::system_messages;
use crate::run::RunConfig;
use crate::tool::{AgentTool, ToolRegistry};

/// Live agent ready to run the bounded tool loop.
pub struct Agent {
    /// Stable id matching the [`AgentDef::id`] it was built from.
    pub id: String,
    /// Provider name carried for diagnostics and cross-provider routing.
    pub provider_name: ProviderName,
    pub(crate) provider: Arc<dyn Provider>,
    pub(crate) system_instruction: Vec<MessageNum>,
    pub(crate) model_override: Option<String>,
    pub(crate) tools: Vec<Arc<dyn AgentTool>>,
    pub(crate) run_config: RunConfig,
    pub(crate) observer: Arc<dyn Observer>,
}

impl fmt::Debug for Agent {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Agent")
            .field("id", &self.id)
            .field("provider_name", &self.provider_name)
            .field("bound_provider", &self.provider.name())
            .field("system_instruction_len", &self.system_instruction.len())
            .field("model_override", &self.model_override)
            .field("tools_len", &self.tools.len())
            .field("run_config", &self.run_config)
            .field("observer_refs", &Arc::strong_count(&self.observer))
            .finish()
    }
}

impl Agent {
    /// Binds an [`AgentDef`] against provider and tool registries.
    ///
    /// This is the only stage where a declared provider name becomes a live
    /// provider client. Missing provider and tool names are returned as stable
    /// [`AgentError`] variants.
    pub async fn from_def(
        def: AgentDef,
        providers: &ProviderRegistry,
        tools: &ToolRegistry,
        observer: Arc<dyn Observer>,
    ) -> AgentResult<Self> {
        let provider =
            providers
                .get(&def.provider)
                .ok_or_else(|| AgentError::ProviderNotFound {
                    provider: def.provider.clone(),
                })?;

        let system_instruction = match def.system_prompt.as_deref() {
            Some(text) => system_messages(text, &def.provider)?,
            None => Vec::new(),
        };

        let mut resolved = Vec::with_capacity(def.tool_names.len());
        for name in &def.tool_names {
            let tool = tools
                .get(name)
                .ok_or_else(|| AgentError::ToolNotFound { name: name.clone() })?;
            resolved.push(tool);
        }

        Ok(Self {
            id: def.id,
            provider_name: def.provider,
            provider,
            system_instruction,
            model_override: def.model,
            tools: resolved,
            run_config: def.run_config,
            observer,
        })
    }

    /// Binds an [`AgentDef`] with a [`NoopObserver`].
    pub async fn from_def_noop(
        def: AgentDef,
        providers: &ProviderRegistry,
        tools: &ToolRegistry,
    ) -> AgentResult<Self> {
        Self::from_def(def, providers, tools, Arc::new(NoopObserver)).await
    }

    /// Borrowed view over the system instruction messages.
    pub fn system_instruction(&self) -> &[MessageNum] {
        &self.system_instruction
    }

    /// Returns the resolved tool declarations the loop may dispatch.
    pub fn tools(&self) -> &[Arc<dyn AgentTool>] {
        &self.tools
    }
}
