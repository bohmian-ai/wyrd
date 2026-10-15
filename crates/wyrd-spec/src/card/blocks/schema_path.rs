//! Known-missing field detection for references into a declared JSON Schema.
//!
//! Registration refuses a reference whose selectors cannot exist under a
//! known output schema. Anything the walker cannot decide — an unknown
//! keyword, an open object, a missing `type` — is admitted so unknown shapes
//! stay usable and fall back to missing-value handling at execution.

use serde_json::Value;

use super::reference::PathSegment;

/// Most `$ref` hops followed before a schema is treated as unknown.
const MAX_REF_HOPS: usize = 32;

/// Returns `false` only when `path` provably selects nothing under `schema`.
///
/// `root` resolves local `#/...` `$ref` pointers. `oneOf`/`anyOf` admit the
/// path when any branch does.
pub(super) fn admits(root: &Value, schema: &Value, path: &[PathSegment]) -> bool {
    let Some(schema) = resolve(root, schema) else {
        return true;
    };
    let Some((segment, rest)) = path.split_first() else {
        return true;
    };
    for keyword in ["oneOf", "anyOf"] {
        if let Some(branches) = schema.get(keyword).and_then(Value::as_array) {
            return branches.iter().any(|branch| admits(root, branch, path));
        }
    }
    match segment {
        PathSegment::Field(name) => {
            if let Some(property) = schema.get("properties").and_then(|props| props.get(name)) {
                return admits(root, property, rest);
            }
            if excludes_type(schema, "object") {
                return false;
            }
            match schema.get("additionalProperties") {
                Some(Value::Bool(false)) => false,
                Some(additional @ Value::Object(_)) => admits(root, additional, rest),
                _ => true,
            }
        }
        PathSegment::Index(_) | PathSegment::Wildcard => match schema.get("items") {
            Some(items @ Value::Object(_)) => admits(root, items, rest),
            Some(_) => true,
            None => !excludes_type(schema, "array"),
        },
    }
}

/// Follows local `$ref` pointers; `None` when a pointer cannot be resolved.
fn resolve<'a>(root: &'a Value, mut schema: &'a Value) -> Option<&'a Value> {
    for _ in 0..MAX_REF_HOPS {
        let Some(pointer) = schema.get("$ref").and_then(Value::as_str) else {
            return Some(schema);
        };
        schema = root.pointer(pointer.strip_prefix('#')?)?;
    }
    None
}

/// Returns whether the schema's declared `type` rules out `wanted`.
fn excludes_type(schema: &Value, wanted: &str) -> bool {
    match schema.get("type") {
        Some(Value::String(declared)) => declared != wanted,
        Some(Value::Array(declared)) => !declared.iter().any(|item| item == wanted),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    /// Field, index, and wildcard selectors are refused only when the schema
    /// proves them absent.
    ///
    /// # Panics
    /// Panics when a selector is admitted or refused incorrectly.
    #[test]
    fn schema_path_refuses_only_known_missing_selectors() {
        let schema = json!({
            "definitions": {"Item": {"type": "object", "properties": {"name": {"type": "string"}}, "additionalProperties": false}},
            "type": "object",
            "properties": {
                "items": {"type": "array", "items": {"$ref": "#/definitions/Item"}},
                "open": {"type": "object"},
                "maybe": {"anyOf": [{"$ref": "#/definitions/Item"}, {"type": "null"}]},
                "text": {"type": "string"}
            },
            "additionalProperties": false
        });
        let field = |name: &str| PathSegment::Field(name.to_owned());
        assert!(admits(
            &schema,
            &schema,
            &[field("items"), PathSegment::Wildcard, field("name")]
        ));
        assert!(admits(
            &schema,
            &schema,
            &[field("open"), field("anything")]
        ));
        assert!(admits(&schema, &schema, &[field("maybe"), field("name")]));
        assert!(!admits(&schema, &schema, &[field("missing")]));
        assert!(!admits(
            &schema,
            &schema,
            &[field("items"), PathSegment::Index(0), field("nope")]
        ));
        assert!(!admits(&schema, &schema, &[field("text"), field("x")]));
        assert!(!admits(
            &schema,
            &schema,
            &[field("text"), PathSegment::Index(0)]
        ));
        assert!(admits(&json!({}), &json!({}), &[field("x")]));
    }
}
