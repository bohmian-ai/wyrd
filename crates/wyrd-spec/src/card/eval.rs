//! Eval Card spec.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::reference::CardRef;

/// Reusable evaluation logic and pass gates.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct EvalSpec {
    /// Eval description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Eval type, such as `judge`, `assertion`, or `benchmark`.
    pub eval_type: String,
    /// Target Card references.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub target_refs: Vec<CardRef>,
    /// Judge Card references.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub judge_refs: Vec<CardRef>,
    /// Assertion definitions.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub assertions: Vec<EvalAssertion>,
    /// Pass gate definitions.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pass_gates: Vec<EvalPassGate>,
    /// Dataset references.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dataset_refs: Vec<CardRef>,
    /// Free-form details.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub details: BTreeMap<String, serde_json::Value>,
}

/// Evaluation assertion.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct EvalAssertion {
    /// Assertion name.
    pub name: String,
    /// Assertion expression or declarative rule.
    pub rule: String,
    /// Assertion metadata.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub metadata: BTreeMap<String, serde_json::Value>,
}

/// Evaluation pass gate.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct EvalPassGate {
    /// Metric or assertion name.
    pub name: String,
    /// Comparison operator.
    pub operator: String,
    /// Threshold value.
    pub threshold: f64,
}
