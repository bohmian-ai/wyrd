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
    pub metadata: BTreeMap<String, serde_json::Value>,
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
    pub metadata: BTreeMap<String, serde_json::Value>,
}
