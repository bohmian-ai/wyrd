//! Sub-agent Card spec.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::card::common::{Governance, NonSecretValue, ObservationHooks};
use crate::reference::CardRef;

/// Declarative sub-agent definition for harnesses.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct SubAgentSpec {
    /// Sub-agent description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// System prompt or prompt reference text.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub system_prompt: Option<String>,
    /// Model selector.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// Tool references.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_refs: Vec<CardRef>,
    /// Disallowed tool names.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub disallowed_tools: Vec<String>,
    /// Skill references.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub skill_refs: Vec<CardRef>,
    /// Maximum turns.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_turns: Option<u32>,
    /// Permission mode.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub permission_mode: Option<String>,
    /// Memory descriptor.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub memory: BTreeMap<String, NonSecretValue>,
    /// Whether background execution is allowed by the harness.
    #[serde(default)]
    pub background: bool,
    /// Effort setting.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effort: Option<String>,
    /// Isolation descriptor.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub isolation: Option<String>,
    /// Compatible CLI names.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub compatible_clis: Vec<String>,
    /// MCP server references or names.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mcp_servers: Vec<String>,
    /// Sandbox mode.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sandbox_mode: Option<String>,
    /// Model temperature.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    /// Governance and audit controls.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub governance: Option<Governance>,
    /// Observation hooks.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub observation_hooks: Option<ObservationHooks>,
    /// Free-form details.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub details: BTreeMap<String, NonSecretValue>,
}
