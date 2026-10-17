//! Engine-internal comparison and capture failures.
//!
//! [`ExecError`] uses plain `thiserror` and is re-exported from the crate
//! root, but it is not a public wire error: it describes failures inside
//! Verifier engine handling. The operator dispatcher, JSONPath extraction and
//! evidence capture raise it. It is never serialized; the Verifier engine
//! records a comparison or JSONPath failure as an inconclusive requirement
//! and a capture failure as [`crate::blocks::EngineError::Capture`]. A
//! JSONPath that matches nothing is not an error: extraction normalizes zero
//! matches to `null` for the operator to judge.

use thiserror::Error;
use wyrd_spec::verification::TaskId;

/// A comparison or capture that could not produce a value.
#[derive(Debug, Error)]
pub enum ExecError {
    /// Operator received an input whose JSON kind it cannot evaluate.
    #[error("operator {op} is not defined on inputs of kind ({left_kind}, {right_kind})")]
    OperatorMismatch {
        /// Name of the operator that rejected the inputs.
        op: String,
        /// JSON kind of the observed (left) operand, such as `string`.
        left_kind: &'static str,
        /// JSON kind of the expected (right) operand, such as `number`.
        right_kind: &'static str,
    },
    /// Operator parameters are valid wire shapes but nonsense at runtime.
    #[error("operator {op}: invalid configuration: {message}")]
    OperatorInvalidConfig {
        /// Name of the operator whose parameters were rejected.
        op: String,
        /// Human-readable description of the invalid parameter.
        message: String,
    },
    /// User-supplied regex pattern failed to compile at evaluation time.
    #[error("operator {op}: regex pattern {pattern:?} is invalid: {source}")]
    OperatorRegexInvalid {
        /// Name of the regex operator that compiled the pattern.
        op: String,
        /// The authored pattern exactly as supplied.
        pattern: String,
        /// The regex compiler's error, kept as the error source.
        #[source]
        source: regex::Error,
    },
    /// The JSONPath library rejected an authored expression at extraction
    /// time. Zero matches never raise this; they normalize to `null`.
    #[error("JsonPath {path} failed: {reason}")]
    JsonPathFailure {
        /// The authored JSONPath expression.
        path: String,
        /// The JSONPath library's error message.
        reason: String,
    },
    /// Operator dispatch rejected the observed/expected pair for type-shape
    /// reasons that the typed catalog cannot encode at parse time.
    #[error("task {task_id:?}: operator {operator:?} cannot compare {reason}")]
    OperatorTypeMismatch {
        /// Id of the requirement whose comparison failed.
        task_id: TaskId,
        /// Debug rendering of the comparison operator.
        operator: String,
        /// The underlying operator error, rendered as text.
        reason: String,
    },
    /// Context capture hashing could not serialize a JSON value to bytes.
    #[error("failed to canonicalize context value for hashing: {reason}")]
    ContextHashFailed {
        /// The `serde_json` serialization error, rendered as text.
        reason: String,
    },
}
