//! Deterministic JSON helpers shared by cards.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use serde::Serialize;
use serde_json::{Map, Value};

use crate::WyrdUtilsResult;

/// Return a stable pretty JSON string for Python `__str__` implementations.
///
/// Python special methods should be infallible. When serialization fails, the
/// returned string names the failure instead of panicking.
#[must_use]
pub fn pretty_json_string<T: Serialize + ?Sized>(value: &T) -> String {
    match serde_json::to_string_pretty(value) {
        Ok(json) => json,
        Err(error) => format!("failed to serialize JSON: {error}"),
    }
}

/// Write JSON with recursively sorted object keys.
///
/// # Errors
/// Returns an error when serialization or filesystem writes fail.
pub fn write_json_sorted<T: Serialize>(path: impl AsRef<Path>, value: &T) -> WyrdUtilsResult<()> {
    let value = serde_json::to_value(value)?;
    let sorted = sort_json_value(value);
    let bytes = serde_json::to_vec_pretty(&sorted)?;
    if let Some(parent) = path.as_ref().parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, bytes)?;
    Ok(())
}

fn sort_json_value(value: Value) -> Value {
    match value {
        Value::Array(values) => Value::Array(values.into_iter().map(sort_json_value).collect()),
        Value::Object(values) => {
            let sorted: BTreeMap<String, Value> = values
                .into_iter()
                .map(|(key, value)| (key, sort_json_value(value)))
                .collect();
            Value::Object(Map::from_iter(sorted))
        }
        value => value,
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use serde_json::json;

    use crate::json::write_json_sorted;
    use crate::uuid7;

    #[test]
    fn write_json_sorted_is_byte_stable() {
        let dir = std::env::temp_dir().join(format!("wyrd-utils-json-{}", uuid7()));
        fs::create_dir_all(&dir).expect("temp dir should be created");
        let path = dir.join("out.json");
        let value = json!({"z": 1, "a": {"b": 2, "a": 3}});

        write_json_sorted(&path, &value).expect("json should be written");
        let written = fs::read_to_string(&path).expect("json should be readable");

        assert_eq!(
            written,
            "{\n  \"a\": {\n    \"a\": 3,\n    \"b\": 2\n  },\n  \"z\": 1\n}"
        );
        fs::remove_dir_all(dir).expect("temp dir should be removed");
    }
}
