//! Deterministic comparisons and evidence capture for Verifier `assert` checks.

use std::borrow::Cow;

use serde_json::Value;
use sha2::{Digest, Sha256};
use wyrd_spec::verification::ContextCapture;
use wyrd_spec::verification::{ComparisonOperator, TaskId};

use crate::error::ExecError;
use crate::operators;

/// The compared outcome of one assertion: passed, the observed actual,
/// borrowed when it is the compared input itself, and a failure message.
pub(crate) type Compared<'a> = (bool, Option<Cow<'a, Value>>, Option<String>);

/// Compare one `observed` value with `expected` under `operator`.
///
/// Shared by Verifier `assert` checks over a `path`.
///
/// # Errors
/// Returns [`ExecError::OperatorTypeMismatch`] for `task_id` when the
/// operator cannot compare the values.
pub(crate) fn evaluate_single<'a>(
    task_id: &TaskId,
    operator: &ComparisonOperator,
    expected: &'a Value,
    observed: &'a Value,
) -> Result<Compared<'a>, ExecError> {
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
    Ok((verdict.passed, verdict.observed, message))
}

/// Compare every element of the `observed` array with `expected`; the
/// assertion passes only when every element passes. The engine judges an
/// empty selection inconclusive before comparing, so none reaches here.
///
/// Shared by item-mode Verifier `assert` checks over an `each` selector.
///
/// # Errors
/// Returns [`ExecError::OperatorTypeMismatch`] for `task_id` when
/// `observed` is not an array or the operator cannot compare an element.
pub(crate) fn evaluate_item_mode<'a>(
    task_id: &TaskId,
    operator: &ComparisonOperator,
    expected: &Value,
    observed: &'a Value,
) -> Result<Compared<'a>, ExecError> {
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
    Ok((failures == 0, Some(Cow::Borrowed(observed)), message))
}

/// Apply `capture` to one `actual` value: kept verbatim for `full`,
/// replaced by its SHA-256 hex digest for `hash`, and dropped for `redact`.
///
/// The one capture rule shared by Verifier requirement results. A borrowed
/// value is copied only when `full` stores it; `hash` reads it in place.
///
/// # Errors
/// Returns `ContextHashFailed` if hashing cannot serialize the value.
pub fn capture_actual(
    actual: Option<Cow<'_, Value>>,
    capture: ContextCapture,
) -> Result<Option<Value>, ExecError> {
    match capture {
        ContextCapture::Full => Ok(actual.map(Cow::into_owned)),
        ContextCapture::Hash => actual
            .as_ref()
            .map(|value| hash_context_value(value).map(serde_json::Value::String))
            .transpose(),
        ContextCapture::Redact => Ok(None),
    }
}

/// Lowercase SHA-256 hex digest of `value`'s compact `serde_json` bytes.
///
/// Backs [`ContextCapture::Hash`] in [`capture_actual`]. The bytes are exactly
/// what [`serde_json::to_vec`] emits: compact output with no insignificant
/// whitespace, object keys in the `Value` map's iteration order, and
/// `serde_json` number and string escaping. Workspace dependencies enable
/// `serde_json/preserve_order`, so key order is insertion or parse order, not
/// sorted: equal objects built in different key orders hash differently.
/// This is not RFC 8785 JCS; changing the byte form changes every stored
/// digest. The bytes stream straight into the hasher, so no serialized copy
/// of `value` is held.
///
/// # Errors
/// Returns [`ExecError::ContextHashFailed`] with the serializer message when
/// `serde_json` cannot serialize `value`.
fn hash_context_value(value: &serde_json::Value) -> Result<String, ExecError> {
    let mut hasher = Sha256::new();
    serde_json::to_writer(&mut hasher, value).map_err(|error| ExecError::ContextHashFailed {
        reason: error.to_string(),
    })?;
    Ok(format!("{:x}", hasher.finalize()))
}

/// Unit tests for [`capture_actual`]: each [`ContextCapture`] mode applied
/// directly to one actual value, without running a Verifier.
#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    /// `full` keeps the actual value verbatim.
    ///
    /// # Panics
    /// Panics when capture fails or the captured value differs from the input.
    #[test]
    fn context_capture_full_keeps_actual_verbatim() {
        let actual = capture_actual(Some(Cow::Owned(json!({"x": 1}))), ContextCapture::Full)
            .expect("context capture succeeds");
        assert_eq!(actual, Some(json!({"x": 1})));
    }

    /// `hash` replaces the actual value with its lowercase SHA-256 hex digest.
    ///
    /// # Panics
    /// Panics when capture fails, drops the value, returns a non-string, or
    /// returns anything other than 64 lowercase hex characters.
    #[test]
    fn context_capture_hash_replaces_with_sha256_hex() {
        let actual = capture_actual(
            Some(Cow::Borrowed(&json!({"x": 1, "y": "v"}))),
            ContextCapture::Hash,
        )
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
    ///
    /// # Panics
    /// Panics when capture fails or still returns a value.
    #[test]
    fn context_capture_redact_drops_actual() {
        let actual = capture_actual(Some(Cow::Owned(json!("pii"))), ContextCapture::Redact)
            .expect("context capture succeeds");
        assert!(actual.is_none());
    }
}
