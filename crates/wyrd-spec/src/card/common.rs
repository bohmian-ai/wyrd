//! Helpers shared across multiple Card specs.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::reference::CardRef;

/// Typed parameter value for experiments, runs, workflows, and tools.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum ParameterValue {
    /// Integer value.
    Int(i64),
    /// Floating-point value.
    Float(f64),
    /// String value.
    Str(String),
    /// Boolean value.
    Bool(bool),
    /// Structured JSON value.
    Json(serde_json::Value),
}

/// Metric summary entry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct MetricEntry {
    /// Metric name.
    pub name: String,
    /// Numeric metric value.
    pub value: f64,
    /// Optional step.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub step: Option<i64>,
    /// Metric event timestamp.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<chrono::DateTime<chrono::Utc>>,
    /// Metric creation timestamp.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<chrono::DateTime<chrono::Utc>>,
    /// Whether this metric came from evaluation.
    #[serde(default)]
    pub is_eval: bool,
}

/// Protocol-specific public metadata for an agent or service interface.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct ProtocolProfile {
    /// Protocol name, such as `a2a`, `ag_ui`, `a2ui`, `mcp`, or `http`.
    pub protocol: String,
    /// Protocol version.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// Protocol-specific declarative metadata.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub metadata: BTreeMap<String, NonSecretValue>,
}

/// Agent-facing interface declaration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct AgentInterface {
    /// Interface name.
    pub name: String,
    /// Interface mode, such as `text`, `json`, `audio`, or `tool`.
    pub mode: String,
    /// Optional schema Card reference.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schema_ref: Option<CardRef>,
    /// Interface metadata.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub metadata: BTreeMap<String, NonSecretValue>,
}

/// Reference to a credential or secret managed outside public Card specs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct CredentialRef {
    /// Credential provider or secret store name.
    pub provider: String,
    /// Key or logical name in that provider.
    pub name: String,
}

/// Non-secret JSON-like configuration value.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum NonSecretValue {
    /// String value.
    Str(String),
    /// Numeric value.
    Number(f64),
    /// Boolean value.
    Bool(bool),
    /// List of non-secret values.
    List(Vec<NonSecretValue>),
    /// Object of non-secret values.
    Object(BTreeMap<String, NonSecretValue>),
}

/// Declarative governance and audit hooks.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct Governance {
    /// Policy Card references.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub policy_refs: Vec<CardRef>,
    /// Audit Card reference.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audit_ref: Option<CardRef>,
    /// Required approval labels or scopes.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub approval_requirements: Vec<String>,
    /// Governance metadata.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub metadata: BTreeMap<String, NonSecretValue>,
}

/// Observation routing hooks for Cards that produce runs or events.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct ObservationHooks {
    /// Observation route Card references.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub route_refs: Vec<CardRef>,
    /// Event names to observe.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub events: Vec<String>,
}
