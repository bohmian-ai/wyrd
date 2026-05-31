//! Serializable agent definition.

use serde::{Deserialize, Serialize};
use skald_spec::ProviderName;

use crate::run::RunConfig;

/// Declarative form of an agent.
///
/// `provider` carries a name only; no live client.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentDef {
    /// Stable agent id used as the map key in a workflow.
    pub id: String,
    /// Provider this agent will dispatch through.
    pub provider: ProviderName,
    /// System prompt rendered into one or more native messages at bind time.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub system_prompt: Option<String>,
    /// Model override; when `Some`, replaces the model field inside any
    /// rendered prompt before dispatch.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// Names of tools the agent may dispatch. Resolved against a tool registry
    /// at bind time.
    #[serde(default)]
    pub tool_names: Vec<String>,
    /// Bounded-loop configuration.
    #[serde(default)]
    pub run_config: RunConfig,
}

impl AgentDef {
    /// Builds a minimal `AgentDef` with the given id and provider.
    pub fn new(id: impl Into<String>, provider: ProviderName) -> Self {
        Self {
            id: id.into(),
            provider,
            system_prompt: None,
            model: None,
            tool_names: Vec::new(),
            run_config: RunConfig::default(),
        }
    }
}
