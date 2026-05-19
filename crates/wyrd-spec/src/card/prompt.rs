//! Prompt Card spec.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::card::common::{NonSecretValue, PromptRole, Provider};
use crate::reference::CardRef;

/// Prompt template and metadata.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct PromptSpec {
    /// Prompt description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Provider identity.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<Provider>,
    /// Model identifier.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// Template body.
    pub template: String,
    /// Template format, such as `text`, `jinja`, or `chat`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub template_format: Option<String>,
    /// Structured prompt messages.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub messages: Vec<PromptMessage>,
    /// Required variable names.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub variables: Vec<String>,
    /// Structured variable descriptors.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub variable_specs: Vec<PromptVariable>,
    /// Tool references made available to this prompt.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_refs: Vec<CardRef>,
    /// Provider parameters.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub parameters: BTreeMap<String, NonSecretValue>,
    /// Input schema.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_schema: Option<serde_json::Value>,
    /// Output schema or structured-output contract.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_schema: Option<serde_json::Value>,
    /// Related experiment.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub experiment_ref: Option<CardRef>,
    /// Audit Card reference.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audit_ref: Option<CardRef>,
    /// Content hash.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_hash: Option<String>,
    /// Free-form details.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub details: BTreeMap<String, NonSecretValue>,
}

/// One prompt message.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct PromptMessage {
    /// Message role.
    pub role: PromptRole,
    /// Message content template.
    pub content: String,
    /// Optional message name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

/// Prompt variable descriptor.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct PromptVariable {
    /// Variable name.
    pub name: String,
    /// Variable description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Whether the variable is required.
    #[serde(default)]
    pub required: bool,
    /// Optional variable schema.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schema: Option<serde_json::Value>,
}
