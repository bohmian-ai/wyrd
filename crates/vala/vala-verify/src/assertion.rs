//! Deterministic comparisons and evidence capture for Verifier `assert` checks.

use serde_json::Value;
use sha2::{Digest, Sha256};
use wyrd_spec::vala::eval::result::EvalContextCapture;
use wyrd_spec::vala::eval::{ComparisonOperator, TaskId};

use crate::error::ExecError;
use crate::operators;

/// The compared outcome of one assertion: passed, observed actual, the
/// expected value compared, and a failure message.
pub(crate) type Compared = (bool, Option<Value>, Value, Option<String>);

/// Compare one `observed` value with `expected` under `operator`.
///
/// Shared by Eval assertion tasks and Verifier `assert` checks over a `path`.
///
/// # Errors
/// Returns [`ExecError::OperatorTypeMismatch`] for `task_id` when the
/// operator cannot compare the values.
pub(crate) fn evaluate_single(
    task_id: &TaskId,
    operator: &ComparisonOperator,
    expected: &Value,
    observed: &Value,
) -> Result<Compared, ExecError> {
    let verdict = operators::evaluate_operator(observed, operator, expected).map_err(|error| {
        ExecError::OperatorTypeMismatch {
            task_id: task_id.clone(),
            operator: format!("{operator:?}"),
            reason: error.to_string(),
        }
    })?;
    let message = if verdict.passed {
        None
    } else {
        Some(format!("operator {operator:?} returned false"))
    };
    Ok((
        verdict.passed,
        verdict.observed,
        verdict.expected.unwrap_or_else(|| expected.clone()),
        message,
    ))
}

/// Compare every element of the `observed` array with `expected`; the
/// assertion passes only when every element passes, so an empty array passes.
///
/// Shared by Eval item-mode assertions and Verifier `assert` checks over an
/// `each` selector.
///
/// # Errors
/// Returns [`ExecError::OperatorTypeMismatch`] for `task_id` when
/// `observed` is not an array or the operator cannot compare an element.
pub(crate) fn evaluate_item_mode(
    task_id: &TaskId,
    operator: &ComparisonOperator,
    expected: &Value,
    observed: &Value,
) -> Result<Compared, ExecError> {
    let mismatch = |reason: String| ExecError::OperatorTypeMismatch {
        task_id: task_id.clone(),
        operator: format!("{operator:?}"),
        reason,
    };
    let Value::Array(items) = observed else {
        return Err(mismatch(
            "item_context_path resolved to a non-array value".to_string(),
        ));
    };

    let mut failures = 0_usize;
    for item in items {
        let verdict = operators::evaluate_operator(item, operator, expected)
            .map_err(|error| mismatch(error.to_string()))?;
        if !verdict.passed {
            failures += 1;
        }
    }

    let message = if failures == 0 {
        None
    } else {
        Some(format!(
            "{failures}/{} items failed operator {operator:?}",
            items.len(),
        ))
    };
    Ok((
        failures == 0,
        Some(observed.clone()),
        expected.clone(),
        message,
    ))
}

/// Apply `capture` to one stored `actual` value: kept verbatim for `full`,
/// replaced by its SHA-256 hex digest for `hash`, and dropped for `redact`.
///
/// The one capture rule shared by Eval results and Verifier requirement
/// results.
///
/// # Errors
/// Returns `ContextHashFailed` if hashing cannot serialize the value.
pub fn capture_actual(
    actual: Option<serde_json::Value>,
    capture: EvalContextCapture,
) -> Result<Option<serde_json::Value>, ExecError> {
    match capture {
        EvalContextCapture::Full => Ok(actual),
        EvalContextCapture::Hash => actual
            .as_ref()
            .map(|value| hash_context_value(value).map(serde_json::Value::String))
            .transpose(),
        EvalContextCapture::Redact => Ok(None),
    }
}

/// SHA-256 hex digest of `value`'s canonical JSON bytes.
fn hash_context_value(value: &serde_json::Value) -> Result<String, ExecError> {
    let bytes = serde_json::to_vec(value).map_err(|error| ExecError::ContextHashFailed {
        reason: error.to_string(),
    })?;
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    Ok(format!("{:x}", hasher.finalize()))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    /// `full` keeps the actual value verbatim.
    #[test]
    fn context_capture_full_keeps_actual_verbatim() {
        let actual = capture_actual(Some(json!({"x": 1})), EvalContextCapture::Full)
            .expect("context capture succeeds");
        assert_eq!(actual, Some(json!({"x": 1})));
    }

    /// `hash` replaces the actual value with its lowercase SHA-256 hex digest.
    #[test]
    fn context_capture_hash_replaces_with_sha256_hex() {
        let actual = capture_actual(Some(json!({"x": 1, "y": "v"})), EvalContextCapture::Hash)
            .expect("context capture succeeds")
            .expect("actual is present");
        let hex = actual.as_str().expect("actual is hash string");
        assert_eq!(hex.len(), 64, "sha256 hex must be 64 chars");
        assert!(
            hex.chars()
                .all(|character| character.is_ascii_hexdigit() && !character.is_ascii_uppercase())
        );
    }

    /// `redact` drops the actual value.
    #[test]
    fn context_capture_redact_drops_actual() {
        let actual = capture_actual(Some(json!("pii")), EvalContextCapture::Redact)
            .expect("context capture succeeds");
        assert!(actual.is_none());
    }
}
