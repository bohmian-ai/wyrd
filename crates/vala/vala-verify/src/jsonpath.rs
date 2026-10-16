//! JSONPath extraction over Verifier evidence.

use jsonpath_rust::JsonPathValue;
use serde_json::Value;
use wyrd_spec::vala::eval::JsonPath;

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

fn values_to_value(values: Vec<Value>) -> Value {
    match values.as_slice() {
        [] => Value::Null,
        [only] => only.clone(),
        _ => Value::Array(values),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    /// A static JSONPath.
    fn path(expr: &str) -> JsonPath {
        JsonPath::new(expr).expect("static test paths are valid")
    }

    /// A scalar path resolves to the value itself.
    #[test]
    fn extracts_scalar() {
        let extracted = extract_jsonpath_from(&json!({"response": "hello"}), &path("$.response"))
            .expect("scalar extraction succeeds");
        assert_eq!(extracted, json!("hello"));
    }

    /// A wildcard path resolves to every match as one array.
    #[test]
    fn extracts_nested_array() {
        let value = json!({"tool_calls": [{"name": "search"}, {"name": "summarise"}]});
        let extracted = extract_jsonpath_from(&value, &path("$.tool_calls[*].name"))
            .expect("array extraction succeeds");
        assert_eq!(extracted, json!(["search", "summarise"]));
    }

    /// A path with no match resolves to null rather than failing.
    #[test]
    fn missing_path_returns_null_not_error() {
        let extracted = extract_jsonpath_from(&json!({"response": "hello"}), &path("$.never"))
            .expect("missing path resolves to null");
        assert_eq!(extracted, Value::Null);
    }

    /// A path that compiles at authoring time but fails at runtime surfaces the
    /// typed error naming the path.
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
