//! LLM-as-judge task.

use serde::{Deserialize, Serialize};

use crate::envelope::CardKind;
use crate::error::WyrdError;
use crate::reference::CardRef;

use super::condition::EvalCondition;
use super::ids::{JsonPath, TaskId};
use super::operator::ComparisonOperator;

/// LLM-as-judge task.
///
/// `judge_ref` must point at a `Prompt` card. The runtime renders the prompt
/// with workflow context, dispatches it through the approved runtime boundary,
/// and applies `operator` and `expected` to the response.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LlmJudgeTask {
    /// Unique identifier within the enclosing eval task map.
    pub id: TaskId,
    /// Reference to a `Prompt` card.
    pub judge_ref: CardRef,
    /// Optional JSONPath selecting the context passed to the judge.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_path: Option<JsonPath>,
    /// Right-hand side of the comparison.
    pub expected: serde_json::Value,
    /// Comparison applied to the judge response and expected value.
    pub operator: ComparisonOperator,
    /// IDs of upstream tasks that must complete before this task runs.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub depends_on: Vec<TaskId>,
    /// Maximum retries on transient runtime failures.
    #[serde(default = "default_max_retries")]
    pub max_retries: u32,
    /// Optional gate evaluated before the task runs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub condition: Option<EvalCondition>,
}

fn default_max_retries() -> u32 {
    2
}

impl LlmJudgeTask {
    /// Constructs an LLM judge task and validates that `judge_ref` points at a
    /// `Prompt` card.
    ///
    /// # Errors
    /// Returns [`WyrdError::Validation`] when `judge_ref.kind` is not
    /// [`CardKind::Prompt`].
    pub fn new(
        id: TaskId,
        judge_ref: CardRef,
        operator: ComparisonOperator,
        expected: serde_json::Value,
    ) -> Result<Self, WyrdError> {
        if judge_ref.kind != CardKind::Prompt {
            return Err(Self::kind_mismatch(&judge_ref.kind));
        }

        Ok(Self {
            id,
            judge_ref,
            context_path: None,
            expected,
            operator,
            depends_on: Vec::new(),
            max_retries: default_max_retries(),
            condition: None,
        })
    }

    /// Re-validates the task after deserialization.
    ///
    /// # Errors
    /// Returns [`WyrdError::Validation`] when `judge_ref.kind` is not
    /// [`CardKind::Prompt`] or the optional condition is invalid.
    pub fn validate(&self) -> Result<(), WyrdError> {
        if self.judge_ref.kind != CardKind::Prompt {
            return Err(Self::kind_mismatch(&self.judge_ref.kind));
        }
        if let Some(condition) = &self.condition {
            condition.validate()?;
        }
        Ok(())
    }

    fn kind_mismatch(kind: &CardKind) -> WyrdError {
        WyrdError::ValaEvalRefKindMismatch {
            message: format!("llm_judge_task.judge_ref must reference a Prompt card; got {kind:?}"),
            details: serde_json::json!({
                "field": "llm_judge.judge_ref",
                "expected": "Prompt",
                "got": format!("{kind:?}"),
            }),
        }
    }
}
