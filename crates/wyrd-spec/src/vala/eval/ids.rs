//! Eval-scoped identifier and path types.
//!
//! Vala-wide ids live in [`crate::vala::ids`] and are re-exported here so
//! existing `wyrd_spec::vala::eval::ids::*` imports continue to resolve.

pub use crate::ids::DataTenantId;
pub use crate::vala::ids::{
    EntityUid, LeaseToken, RecordId, RunId, SessionId, SpanId, TraceId, WorkflowUid,
};

use std::str::FromStr;
use std::sync::OnceLock;

use regex::Regex;
use serde::{Deserialize, Deserializer, Serialize};

use crate::error::WyrdError;

/// Validated task identifier used as the task-map key inside an eval spec.
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
    fn schema_name() -> String {
        "TaskId".to_string()
    }

    fn json_schema(_generator: &mut schemars::r#gen::SchemaGenerator) -> schemars::schema::Schema {
        use schemars::schema::{InstanceType, SchemaObject, SingleOrVec, StringValidation};

        SchemaObject {
            instance_type: Some(SingleOrVec::Single(Box::new(InstanceType::String))),
            string: Some(Box::new(StringValidation {
                max_length: Some(128),
                min_length: Some(1),
                pattern: Some(r"^[a-zA-Z_][a-zA-Z0-9_-]*$".to_string()),
            })),
            ..Default::default()
        }
        .into()
    }
}

/// Validated scenario identifier used by eval-scenario payloads.
///
/// Scenario IDs share the same grammar as [`TaskId`] but keep a distinct type
/// because they occupy a different namespace and may diverge later.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(transparent)]
pub struct ScenarioId(String);

impl ScenarioId {
    /// Constructs a validated scenario identifier.
    ///
    /// # Errors
    /// Returns [`WyrdError::Validation`] when the identifier is empty, longer
    /// than 128 characters, or does not match the shared scenario-id grammar.
    pub fn new(value: impl Into<String>) -> Result<Self, WyrdError> {
        let value = value.into();
        validate_id_grammar("scenario_id", &value)?;
        Ok(Self(value))
    }

    /// Borrows the validated identifier as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl FromStr for ScenarioId {
    type Err = WyrdError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::new(value)
    }
}

impl<'de> Deserialize<'de> for ScenarioId {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        Self::new(value).map_err(serde::de::Error::custom)
    }
}

impl schemars::JsonSchema for ScenarioId {
    fn schema_name() -> String {
        "ScenarioId".to_string()
    }

    fn json_schema(_generator: &mut schemars::r#gen::SchemaGenerator) -> schemars::schema::Schema {
        use schemars::schema::{InstanceType, SchemaObject, SingleOrVec, StringValidation};

        SchemaObject {
            instance_type: Some(SingleOrVec::Single(Box::new(InstanceType::String))),
            string: Some(Box::new(StringValidation {
                max_length: Some(128),
                min_length: Some(1),
                pattern: Some(r"^[a-zA-Z_][a-zA-Z0-9_-]*$".to_string()),
            })),
            ..Default::default()
        }
        .into()
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
    fn schema_name() -> String {
        "JsonPath".to_string()
    }

    fn json_schema(_generator: &mut schemars::r#gen::SchemaGenerator) -> schemars::schema::Schema {
        use schemars::schema::{InstanceType, SchemaObject, SingleOrVec, StringValidation};

        SchemaObject {
            instance_type: Some(SingleOrVec::Single(Box::new(InstanceType::String))),
            string: Some(Box::new(StringValidation {
                max_length: Some(1024),
                min_length: Some(1),
                pattern: Some(r"^\$.*$".to_string()),
            })),
            ..Default::default()
        }
        .into()
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
