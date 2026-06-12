//! Per-task evaluation result + workflow-level pass gate.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::ids::TaskId;
use super::operator::ComparisonOperator;

/// Declares how `vala-eval` populates [`AssertionResult::actual`] when writing
/// result rows to `eval_task_results`.
///
/// Evaluation correctness ([`AssertionResult::passed`]) is unaffected by this
/// setting. Only the stored snapshot of the extracted value changes.
///
/// Set at the [`super::spec::EvalSpec`] level via `context_capture`. Absent
/// means `Full`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum EvalContextCapture {
    /// Store the raw value extracted at the context path. Default for offline evals.
    Full,
    /// Store a SHA-256 hex digest of the extracted value. Allows determinism
    /// checks without persisting PII.
    Hash,
    /// Do not populate `actual`. Use when the eval runs over production traffic
    /// where context fields may contain PII.
    Redact,
}

/// Result of one evaluated task.
///
/// Emitted by `vala-eval` per task per workflow run; persisted to the OLAP
/// `eval_task_results` table.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AssertionResult {
    /// Identifier of the evaluated task.
    pub task_id: TaskId,
    /// True iff the assertion compared favorably and no error occurred.
    pub passed: bool,
    /// JSON value extracted at the task's context path.
    ///
    /// `None` when [`EvalContextCapture::Redact`] is set on the enclosing spec.
    /// A SHA-256 hex digest string when [`EvalContextCapture::Hash`].
    /// Populated verbatim when [`EvalContextCapture::Full`] or when
    /// `context_capture` is absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actual: Option<serde_json::Value>,
    /// Right-hand side the task declared.
    pub expected: serde_json::Value,
    /// Operator the task declared.
    pub operator: ComparisonOperator,
    /// Optional human-readable detail, such as a regex mismatch position.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    /// Topological stage, 0-indexed, within the execution plan.
    ///
    /// Useful for stage-aligned visualisations of long runs.
    pub stage: u32,
    /// Wall-clock start of the assertion's evaluation step.
    pub started_at: DateTime<Utc>,
    /// Wall-clock duration in milliseconds.
    pub duration_ms: u64,
}

/// Workflow-level pass criterion.
///
/// Selects how [`AssertionResult`] rows roll up into the workflow's
/// `pass / fail` outcome. Thresholds are in `[0.0, 1.0]`; out-of-range values
/// are rejected by [`EvalPassGate::validate`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum EvalPassGate {
    /// Pass iff `(passed_tasks / total_tasks) >= threshold`.
    OverallPassRate {
        /// Minimum accepted overall pass rate in `[0.0, 1.0]`.
        threshold: f64,
    },
    /// Pass iff the minimum per-task pass rate across **all** tasks meets the
    /// threshold.
    ///
    /// Computed as `min(pass_rate per task across all subjects)`. A task that
    /// never appears scores 0.0. This is task-level, not judge-type-level —
    /// `LlmJudge` and other task types all contribute equally.
    PerJudgePassRate {
        /// Minimum accepted per-task pass rate in `[0.0, 1.0]`.
        threshold: f64,
    },
    /// Pass iff every task passes.
    AllPass,
}

impl EvalPassGate {
    /// Validate thresholds.
    ///
    /// # Errors
    /// Returns [`crate::error::WyrdError::Validation`] when a threshold is
    /// outside `[0.0, 1.0]` or is NaN.
    pub fn validate(&self) -> Result<(), crate::error::WyrdError> {
        let threshold = match self {
            Self::OverallPassRate { threshold } | Self::PerJudgePassRate { threshold } => {
                *threshold
            }
            Self::AllPass => return Ok(()),
        };

        if !(0.0..=1.0).contains(&threshold) || threshold.is_nan() {
            return Err(crate::error::WyrdError::Validation {
                message: format!("eval_pass_gate threshold must be in [0.0, 1.0]; got {threshold}"),
                details: serde_json::Value::Null,
            });
        }

        Ok(())
    }
}
