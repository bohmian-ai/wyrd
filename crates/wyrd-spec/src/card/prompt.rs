//! Prompt Card spec.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::reference::CardRef;

/// Prompt template and metadata.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct PromptSpec {
    /// Prompt description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Template body.
    pub template: String,
    /// Template format, such as `text`, `jinja`, or `chat`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub template_format: Option<String>,
    /// Required variable names.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub variables: Vec<String>,
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
    pub details: BTreeMap<String, serde_json::Value>,
}
