//! Experiment Card spec.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::card::common::{MetricEntry, NonSecretValue, ParameterValue};
use crate::reference::CardRef;
use crate::run::RunRef;

/// Grouping and comparison context for related runs.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct ExperimentSpec {
    /// Experiment type.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub experiment_type: Option<String>,
    /// Experiment description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Primary target Cards.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub target_refs: Vec<CardRef>,
    /// Default parameters applied to runs.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub default_parameters: BTreeMap<String, ParameterValue>,
    /// Associated runs.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub run_refs: Vec<RunRef>,
    /// Summary metrics for comparison.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub summary_metrics: Vec<MetricEntry>,
    /// Best run reference.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub best_run_ref: Option<RunRef>,
    /// Artifact references.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub card_refs: Vec<CardRef>,
    /// Free-form details.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub details: BTreeMap<String, NonSecretValue>,
}
