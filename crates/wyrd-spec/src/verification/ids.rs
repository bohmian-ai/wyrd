//! Verifier step and requirement identifiers and evidence paths.

use std::str::FromStr;
use std::sync::OnceLock;

use regex::Regex;
use serde::{Deserialize, Deserializer, Serialize};

use crate::error::WyrdError;

/// Validated identifier of a Verifier step or requirement.
///
/// Task IDs must match `^[a-zA-Z_][a-zA-Z0-9_-]*$` and have length `1..=128`.
/// Deserialization goes through [`TaskId::new`] so wire payloads cannot bypass
/// validation.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(transparent)]
pub struct TaskId(String);

impl TaskId {
    /// Constructs a validated task identifier.
    ///
    /// # Errors
    /// Returns [`WyrdError::Validation`] when the identifier is empty, longer
    /// than 128 characters, or does not match the task-id grammar.
    pub fn new(value: impl Into<String>) -> Result<Self, WyrdError> {
        let value = value.into();
        validate_id_grammar("task_id", &value)?;
        Ok(Self(value))
    }

    /// Borrows the validated identifier as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl FromStr for TaskId {
    type Err = WyrdError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::new(value)
    }
}

impl<'de> Deserialize<'de> for TaskId {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        Self::new(value).map_err(serde::de::Error::custom)
    }
}

impl schemars::JsonSchema for TaskId {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "TaskId".into()
    }

    fn json_schema(_generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({ "type": "string", "minLength": 1, "maxLength": 128, "pattern": r"^[a-zA-Z_][a-zA-Z0-9_-]*$" })
    }
}

/// Validated JSONPath expression used to address workflow and scenario fields.
///
/// This surface accepts a small RFC 9535-style subset: the path must start
/// with `$`, be at most 1024 characters, contain no control characters, and
/// have balanced brackets and quotes. Deserialization goes through
/// [`JsonPath::new`] so wire payloads cannot bypass validation.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(transparent)]
pub struct JsonPath(String);

impl JsonPath {
    /// Constructs a validated JSONPath expression.
    ///
    /// # Errors
    /// Returns [`WyrdError::Validation`] when the path does not begin with
    /// `$`, exceeds 1024 characters, contains control characters, or has
    /// unbalanced brackets or quotes.
    pub fn new(value: impl Into<String>) -> Result<Self, WyrdError> {
        let value = value.into();
        if !value.starts_with('$') {
            return Err(WyrdError::Validation {
                message: "json_path must start with `$`".to_string(),
                details: serde_json::Value::Null,
            });
        }

        if value.len() > 1024 {
            return Err(WyrdError::Validation {
                message: format!("json_path length must be <= 1024, got {}", value.len()),
                details: serde_json::Value::Null,
            });
        }

        let mut bracket_depth = 0_i32;
        let mut in_single_quote = false;
        let mut in_double_quote = false;
        let mut escaped = false;
        for ch in value.chars() {
            if ch.is_control() {
                return Err(WyrdError::Validation {
                    message: "json_path contains control characters".to_string(),
                    details: serde_json::Value::Null,
                });
            }

            if ch == '\\' {
                escaped = !escaped;
                continue;
            }

            match ch {
                '\'' if !in_double_quote && !escaped => in_single_quote = !in_single_quote,
                '"' if !in_single_quote && !escaped => in_double_quote = !in_double_quote,
                '[' if !in_single_quote && !in_double_quote => bracket_depth += 1,
                ']' if !in_single_quote && !in_double_quote => {
                    bracket_depth -= 1;
                    if bracket_depth < 0 {
                        return Err(WyrdError::Validation {
                            message: "json_path has unbalanced `]`".to_string(),
                            details: serde_json::Value::Null,
                        });
                    }
                }
                _ => {}
            }
            escaped = false;
        }

        if bracket_depth != 0 || in_single_quote || in_double_quote {
            return Err(WyrdError::Validation {
                message: "json_path has unbalanced brackets or quotes".to_string(),
                details: serde_json::Value::Null,
            });
        }

        Ok(Self(value))
    }

    /// Borrows the validated path as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl FromStr for JsonPath {
    type Err = WyrdError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::new(value)
    }
}

impl<'de> Deserialize<'de> for JsonPath {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        Self::new(value).map_err(serde::de::Error::custom)
    }
}

impl schemars::JsonSchema for JsonPath {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "JsonPath".into()
    }

    fn json_schema(_generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({ "type": "string", "minLength": 1, "maxLength": 1024, "pattern": r"^\$.*$" })
    }
}

fn validate_id_grammar(field_name: &str, value: &str) -> Result<(), WyrdError> {
    if value.is_empty() || value.len() > 128 {
        return Err(WyrdError::Validation {
            message: format!("{field_name} length must be 1..=128, got {}", value.len()),
            details: serde_json::Value::Null,
        });
    }
    if !task_id_regex().is_match(value) {
        return Err(WyrdError::Validation {
            message: format!("{field_name} {value:?} must match ^[a-zA-Z_][a-zA-Z0-9_-]*$"),
            details: serde_json::Value::Null,
        });
    }
    Ok(())
}

fn task_id_regex() -> &'static Regex {
    static TASK_ID_REGEX: OnceLock<Regex> = OnceLock::new();
    TASK_ID_REGEX.get_or_init(|| match Regex::new(r"^[a-zA-Z_][a-zA-Z0-9_-]*$") {
        Ok(regex) => regex,
        Err(error) => panic!("task_id regex is static and valid: {error}"),
    })
}

#[cfg(test)]
mod tests {
    use super::{JsonPath, TaskId};

    #[test]
    fn task_id_accepts_simple_identifier() {
        let id = TaskId::new("step_one").expect("valid task id");
        assert_eq!(id.as_str(), "step_one");
    }

    #[test]
    fn task_id_accepts_hyphen_after_first_char() {
        TaskId::new("step-one").expect("valid task id");
    }

    #[test]
    fn task_id_rejects_leading_digit() {
        assert!(TaskId::new("1step").is_err());
    }

    #[test]
    fn task_id_rejects_empty() {
        assert!(TaskId::new("").is_err());
    }

    #[test]
    fn task_id_rejects_overlong() {
        let long = "a".repeat(129);
        assert!(TaskId::new(long).is_err());
    }

    #[test]
    fn task_id_serde_transparent() {
        let id = TaskId::new("alpha").expect("valid task id");
        let json = serde_json::to_string(&id).expect("task id serializes");
        assert_eq!(json, "\"alpha\"");
        let round_trip: TaskId = serde_json::from_str(&json).expect("task id deserializes");
        assert_eq!(id, round_trip);
    }

    #[test]
    fn task_id_deserialize_rejects_leading_digit() {
        let result: Result<TaskId, _> = serde_json::from_str("\"1step\"");
        assert!(result.is_err());
    }

    #[test]
    fn task_id_deserialize_rejects_overlong() {
        let long = format!("\"{}\"", "a".repeat(129));
        let result: Result<TaskId, _> = serde_json::from_str(&long);
        assert!(result.is_err());
    }

    #[test]
    fn task_id_deserialize_rejects_empty_string() {
        let result: Result<TaskId, _> = serde_json::from_str("\"\"");
        assert!(result.is_err());
    }

    #[test]
    fn task_id_deserialize_rejects_disallowed_char() {
        let result: Result<TaskId, _> = serde_json::from_str("\"has space\"");
        assert!(result.is_err());
    }

    #[test]
    fn json_path_accepts_root() {
        JsonPath::new("$").expect("valid json path");
    }

    #[test]
    fn json_path_accepts_dot_segments() {
        JsonPath::new("$.response.message").expect("valid json path");
    }

    #[test]
    fn json_path_accepts_bracket_indices() {
        JsonPath::new("$.docs[0].title").expect("valid json path");
    }

    #[test]
    fn json_path_accepts_quoted_keys() {
        JsonPath::new("$['weird key'].value").expect("valid json path");
    }

    #[test]
    fn json_path_rejects_missing_dollar() {
        assert!(JsonPath::new("response").is_err());
    }

    #[test]
    fn json_path_rejects_unbalanced_bracket() {
        assert!(JsonPath::new("$.docs[0").is_err());
    }

    #[test]
    fn json_path_rejects_unbalanced_quote() {
        assert!(JsonPath::new("$['unterminated").is_err());
    }

    #[test]
    fn json_path_rejects_control_char() {
        assert!(JsonPath::new("$.a\nb").is_err());
    }

    #[test]
    fn json_path_deserialize_rejects_missing_dollar() {
        let result: Result<JsonPath, _> = serde_json::from_str("\"response\"");
        assert!(result.is_err());
    }

    #[test]
    fn json_path_deserialize_rejects_unbalanced_bracket() {
        let result: Result<JsonPath, _> = serde_json::from_str("\"$.docs[0\"");
        assert!(result.is_err());
    }

    #[test]
    fn json_path_accepts_key_ending_in_escaped_backslash() {
        // $['a\\'] means the key "a\" (backslash at end).
        // Old code rejected this: the second `\` was `previous`, so it mistakenly
        // treated the closing `'` as escaped and never closed in_single_quote.
        // New code (escaped boolean) correctly toggles in_single_quote on the `'`.
        JsonPath::new(r"$['a\\']").expect("key ending with escaped backslash is valid");
    }

    #[test]
    fn json_path_accepts_escaped_backslash_then_bracket() {
        JsonPath::new(r"$['a\\'][0]").expect("escaped backslash then bracket is valid");
    }

    #[test]
    fn json_path_deserialize_rejects_unbalanced_quote() {
        let result: Result<JsonPath, _> = serde_json::from_str("\"$['unterminated\"");
        assert!(result.is_err());
    }

    #[test]
    fn json_path_deserialize_rejects_control_char() {
        let result: Result<JsonPath, _> = serde_json::from_str("\"$.a\\nb\"");
        assert!(result.is_err());
    }
}
