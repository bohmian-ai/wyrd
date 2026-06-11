//! Two-phase error model for the eval engine.
//!
//! - [`EvalExecError`] is crate-local and uses plain `thiserror`. The
//!   executors, stores, and operator dispatcher raise this. It is never
//!   serialized and never crosses an external boundary.
//! - [`EvalPlanError`] is the typed scenario-loader failure surface. Loader
//!   failures are plan/input failures, not execution failures.
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

    /// All retry attempts for an `LlmJudge` task were exhausted.
    #[error(
        "llm_judge task {task_id:?}: judge failed after {attempts} attempt(s); last error: {last_error}"
    )]
    JudgeRetriesExhausted {
        task_id: TaskId,
        attempts: u32,
        last_error: String,
    },

    /// The judge returned malformed structured output.
    #[error("llm_judge task {task_id:?}: judge returned invalid structured output: {reason}")]
    JudgeInvalidOutput { task_id: TaskId, reason: String },

    /// `TraceSource::fetch` returned `Err` or timed out before the deadline.
    #[error("trace {trace_id} unavailable: {reason}")]
    TraceUnavailable { trace_id: String, reason: String },

    /// `TraceSource` returned `TraceUnavailable` for this task's record.
    #[error("trace_assertion task {task_id:?}: trace unavailable: {reason}")]
    TraceUnavailableForTask { task_id: TaskId, reason: String },

    /// A trace task ran without a trace id on the current snapshot.
    #[error("trace_assertion task {task_id:?}: no trace_id on record")]
    TraceIdMissing { task_id: TaskId },

    /// An `LlmJudge` task declared a media variable that the record did not
    /// supply.
    #[error("media binding {binding} missing for judge invocation")]
    MediaBindingMissing { binding: String },

    /// A required media id was absent from the record binding set.
    #[error("llm_judge task {task_id:?}: required media id {media_id:?} not bound to the record")]
    MediaBindingMissingForTask { task_id: TaskId, media_id: String },

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

    /// A task's condition referenced a JSONPath that did not resolve against
    /// the current execution context.
    #[error("task {task_id:?}: condition path {path:?} did not resolve")]
    ConditionPathMissing { task_id: TaskId, path: String },

    /// A task asked to extract a JSONPath that did not resolve against the
    /// current execution context.
    #[error("task {task_id:?}: extract path {path:?} did not resolve")]
    ExtractPathMissing { task_id: TaskId, path: String },

    /// Operator dispatch rejected the observed/expected pair for type-shape
    /// reasons that the typed catalog cannot encode at parse time.
    #[error("task {task_id:?}: operator {operator:?} cannot compare {reason}")]
    OperatorTypeMismatch {
        task_id: TaskId,
        operator: String,
        reason: String,
    },

    /// The task registry saw the same task id twice while indexing a plan.
    #[error("duplicate eval task id {task:?}")]
    DuplicateTaskId { task: TaskId },

    /// The task registry saw a dependency absent from the indexed task set.
    #[error("task {task:?} depends on unknown task {dep:?}")]
    UnknownDependency { task: TaskId, dep: TaskId },

    /// Failed to serialize an eval results artifact to JSON.
    #[error("failed to serialize eval results: {reason}")]
    ResultsSerializeFailed { reason: String },

    /// Failed to deserialize an eval results artifact from JSON.
    #[error("failed to deserialize eval results: {reason}")]
    ResultsDeserializeFailed { reason: String },

    /// IO error while reading or writing an eval results artifact.
    #[error("eval results IO failed at {path}: {reason}")]
    ResultsIoFailed { path: String, reason: String },

    /// Context capture hashing could not canonicalize a JSON value.
    #[error("failed to canonicalize context value for hashing: {reason}")]
    ContextHashFailed { reason: String },
}

/// Errors raised while loading an offline scenario collection.
#[derive(Debug, Error)]
pub enum EvalPlanError {
    /// The scenario file path did not exist.
    #[error("scenario file not found: {path}")]
    ScenarioFileMissing { path: String },

    /// The scenario file could not be read or parsed.
    #[error("scenario file unreadable: {path}{}: {reason}", line.map(|n| format!(" (line {n})")).unwrap_or_default())]
    ScenarioFileUnreadable {
        path: String,
        line: Option<usize>,
        reason: String,
    },

    /// The scenario file extension is not one of json, yaml, yml, or jsonl.
    #[error("scenario file extension not supported: {path} (got {ext:?})")]
    ScenarioFileExtUnsupported { path: String, ext: Option<String> },

    /// A scenario failed per-scenario validation.
    #[error("scenario invalid: {path}{}: {reason}", scenario_id.as_deref().map(|s| format!(" (scenario {s})")).unwrap_or_default())]
    ScenarioCollectionInvalid {
        path: String,
        scenario_id: Option<String>,
        reason: String,
    },

    /// The collection contains the same scenario id more than once.
    #[error("scenario collection has duplicate id {scenario_id}: {path}")]
    ScenarioCollectionDuplicateId { path: String, scenario_id: String },

    /// The collection had no scenarios after parsing.
    #[error("scenario collection is empty: {path}")]
    ScenarioCollectionEmpty { path: String },
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

    /// Eval results could not be serialized, deserialized, or written to
    /// storage. This is a server-side failure, not a caller input error.
    #[error("[WYRD_EVAL_500_RESULTS_PERSISTENCE_FAILED] {message}")]
    #[wyrd_error(
        code = "WYRD_EVAL_500_RESULTS_PERSISTENCE_FAILED",
        status = 500,
        title = "Eval results persistence failed",
        remediation = "Check server storage availability and permissions; retry after the underlying I/O issue is resolved."
    )]
    ResultsPersistenceFailed {
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
            | Self::JsonPathFailure { message, details }
            | Self::ResultsPersistenceFailed { message, details } => (message, details),
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
            EvalExecError::JudgeRetriesExhausted {
                task_id,
                attempts,
                last_error,
            } => Self::JudgeUnavailable {
                message,
                details: serde_json::json!({
                    "task_id": task_id.as_str(),
                    "attempts": attempts,
                    "last_error": last_error,
                }),
            },
            EvalExecError::JudgeInvalidOutput { task_id, reason } => Self::JudgeUnavailable {
                message,
                details: serde_json::json!({
                    "task_id": task_id.as_str(),
                    "reason": reason,
                }),
            },
            EvalExecError::TraceUnavailable { trace_id, reason } => Self::TraceUnavailable {
                message,
                details: serde_json::json!({
                    "trace_id": trace_id,
                    "reason": reason,
                }),
            },
            EvalExecError::TraceUnavailableForTask { task_id, reason } => Self::TraceUnavailable {
                message,
                details: serde_json::json!({
                    "task_id": task_id.as_str(),
                    "reason": reason,
                }),
            },
            EvalExecError::TraceIdMissing { task_id } => Self::TraceUnavailable {
                message,
                details: serde_json::json!({
                    "task_id": task_id.as_str(),
                    "reason": "trace_id missing on record",
                }),
            },
            EvalExecError::MediaBindingMissing { binding } => Self::MediaBindingMissing {
                message,
                details: serde_json::json!({ "binding": binding }),
            },
            EvalExecError::MediaBindingMissingForTask { task_id, media_id } => {
                Self::MediaBindingMissing {
                    message,
                    details: serde_json::json!({
                        "task_id": task_id.as_str(),
                        "binding": media_id,
                    }),
                }
            }
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
            EvalExecError::ConditionPathMissing { task_id, path } => Self::JsonPathFailure {
                message,
                details: serde_json::json!({
                    "task_id": task_id.as_str(),
                    "path": path,
                    "reason": "condition path did not resolve",
                }),
            },
            EvalExecError::ExtractPathMissing { task_id, path } => Self::JsonPathFailure {
                message,
                details: serde_json::json!({
                    "task_id": task_id.as_str(),
                    "path": path,
                    "reason": "extract path did not resolve",
                }),
            },
            EvalExecError::OperatorTypeMismatch {
                task_id,
                operator,
                reason,
            } => Self::OperatorMismatch {
                message,
                details: serde_json::json!({
                    "task_id": task_id.as_str(),
                    "operator": operator,
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
            EvalExecError::ResultsSerializeFailed { reason } => Self::ResultsPersistenceFailed {
                message,
                details: serde_json::json!({
                    "operation": "serialize",
                    "reason": reason,
                }),
            },
            EvalExecError::ResultsDeserializeFailed { reason } => Self::ResultsPersistenceFailed {
                message,
                details: serde_json::json!({
                    "operation": "deserialize",
                    "reason": reason,
                }),
            },
            EvalExecError::ResultsIoFailed { path, reason } => Self::ResultsPersistenceFailed {
                message,
                details: serde_json::json!({
                    "operation": "io",
                    "path": path,
                    "reason": reason,
                }),
            },
            EvalExecError::ContextHashFailed { reason } => Self::OperatorMismatch {
                message,
                details: serde_json::json!({
                    "operator": "context_capture_hash",
                    "reason": reason,
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
