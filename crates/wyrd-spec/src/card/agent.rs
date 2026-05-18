//! Agent Card spec.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::card::common::{AgentInterface, ProtocolProfile};
use crate::reference::CardRef;

/// Protocol-neutral agent metadata and composition.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct AgentSpec {
    /// Agent description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Agent capabilities.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub capabilities: Vec<String>,
    /// Default input modes.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub default_input_modes: Vec<String>,
    /// Default output modes.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub default_output_modes: Vec<String>,
    /// Primary prompt reference.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt_ref: Option<CardRef>,
    /// Additional prompt references.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub prompt_refs: Vec<CardRef>,
    /// Tool references.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_refs: Vec<CardRef>,
    /// Skill references.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub skill_refs: Vec<CardRef>,
    /// Sub-agent references.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub subagent_refs: Vec<CardRef>,
    /// Memory profile reference.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory_ref: Option<CardRef>,
    /// Maximum loop iterations.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_iterations: Option<u32>,
    /// Supported interfaces.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub interfaces: Vec<AgentInterface>,
    /// Protocol profiles.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub protocol_profiles: Vec<ProtocolProfile>,
    /// Security scheme names or inline descriptors.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub security_schemes: Vec<String>,
    /// Security requirements.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub security_requirements: Vec<String>,
    /// Provider metadata.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub provider: BTreeMap<String, serde_json::Value>,
    /// Documentation URL.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub documentation_url: Option<String>,
    /// Icon URL.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon_url: Option<String>,
    /// Signature metadata.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub signatures: BTreeMap<String, serde_json::Value>,
    /// Free-form details.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub details: BTreeMap<String, serde_json::Value>,
}
