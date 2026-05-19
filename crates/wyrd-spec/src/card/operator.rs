//! Operator Card spec.

use serde::{Deserialize, Serialize};

use crate::card::artifact::FrameworkAdapterRef;
use crate::reference::CardRef;

/// Server-side, policy-gated Operator declaration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct OperatorSpec {
    /// Framework adapter used to execute this Operator.
    pub adapter: FrameworkAdapterRef,
    /// Declared runtime inputs.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub inputs: Vec<OperatorInput>,
    /// Policy Cards evaluated before invocation.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pre_invoke: Vec<CardRef>,
    /// Policy Cards evaluated after invocation.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub post_invoke: Vec<CardRef>,
    /// Optional runtime budget.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub budget: Option<OperatorBudget>,
}

/// Operator input declaration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct OperatorInput {
    /// Input name.
    pub name: String,
    /// Schema reference URI or symbolic schema identifier.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schema_ref: Option<String>,
}

/// Operator execution budget.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct OperatorBudget {
    /// Maximum wall-clock runtime in seconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_wall_seconds: Option<u32>,
    /// Maximum memory in MiB.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_memory_mb: Option<u32>,
    /// Maximum tool calls allowed during invocation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_tool_calls: Option<u32>,
}
