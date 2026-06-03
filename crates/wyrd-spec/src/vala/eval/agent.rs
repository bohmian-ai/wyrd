//! Agent-assertion task.

use serde::{Deserialize, Serialize};

use super::condition::EvalCondition;
use super::ids::{JsonPath, TaskId};
use super::operator::ComparisonOperator;

/// Assertion over the assembled GenAI workflow envelope.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AgentAssertionTask {
    /// Unique identifier within the enclosing eval task map.
    pub id: TaskId,
    /// JSONPath into the GenAI workflow envelope.
    pub workflow_field_path: JsonPath,
    /// Comparison applied to the selected workflow value and expected value.
    pub operator: ComparisonOperator,
    /// Right-hand side of the comparison.
    pub expected: serde_json::Value,
    /// IDs of upstream tasks that must complete before this task runs.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub depends_on: Vec<TaskId>,
    /// Optional gate evaluated before the task runs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub condition: Option<EvalCondition>,
}
