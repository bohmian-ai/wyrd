//! Per-task gate predicate.
//!
//! Folds the conditional task shape into a single optional field on every
//! executable eval task.

use serde::{Deserialize, Serialize};

use crate::error::WyrdError;

use super::ids::JsonPath;
use super::operator::ComparisonOperator;

/// Recursion depth cap for [`EvalCondition::subsequent`].
///
/// Conditions are author-time declarations; nothing real-world should need
/// more than a handful of chained predicates. Sixteen is a generous ceiling,
/// and anything past it is almost certainly a misuse.
pub const MAX_CONDITION_DEPTH: u32 = 16;

/// A typed predicate evaluated against one JSON value extracted from workflow
/// context or scenario payload.
///
/// To chain conditions, set both [`EvalCondition::combinator`] and
/// [`EvalCondition::subsequent`]. The whole chain is evaluated left-to-right
/// with `AND` short-circuit on false and `OR` short-circuit on true.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EvalCondition {
    /// JSONPath into the workflow context or scenario payload.
    pub path: JsonPath,
    /// Comparison applied to `(value-at-path, expected)`.
    pub operator: ComparisonOperator,
    /// Right-hand side of the comparison.
    pub expected: serde_json::Value,
    /// Optional combinator for chaining.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub combinator: Option<ConditionCombinator>,
    /// Next link in the chain.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subsequent: Option<Box<EvalCondition>>,
}

impl EvalCondition {
    /// Walks the chain and returns its depth as a number of links.
    #[must_use]
    pub fn depth(&self) -> u32 {
        let mut depth = 1_u32;
        let mut current = self.subsequent.as_deref();
        while let Some(node) = current {
            depth = depth.saturating_add(1);
            current = node.subsequent.as_deref();
        }
        depth
    }

    /// Validates chain depth and combinator/subsequent consistency.
    ///
    /// # Errors
    /// Returns [`WyrdError::Validation`] when the chain is too deep or any
    /// link sets only one of `combinator` and `subsequent`.
    pub fn validate(&self) -> Result<(), WyrdError> {
        let depth = self.depth();
        if depth > MAX_CONDITION_DEPTH {
            return Err(WyrdError::Validation {
                message: format!("eval_condition depth {depth} exceeds max {MAX_CONDITION_DEPTH}"),
                details: serde_json::Value::Null,
            });
        }

        let mut current = Some(self);
        while let Some(node) = current {
            match (node.combinator.is_some(), node.subsequent.is_some()) {
                (false, false) | (true, true) => {}
                (true, false) => {
                    return Err(WyrdError::Validation {
                        message: "eval_condition has combinator without subsequent".to_string(),
                        details: serde_json::Value::Null,
                    });
                }
                (false, true) => {
                    return Err(WyrdError::Validation {
                        message: "eval_condition has subsequent without combinator".to_string(),
                        details: serde_json::Value::Null,
                    });
                }
            }
            current = node.subsequent.as_deref();
        }

        Ok(())
    }
}

/// Logical combinator between two adjacent [`EvalCondition`] nodes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ConditionCombinator {
    /// `lhs AND rhs`, short-circuiting on false.
    And,
    /// `lhs OR rhs`, short-circuiting on true.
    Or,
}
