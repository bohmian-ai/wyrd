//! Tool Card spec.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::card::common::{CredentialRef, NonSecretValue};

/// Declarative tool definition for agent harnesses.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct ToolSpec {
    /// LLM-visible tool name.
    pub name: String,
    /// Tool description.
    pub description: String,
    /// Tool type, such as `script`, `api`, `mcp`, or `builtin`.
    pub tool_type: String,
    /// Input argument schema.
    pub args_schema: serde_json::Value,
    /// Output schema.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_schema: Option<serde_json::Value>,
    /// Script tool config.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub script_config: BTreeMap<String, NonSecretValue>,
    /// API tool config.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub api_config: BTreeMap<String, NonSecretValue>,
    /// Credential references required by this tool.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub credential_refs: Vec<CredentialRef>,
    /// MCP server name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mcp_server_name: Option<String>,
    /// Allowed downstream tool names.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub allowed_tools: Vec<String>,
    /// Whether approval is required before use.
    #[serde(default)]
    pub requires_approval: bool,
    /// Hook event names.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hook_events: Vec<String>,
    /// Hook matcher metadata.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub hook_matcher: BTreeMap<String, NonSecretValue>,
    /// Free-form details.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub details: BTreeMap<String, NonSecretValue>,
}
