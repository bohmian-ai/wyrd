//! Drift Card spec.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::card::common::NonSecretValue;
use crate::reference::CardRef;

/// Reusable drift-monitor definition.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct DriftSpec {
    /// Drift description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Drift method.
    pub method: DriftMethod,
    /// Method-specific profile.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile: Option<DriftProfile>,
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
    pub details: BTreeMap<String, NonSecretValue>,
}

/// Supported drift monitor method.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "PascalCase")]
pub enum DriftMethod {
    /// Statistical process-control drift.
    Spc,
    /// Population stability index drift.
    Psi,
    /// Custom metric drift.
    Custom,
    /// Agent evaluation drift.
    Agent,
    /// External drift method.
    #[default]
    External,
}

/// Drift profile configuration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(tag = "type", content = "config", rename_all = "PascalCase")]
pub enum DriftProfile {
    /// SPC profile.
    Spc(SpcProfile),
    /// PSI profile.
    Psi(PsiProfile),
    /// Custom profile.
    Custom(CustomProfile),
    /// Agent profile.
    Agent(AgentEvalProfile),
}

/// SPC profile fields.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct SpcProfile {
    /// Sample size.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sample_size: Option<usize>,
    /// Feature map.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub feature_map: BTreeMap<String, String>,
    /// Alert threshold.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alert_threshold: Option<f64>,
}

/// PSI profile fields.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct PsiProfile {
    /// Categorical feature names.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub categorical_features: Vec<String>,
    /// Numeric binning strategy.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub binning_strategy: Option<String>,
    /// Alert threshold.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alert_threshold: Option<f64>,
}

/// Custom drift profile fields.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct CustomProfile {
    /// Metric name.
    pub metric: String,
    /// Sample size.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sample_size: Option<usize>,
    /// Alert threshold.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alert_threshold: Option<f64>,
}

/// Agent evaluation drift profile fields.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct AgentEvalProfile {
    /// Evaluation task.
    pub task: String,
    /// Sample ratio.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sample_ratio: Option<f64>,
    /// Alert threshold.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alert_threshold: Option<f64>,
}
