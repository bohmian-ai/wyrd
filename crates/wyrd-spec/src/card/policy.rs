//! Policy Card spec.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// Declarative policy rule set.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct PolicySpec {
    /// Policy description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Policy rules.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rules: Vec<PolicyRule>,
    /// Enforcement mode.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enforcement: Option<String>,
    /// Free-form details.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub details: BTreeMap<String, serde_json::Value>,
}

/// One policy rule.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct PolicyRule {
    /// Rule name.
    pub name: String,
    /// Rule expression.
    pub expression: String,
    /// Action such as `allow`, `warn`, `block`, or `approval_required`.
    pub action: String,
    /// Rule metadata.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub metadata: BTreeMap<String, serde_json::Value>,
}
