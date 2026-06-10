//! Two-phase error model for the eval engine.
//!
//! - [`EvalExecError`] is crate-local and uses plain `thiserror`. The
//!   executors, stores, scenario loader, and operator dispatcher raise this.
//!   It is never serialized and never crosses an external boundary.
//! - [`EvalError`] is the public-facing enum. Variants carry
//!   `#[wyrd_error(code = "WYRD_EVAL_<STATUS>_<SLUG>", status, title,
//!   remediation)]` so the `WyrdError` derive supplies stable metadata.
//!
//! Code format follows the wyrd-rust-python skill `references/errors.md`:
//! `WYRD_<DOMAIN>_<STATUS>_<SLUG>`.

use serde::{Deserialize, Serialize};
use thiserror::Error;

use wyrd_error_derive::WyrdError;
use wyrd_spec::vala::eval::ids::TaskId;

/// Errors raised inside the engine: executor dispatch, store lookups, scenario
/// loading, JsonPath evaluation, and operator mismatch.
///
/// Crate-local; never serialized; never crosses a public boundary. Surface
/// layers convert to [`EvalError`] via the `From` impl at the bottom of this
/// file.
#[derive(Debug, Error)]
pub enum EvalExecError {
    /// `wyrd_spec::vala::eval::plan::validate_dag` rejected the spec.
    #[error("eval task DAG invalid: {reason}")]
    DagInvalid { reason: String },

    /// A `TaskId` referenced inside a stage was absent from the task registry.
    #[error("task {task_id} not found in task registry")]
    TaskNotFound { task_id: String },

    /// A task referenced a dependency that no prior stage produced.
    #[error("task {task_id} depends on {depends_on}, which has no recorded result")]
    DependencyMissing { task_id: String, depends_on: String },

    /// The injected `JudgeInvoker` could not be reached or returned a terminal
    /// failure after retry budget.
    #[error("judge invoker unavailable: {reason}")]
    JudgeUnavailable { reason: String },

    /// `TraceSource::fetch` returned `Err` or timed out before the deadline.
    #[error("trace {trace_id} unavailable: {reason}")]
    TraceUnavailable { trace_id: String, reason: String },

    /// An `LlmJudge` task declared a media variable that the record did not
    /// supply.
    #[error("media binding {binding} missing for judge invocation")]
    MediaBindingMissing { binding: String },

    /// Operator received an input whose JSON kind it cannot evaluate.
    #[error("operator {op} is not defined on inputs of kind ({left_kind}, {right_kind})")]
    OperatorMismatch {
        op: String,
        left_kind: &'static str,
        right_kind: &'static str,
    },

    /// Operator parameters are valid wire shapes but nonsense at runtime.
    #[error("operator {op}: invalid configuration: {message}")]
    OperatorInvalidConfig { op: String, message: String },

    /// User-supplied regex pattern failed to compile at evaluation time.
    #[error("operator {op}: regex pattern {pattern:?} is invalid: {source}")]
    OperatorRegexInvalid {
        op: String,
        pattern: String,
        #[source]
        source: regex::Error,
    },

    /// A scenario / record lookup by `RecordId` returned no row.
    #[error("record {record_id} not found")]
    RecordMissing { record_id: String },

    /// The scenario loader failed to parse a JSONL / JSON / YAML payload.
    #[error("scenario loader failure ({scenario_source}): {reason}")]
    ScenarioLoaderFailure {
        scenario_source: String,
        reason: String,
    },

    /// A `JsonPath` evaluation produced no matches or surfaced a parser error.
    #[error("JsonPath {path} failed: {reason}")]
    JsonPathFailure { path: String, reason: String },

    /// The task registry saw the same task id twice while indexing a plan.
    #[error("duplicate eval task id {task:?}")]
    DuplicateTaskId { task: TaskId },

    /// The task registry saw a dependency absent from the indexed task set.
    #[error("task {task:?} depends on unknown task {dep:?}")]
    UnknownDependency { task: TaskId, dep: TaskId },
}

/// Public-facing eval error.
///
/// Variants carry the `#[wyrd_error(...)]` metadata the derive turns into
/// `code()`, `status()`, `title()`, and `remediation()`. Server, CLI, and
/// protocol-router layers raise this after translating [`EvalExecError`].
#[derive(Debug, Clone, Error, Serialize, Deserialize, schemars::JsonSchema, WyrdError)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum EvalError {
    /// Eval task DAG failed validation.
    #[error("[WYRD_EVAL_422_DAG_INVALID] {message}")]
    #[wyrd_error(
        code = "WYRD_EVAL_422_DAG_INVALID",
        status = 422,
        title = "Eval task DAG invalid",
        remediation = "Remove cycles, self-loops, or references to tasks that the EvalSpec does not define."
    )]
    DagInvalid {
        message: String,
        details: serde_json::Value,
    },

    /// A required task was not found in the spec's task registry.
    #[error("[WYRD_EVAL_404_TASK_NOT_FOUND] {message}")]
    #[wyrd_error(
        code = "WYRD_EVAL_404_TASK_NOT_FOUND",
        status = 404,
        title = "Eval task not found",
        remediation = "Check the task_id against EvalSpec.tasks; spec validation should have caught this earlier."
    )]
    TaskNotFound {
        message: String,
        details: serde_json::Value,
    },

    /// A task's `depends_on` edge points at a task whose output never landed.
    #[error("[WYRD_EVAL_422_DEPENDENCY_MISSING] {message}")]
    #[wyrd_error(
        code = "WYRD_EVAL_422_DEPENDENCY_MISSING",
        status = 422,
        title = "Eval task dependency missing",
        remediation = "Confirm the depended-on task is not gated by an EvalCondition that always skips, or remove the dependency."
    )]
    DependencyMissing {
        message: String,
        details: serde_json::Value,
    },

    /// The injected `JudgeInvoker` is unavailable past retry budget.
    #[error("[WYRD_EVAL_503_JUDGE_UNAVAILABLE] {message}")]
    #[wyrd_error(
        code = "WYRD_EVAL_503_JUDGE_UNAVAILABLE",
        status = 503,
        title = "LLM judge unavailable",
        remediation = "Retry after the provider recovers, or raise LlmJudge.max_retries; for tests, register a JudgeInvoker mock."
    )]
    JudgeUnavailable {
        message: String,
        details: serde_json::Value,
    },

    /// `TraceSource::fetch` returned no span set before the deadline.
    #[error("[WYRD_EVAL_504_TRACE_UNAVAILABLE] {message}")]
    #[wyrd_error(
        code = "WYRD_EVAL_504_TRACE_UNAVAILABLE",
        status = 504,
        title = "Trace unavailable",
        remediation = "Wait for trace ingestion to land the spans, raise the per-call deadline, or schedule a retry via the AwaitingTrace lifecycle."
    )]
    TraceUnavailable {
        message: String,
        details: serde_json::Value,
    },

    /// An `LlmJudge` task referenced a media variable the record did not bind.
    #[error("[WYRD_EVAL_422_MEDIA_BINDING_MISSING] {message}")]
    #[wyrd_error(
        code = "WYRD_EVAL_422_MEDIA_BINDING_MISSING",
        status = 422,
        title = "Media binding missing",
        remediation = "Supply every media variable the judge prompt declares before invoking the eval record."
    )]
    MediaBindingMissing {
        message: String,
        details: serde_json::Value,
    },

    /// Operator dispatch rejected the actual / expected pair under its
    /// contract.
    #[error("[WYRD_EVAL_422_OPERATOR_MISMATCH] {message}")]
    #[wyrd_error(
        code = "WYRD_EVAL_422_OPERATOR_MISMATCH",
        status = 422,
        title = "Operator mismatch",
        remediation = "Pick a ComparisonOperator that matches the actual value's JSON shape, or adjust the JsonPath that produced the actual value."
    )]
    OperatorMismatch {
        message: String,
        details: serde_json::Value,
    },

    /// A `record_id` referenced by the scoring path did not resolve.
    #[error("[WYRD_EVAL_404_RECORD_MISSING] {message}")]
    #[wyrd_error(
        code = "WYRD_EVAL_404_RECORD_MISSING",
        status = 404,
        title = "Eval record missing",
        remediation = "Confirm the record_id was persisted and has not been redacted by retention; retry after ingestion catches up."
    )]
    RecordMissing {
        message: String,
        details: serde_json::Value,
    },

    /// Scenario loader could not parse the supplied dataset.
    #[error("[WYRD_EVAL_400_SCENARIO_LOADER_FAILURE] {message}")]
    #[wyrd_error(
        code = "WYRD_EVAL_400_SCENARIO_LOADER_FAILURE",
        status = 400,
        title = "Scenario loader failure",
        remediation = "Fix the JSONL / JSON / YAML scenario payload; consult the source path and line in details."
    )]
    ScenarioLoaderFailure {
        message: String,
        details: serde_json::Value,
    },

    /// JsonPath evaluation against context / record / span / workflow failed.
    #[error("[WYRD_EVAL_422_JSONPATH_FAILURE] {message}")]
    #[wyrd_error(
        code = "WYRD_EVAL_422_JSONPATH_FAILURE",
        status = 422,
        title = "JsonPath evaluation failed",
        remediation = "Check the JsonPath against the actual record / span shape; JsonPath conforms to the RFC 9535 subset declared on wyrd_spec::vala::eval::ids::JsonPath."
    )]
    JsonPathFailure {
        message: String,
        details: serde_json::Value,
    },
}

impl EvalError {
    /// RFC 9457 JSON problem payload.
    #[must_use]
    pub fn as_problem_json(&self) -> serde_json::Value {
        let (message, details) = self.message_details();
        serde_json::json!({
            "type": format!("https://wyrd.dev/problems/{}", self.code()),
            "title": self.title(),
            "status": self.status(),
            "detail": message,
            "code": self.code(),
            "details": details,
            "remediation": self.remediation(),
        })
    }

    fn message_details(&self) -> (&str, &serde_json::Value) {
        match self {
            Self::DagInvalid { message, details }
            | Self::TaskNotFound { message, details }
            | Self::DependencyMissing { message, details }
            | Self::JudgeUnavailable { message, details }
            | Self::TraceUnavailable { message, details }
            | Self::MediaBindingMissing { message, details }
            | Self::OperatorMismatch { message, details }
            | Self::RecordMissing { message, details }
            | Self::ScenarioLoaderFailure { message, details }
            | Self::JsonPathFailure { message, details } => (message, details),
        }
    }
}

impl From<EvalExecError> for EvalError {
    fn from(error: EvalExecError) -> Self {
        let message = error.to_string();
        match error {
            EvalExecError::DagInvalid { reason } => Self::DagInvalid {
                message,
                details: serde_json::json!({ "reason": reason }),
            },
            EvalExecError::TaskNotFound { task_id } => Self::TaskNotFound {
                message,
                details: serde_json::json!({ "task_id": task_id }),
            },
            EvalExecError::DependencyMissing {
                task_id,
                depends_on,
            } => Self::DependencyMissing {
                message,
                details: serde_json::json!({
                    "task_id": task_id,
                    "depends_on": depends_on,
                }),
            },
            EvalExecError::JudgeUnavailable { reason } => Self::JudgeUnavailable {
                message,
                details: serde_json::json!({ "reason": reason }),
            },
            EvalExecError::TraceUnavailable { trace_id, reason } => Self::TraceUnavailable {
                message,
                details: serde_json::json!({
                    "trace_id": trace_id,
                    "reason": reason,
                }),
            },
            EvalExecError::MediaBindingMissing { binding } => Self::MediaBindingMissing {
                message,
                details: serde_json::json!({ "binding": binding }),
            },
            EvalExecError::OperatorMismatch {
                op,
                left_kind,
                right_kind,
            } => Self::OperatorMismatch {
                message,
                details: serde_json::json!({
                    "operator": op,
                    "left_kind": left_kind,
                    "right_kind": right_kind,
                }),
            },
            EvalExecError::OperatorInvalidConfig {
                op,
                message: reason,
            } => Self::OperatorMismatch {
                message,
                details: serde_json::json!({
                    "operator": op,
                    "reason": reason,
                }),
            },
            EvalExecError::OperatorRegexInvalid {
                op,
                pattern,
                source,
            } => Self::OperatorMismatch {
                message,
                details: serde_json::json!({
                    "operator": op,
                    "pattern": pattern,
                    "reason": source.to_string(),
                }),
            },
            EvalExecError::RecordMissing { record_id } => Self::RecordMissing {
                message,
                details: serde_json::json!({ "record_id": record_id }),
            },
            EvalExecError::ScenarioLoaderFailure {
                scenario_source,
                reason,
            } => Self::ScenarioLoaderFailure {
                message,
                details: serde_json::json!({
                    "source": scenario_source,
                    "reason": reason,
                }),
            },
            EvalExecError::JsonPathFailure { path, reason } => Self::JsonPathFailure {
                message,
                details: serde_json::json!({
                    "path": path,
                    "reason": reason,
                }),
            },
            EvalExecError::DuplicateTaskId { task } => Self::TaskNotFound {
                message,
                details: serde_json::json!({ "task_id": task.as_str() }),
            },
            EvalExecError::UnknownDependency { task, dep } => Self::DependencyMissing {
                message,
                details: serde_json::json!({
                    "task_id": task.as_str(),
                    "depends_on": dep.as_str(),
                }),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{EvalError, EvalExecError};

    #[test]
    fn dag_invalid_translates_with_reason_detail() {
        let exec = EvalExecError::DagInvalid {
            reason: "cycle: t1 -> t2 -> t1".to_owned(),
        };
        let public: EvalError = exec.into();
        let problem = public.as_problem_json();

        assert_eq!(problem["code"], "WYRD_EVAL_422_DAG_INVALID");
        assert_eq!(problem["status"], 422);
        assert_eq!(problem["details"]["reason"], "cycle: t1 -> t2 -> t1");
    }

    #[test]
    fn judge_unavailable_serde_tag() {
        let public = EvalError::JudgeUnavailable {
            message: "provider 503".to_owned(),
            details: serde_json::json!({ "reason": "provider 503" }),
        };
        let value = serde_json::to_value(public).expect("serializes");
        assert_eq!(value["kind"], "judge_unavailable");
    }
}
