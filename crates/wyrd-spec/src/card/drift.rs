//! Drift Card spec.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::reference::CardRef;

/// Reusable drift-monitor definition.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct DriftSpec {
    /// Drift description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Drift method.
    pub method: String,
    /// Baseline data reference.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub baseline_ref: Option<CardRef>,
    /// Target data/model/service references.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub target_refs: Vec<CardRef>,
    /// Feature names.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub features: Vec<String>,
    /// Thresholds by metric.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub thresholds: BTreeMap<String, f64>,
    /// Free-form details.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub details: BTreeMap<String, serde_json::Value>,
}
