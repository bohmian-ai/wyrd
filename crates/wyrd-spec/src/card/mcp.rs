//! MCP Card spec.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::card::common::NonSecretValue;
use crate::reference::Ref;

/// MCP server or tool catalog metadata.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct McpSpec {
    /// MCP description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Server name.
    pub server_name: String,
    /// Transport descriptor.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transport: Option<String>,
    /// Tool references.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_refs: Vec<Ref>,
    /// Required scopes.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub scopes: Vec<String>,
    /// Free-form details.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub details: BTreeMap<String, NonSecretValue>,
}
