//! Agent Card spec.

use serde::{Deserialize, Serialize};

use crate::reference::PromptRef;

/// Pure-serde mirror of the Skald agent run configuration.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct AgentRunConfigSpec {
    /// Maximum loop iterations before the agent stops.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_iterations: Option<u32>,
    /// Maximum concurrent runtime-local tool calls per model iteration.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_concurrency_cap: Option<usize>,
    /// Maximum recent session turns loaded at run start.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_recent_limit: Option<usize>,
    /// Overall agent run timeout in milliseconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u64>,
}

/// Agent authoring body inside a Wyrd Agent Card envelope.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct AgentSpec {
    /// Prompt reference preserved on disk and resolved by `wyrd-cards`.
    #[cfg_attr(feature = "server", schema(value_type = serde_json::Value))]
    pub prompt: PromptRef,
    /// Runtime-local tool names resolved from a local Skald tool registry.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_names: Vec<String>,
    /// Agent run configuration.
    #[serde(default, skip_serializing_if = "is_default_run_config")]
    pub run_config: AgentRunConfigSpec,
}

fn is_default_run_config(config: &AgentRunConfigSpec) -> bool {
    config == &AgentRunConfigSpec::default()
}
