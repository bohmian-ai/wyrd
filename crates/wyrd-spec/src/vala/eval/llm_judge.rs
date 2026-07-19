//! LLM-as-judge task.

use serde::{Deserialize, Serialize};

use crate::card::agent::AgentSpec;
use crate::envelope::CardKind;
use crate::error::WyrdError;
use crate::reference::InlineableRef;

use super::condition::EvalCondition;
use super::ids::{JsonPath, TaskId};
use super::operator::ComparisonOperator;

/// LLM-as-judge task.
///
/// `judge_ref` must point at an `Agent` card or carry an inline Agent spec.
/// The Agent resolves its Prompt child, runs one constrained structured-output
/// turn with workflow context, and the Eval applies `operator` and `expected`
/// to the response.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LlmJudgeTask {
    /// Unique identifier within the enclosing eval task map.
    pub id: TaskId,
    /// Reference to or inline definition of an Agent card.
    pub judge_ref: InlineableRef<AgentSpec>,
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
    /// Constructs an LLM judge task and validates that a resolved `judge_ref`
    /// points at an Agent card.
    ///
    /// # Errors
    /// Returns [`WyrdError::Validation`] when a resolved `judge_ref.kind` is
    /// not [`CardKind::Agent`].
    pub fn new(
        id: TaskId,
        judge_ref: impl Into<InlineableRef<AgentSpec>>,
        operator: ComparisonOperator,
        expected: serde_json::Value,
    ) -> Result<Self, WyrdError> {
        let judge_ref = judge_ref.into();
        if let Some(card_ref) = judge_ref.as_card_ref()
            && card_ref.kind != CardKind::Agent
        {
            return Err(Self::kind_mismatch(&card_ref.kind));
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
    /// Returns [`WyrdError::Validation`] when a resolved `judge_ref.kind` is
    /// not [`CardKind::Agent`] or the optional condition is invalid.
    pub fn validate(&self) -> Result<(), WyrdError> {
        if let Some(card_ref) = self.judge_ref.as_card_ref()
            && card_ref.kind != CardKind::Agent
        {
            return Err(Self::kind_mismatch(&card_ref.kind));
        }
        if let Some(condition) = &self.condition {
            condition.validate()?;
        }
        Ok(())
    }

    fn kind_mismatch(kind: &CardKind) -> WyrdError {
        WyrdError::ValaEvalRefKindMismatch {
            message: format!("llm_judge_task.judge_ref must reference an Agent card; got {kind:?}"),
            details: serde_json::json!({
                "field": "llm_judge.judge_ref",
                "expected": "Agent",
                "got": format!("{kind:?}"),
            }),
        }
    }
}
