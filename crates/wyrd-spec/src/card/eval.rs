//! Eval Card spec.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::card::common::{Governance, NonSecretValue, ObservationHooks, ParameterValue};
use crate::reference::CardRef;

/// Reusable evaluation logic and pass gates.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct EvalSpec {
    /// Eval description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Eval type, such as `judge`, `assertion`, or `benchmark`.
    pub eval_type: EvalType,
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
    /// Reusable evaluation profile.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile: Option<EvalProfile>,
    /// Default non-secret parameters for eval orchestration.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub default_parameters: BTreeMap<String, ParameterValue>,
    /// Governance and audit requirements.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub governance: Option<Governance>,
    /// Observation routing for eval runs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub observation_hooks: Option<ObservationHooks>,
    /// Free-form details.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub details: BTreeMap<String, NonSecretValue>,
}

/// Reusable evaluation category.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum EvalType {
    /// Rule/assertion evaluation.
    Assertion,
    /// LLM or model judge evaluation.
    Judge,
    /// Dataset or benchmark evaluation.
    Benchmark,
    /// Agentic evaluation of tool use or multi-step behavior.
    Agentic,
    /// Custom evaluation profile.
    #[default]
    Custom,
}

/// Typed reusable evaluation profile.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(tag = "type", content = "config", rename_all = "snake_case")]
pub enum EvalProfile {
    /// Assertion bundle profile.
    Assertion(AssertionEvalProfile),
    /// Judge prompt/profile.
    Judge(JudgeEvalProfile),
    /// Benchmark profile.
    Benchmark(BenchmarkEvalProfile),
    /// Agentic evaluation profile.
    Agentic(AgenticEvalProfile),
    /// Custom profile.
    Custom(CustomEvalProfile),
}

/// Assertion-based eval profile.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct AssertionEvalProfile {
    /// Assertion expressions or rule names.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rules: Vec<String>,
    /// Optional input schema.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_schema: Option<serde_json::Value>,
    /// Optional output schema.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_schema: Option<serde_json::Value>,
}

/// Judge-based eval profile.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct JudgeEvalProfile {
    /// Prompt Cards used by the judge.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub prompt_refs: Vec<CardRef>,
    /// Judge rubric.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rubric: Vec<EvalRubricItem>,
    /// Judge configuration without secret values.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub config: BTreeMap<String, NonSecretValue>,
}

/// Benchmark eval profile.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct BenchmarkEvalProfile {
    /// Benchmark name.
    pub name: String,
    /// Dataset Card references.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dataset_refs: Vec<CardRef>,
    /// Expected metric names.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub metrics: Vec<String>,
}

/// Agentic eval profile.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct AgenticEvalProfile {
    /// Scenario definitions.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub scenarios: Vec<EvalScenario>,
    /// Required tool names or Card aliases.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub required_tools: Vec<String>,
    /// Maximum step count for the evaluated behavior.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_steps: Option<u32>,
}

/// Custom eval profile.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct CustomEvalProfile {
    /// Custom profile name.
    pub name: String,
    /// Non-secret profile configuration.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub config: BTreeMap<String, NonSecretValue>,
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
    pub metadata: BTreeMap<String, NonSecretValue>,
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

/// Judge rubric item.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct EvalRubricItem {
    /// Rubric criterion name.
    pub name: String,
    /// Criterion description.
    pub description: String,
    /// Optional weight.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub weight: Option<f64>,
}

/// Agentic eval scenario.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct EvalScenario {
    /// Scenario name.
    pub name: String,
    /// Scenario input values.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub inputs: BTreeMap<String, ParameterValue>,
    /// Expected outcomes or checks.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub expectations: Vec<String>,
}
