//! JSONPath extraction over Verifier evidence.

use jsonpath_rust::JsonPathValue;
use serde_json::Value;
use wyrd_spec::verification::JsonPath;

use crate::error::ExecError;

/// Extract a JSON value at `path` from any JSON value.
///
/// The single Wyrd-typed surface over the workspace JSONPath dependency.
/// Executor code goes through this so the upstream error type never leaks
/// into engine code.
///
/// # Errors
/// Returns [`ExecError::JsonPathFailure`] when the path fails to compile
/// or evaluate.
pub fn extract_jsonpath_from(value: &Value, path: &JsonPath) -> Result<Value, ExecError> {
    let values = extract_jsonpath_values(value, path)?;
    Ok(values_to_value(values))
}

/// Compile `path` with the workspace JSONPath dependency and collect every
/// match against `value`, in document order.
///
/// This is the fallible half of [`extract_jsonpath_from`]: it owns the only
/// conversion from the upstream error type into [`ExecError`]. Borrowed slice
/// matches are cloned, computed matches are taken as-is, and the upstream
/// no-value marker is skipped, so an unmatched path yields an empty vector
/// rather than an error.
///
/// # Errors
/// Returns [`ExecError::JsonPathFailure`] carrying the authored path and the
/// upstream message when the expression fails to compile. Matching itself
/// does not fail.
fn extract_jsonpath_values(value: &Value, path: &JsonPath) -> Result<Vec<Value>, ExecError> {
    let query = jsonpath_rust::JsonPath::<Value>::try_from(path.as_str()).map_err(|error| {
        ExecError::JsonPathFailure {
            path: path.as_str().to_owned(),
            reason: error.to_string(),
        }
    })?;

    let mut values = Vec::new();
    for candidate in query.find_slice(value) {
        match candidate {
            JsonPathValue::Slice(value, _) => values.push(value.clone()),
            JsonPathValue::NewValue(value) => values.push(value),
            JsonPathValue::NoValue => {}
        }
    }

    Ok(values)
}

/// Normalize the collected JSONPath matches into one evidence value.
///
/// Zero matches become `null`, so a missing field is evidence for the
/// requirement operator to judge instead of an execution failure. Exactly one
/// match is returned unwrapped as that value, and two or more matches are
/// returned as a JSON array in match order. Infallible and IO-free.
fn values_to_value(values: Vec<Value>) -> Value {
    match values.as_slice() {
        [] => Value::Null,
        [only] => only.clone(),
        _ => Value::Array(values),
    }
}

/// Unit tests for the JSONPath extraction owner: scalar, multi-match, missing
/// path normalization, and the typed failure for an expression the upstream
/// library rejects.
#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    /// Build a [`JsonPath`] from a static test expression.
    ///
    /// Only Wyrd's authoring-time validation runs here; the upstream library
    /// may still reject the expression at extraction time.
    ///
    /// # Panics
    /// Panics when `expr` fails Wyrd [`JsonPath`] validation, which means the
    /// test literal itself is wrong.
    fn path(expr: &str) -> JsonPath {
        JsonPath::new(expr).expect("static test paths are valid")
    }

    /// A scalar path resolves to the value itself, not a one-element array.
    ///
    /// # Panics
    /// Panics when extraction fails or the extracted value is not `"hello"`.
    #[test]
    fn extracts_scalar() {
        let extracted = extract_jsonpath_from(&json!({"response": "hello"}), &path("$.response"))
            .expect("scalar extraction succeeds");
        assert_eq!(extracted, json!("hello"));
    }

    /// A wildcard path resolves to every match as one array in match order.
    ///
    /// # Panics
    /// Panics when extraction fails or the array differs from the two names.
    #[test]
    fn extracts_nested_array() {
        let value = json!({"tool_calls": [{"name": "search"}, {"name": "summarise"}]});
        let extracted = extract_jsonpath_from(&value, &path("$.tool_calls[*].name"))
            .expect("array extraction succeeds");
        assert_eq!(extracted, json!(["search", "summarise"]));
    }

    /// A path with no match resolves to null rather than failing.
    ///
    /// # Panics
    /// Panics when extraction returns an error or a non-null value.
    #[test]
    fn missing_path_returns_null_not_error() {
        let extracted = extract_jsonpath_from(&json!({"response": "hello"}), &path("$.never"))
            .expect("missing path resolves to null");
        assert_eq!(extracted, Value::Null);
    }

    /// A path that compiles at authoring time but fails at runtime surfaces the
    /// typed error naming the path.
    ///
    /// # Panics
    /// Panics when extraction succeeds or returns anything other than
    /// [`ExecError::JsonPathFailure`] carrying the authored path.
    #[test]
    fn invalid_runtime_path_surfaces_typed_error() {
        let bad = path("$.a[?(@.missing>)]");
        let error =
            extract_jsonpath_from(&json!({"a": 1}), &bad).expect_err("malformed predicate fails");
        assert!(matches!(
            error,
            ExecError::JsonPathFailure { ref path, .. } if path == bad.as_str()
        ));
    }
}
