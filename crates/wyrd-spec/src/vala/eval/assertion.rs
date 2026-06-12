//! Programmatic assertion task.

use serde::{Deserialize, Serialize};

use super::condition::EvalCondition;
use super::ids::{JsonPath, TaskId};
use super::operator::ComparisonOperator;

/// Programmatic assertion evaluated against a JSON value at `context_path`.
///
/// When `item_context_path` is set, the runtime expands the value at that
/// path as an array and emits one assertion result per item.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AssertionTask {
    /// Unique identifier within the enclosing eval task map.
    pub id: TaskId,
    /// Path to the single value to assert against.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_path: Option<JsonPath>,
    /// Path to an array the runtime iterates and asserts per item.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub item_context_path: Option<JsonPath>,
    /// Comparison applied to the observed value and expected value.
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
