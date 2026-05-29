//! Tool declaration contract and lightweight schema validation.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::{SkaldToolError, SkaldToolResult};

/// Declaration data for a callable tool.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolDef {
    /// Stable tool name used in provider-native tool calls.
    pub name: String,
    /// Human-readable tool description sent to providers.
    pub description: String,
    /// JSON object schema for the tool's input arguments.
    pub input_schema: Value,
}

impl ToolDef {
    /// Creates and validates a tool declaration.
    pub fn new(
        name: impl Into<String>,
        description: impl Into<String>,
        input_schema: Value,
    ) -> SkaldToolResult<Self> {
        let tool = Self {
            name: name.into(),
            description: description.into(),
            input_schema,
        };
        tool.validate()?;
        Ok(tool)
    }

    /// Validates the declaration using provider-safe local rules.
    pub fn validate(&self) -> SkaldToolResult<()> {
        validate_name(&self.name)?;
        if self.description.trim().is_empty() {
            return Err(SkaldToolError::invalid_schema(
                Some(self.name.clone()),
                "description must not be empty",
            ));
        }
        validate_input_schema(&self.name, &self.input_schema)
    }
}

fn validate_name(name: &str) -> SkaldToolResult<()> {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return Err(SkaldToolError::invalid_schema(
            None,
            "name must not be empty",
        ));
    };

    if !is_name_start(first) || !chars.all(is_name_continue) || name.len() > 64 {
        return Err(SkaldToolError::invalid_schema(
            Some(name.to_owned()),
            "name must start with an ASCII letter or underscore and contain only ASCII letters, digits, or underscores",
        ));
    }

    Ok(())
}

// Keep validation local and lightweight: enough to reject malformed tool input
// schemas before they are embedded in provider-native request structs.
fn validate_input_schema(tool: &str, schema: &Value) -> SkaldToolResult<()> {
    let Some(object) = schema.as_object() else {
        return Err(SkaldToolError::invalid_schema(
            Some(tool.to_owned()),
            "input_schema must be a JSON object",
        ));
    };

    if let Some(kind) = object.get("type") {
        validate_schema_type(tool, kind)?;
    }
    if let Some(properties) = object.get("properties")
        && !properties.is_object()
    {
        return Err(SkaldToolError::invalid_schema(
            Some(tool.to_owned()),
            "properties must be a JSON object",
        ));
    }
    if let Some(required) = object.get("required") {
        validate_required(tool, required)?;
    }
    if let Some(additional) = object.get("additionalProperties")
        && !additional.is_boolean()
        && !additional.is_object()
    {
        return Err(SkaldToolError::invalid_schema(
            Some(tool.to_owned()),
            "additionalProperties must be a boolean or JSON object",
        ));
    }

    Ok(())
}

// This is a meta-schema sanity check, not a full JSON Schema implementation.
fn validate_schema_type(tool: &str, kind: &Value) -> SkaldToolResult<()> {
    match kind {
        Value::String(value) if is_json_schema_type(value) => Ok(()),
        Value::Array(values)
            if values
                .iter()
                .all(|value| value.as_str().is_some_and(is_json_schema_type)) =>
        {
            Ok(())
        }
        _ => Err(SkaldToolError::invalid_schema(
            Some(tool.to_owned()),
            "type must be a JSON Schema primitive type or array of primitive types",
        )),
    }
}

// Providers expect required arguments to be named fields, not arbitrary values.
fn validate_required(tool: &str, required: &Value) -> SkaldToolResult<()> {
    match required {
        Value::Array(values) if values.iter().all(Value::is_string) => Ok(()),
        _ => Err(SkaldToolError::invalid_schema(
            Some(tool.to_owned()),
            "required must be an array of strings",
        )),
    }
}

fn is_json_schema_type(value: &str) -> bool {
    matches!(
        value,
        "null" | "boolean" | "object" | "array" | "number" | "string" | "integer"
    )
}

fn is_name_start(value: char) -> bool {
    value.is_ascii_alphabetic() || value == '_'
}

fn is_name_continue(value: char) -> bool {
    value.is_ascii_alphanumeric() || value == '_'
}
