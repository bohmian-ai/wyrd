//! Model Card spec.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::reference::CardRef;

/// Pure-data description of a model.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct ModelSpec {
    /// Model description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Model task type.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_type: Option<String>,
    /// Framework name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub framework: Option<String>,
    /// Data Card reference.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data_ref: Option<CardRef>,
    /// Experiment Card reference.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub experiment_ref: Option<CardRef>,
    /// Audit Card reference.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audit_ref: Option<CardRef>,
    /// Model artifact references.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub artifact_refs: Vec<CardRef>,
    /// Free-form details.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub details: BTreeMap<String, serde_json::Value>,
}
