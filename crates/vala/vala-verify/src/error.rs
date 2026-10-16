//! Engine-internal comparison and capture failures.
//!
//! [`ExecError`] is crate-local and uses plain `thiserror`. The operator
//! dispatcher, JSONPath extraction and evidence capture raise it. It is never
//! serialized; the Verifier engine records a comparison failure as an
//! inconclusive requirement and a capture failure as
//! [`crate::blocks::EngineError::Capture`].

use thiserror::Error;
use wyrd_spec::verification::TaskId;

/// A comparison or capture that could not produce a value.
#[derive(Debug, Error)]
pub enum ExecError {
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
    /// A `JsonPath` evaluation produced no matches or surfaced a parser error.
    #[error("JsonPath {path} failed: {reason}")]
    JsonPathFailure { path: String, reason: String },
    /// Operator dispatch rejected the observed/expected pair for type-shape
    /// reasons that the typed catalog cannot encode at parse time.
    #[error("task {task_id:?}: operator {operator:?} cannot compare {reason}")]
    OperatorTypeMismatch {
        task_id: TaskId,
        operator: String,
        reason: String,
    },
    /// Context capture hashing could not canonicalize a JSON value.
    #[error("failed to canonicalize context value for hashing: {reason}")]
    ContextHashFailed { reason: String },
}
