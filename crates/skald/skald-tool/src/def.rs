//! Tool declaration contract and lightweight schema validation.

use std::fmt;
use std::sync::Arc;

use async_trait::async_trait;
use schemars::JsonSchema;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::{SkaldToolError, SkaldToolResult};
use crate::toolerror::{ToolError, ToolResult};
use crate::trait_::AgentTool;

/// Declaration data for a callable tool.
#[derive(Clone, Serialize, Deserialize)]
pub struct ToolDef {
    /// Stable tool name used in provider-native tool calls.
    pub name: String,
    /// Human-readable tool description sent to providers.
    pub description: String,
    /// JSON object schema for the tool's input arguments.
    pub input_schema: Value,
    /// JSON schema for the tool's output value.
    #[serde(default = "default_output_schema")]
    pub output_schema: Value,
    #[serde(skip)]
    call: Option<Arc<dyn Fn(Value) -> ToolResult<Value> + Send + Sync>>,
}

impl fmt::Debug for ToolDef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ToolDef")
            .field("name", &self.name)
            .field("description", &self.description)
            .field("input_schema", &self.input_schema)
            .field("output_schema", &self.output_schema)
            .finish_non_exhaustive()
    }
}

impl PartialEq for ToolDef {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
            && self.description == other.description
            && self.input_schema == other.input_schema
            && self.output_schema == other.output_schema
    }
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
            output_schema: default_output_schema(),
            call: None,
        };
        tool.validate()?;
        Ok(tool)
    }

    /// Creates an executable tool from a synchronous typed Rust closure.
    pub fn function<F, In, Out>(
        name: impl Into<String>,
        description: impl Into<String>,
        f: F,
    ) -> Self
    where
        F: Fn(In) -> ToolResult<Out> + Send + Sync + 'static,
        In: DeserializeOwned + JsonSchema + Send + 'static,
        Out: Serialize + JsonSchema + 'static,
    {
        let input_schema = match serde_json::to_value(schemars::schema_for!(In)) {
            Ok(value) => value,
            Err(error) => serde_json::json!({ "error": error.to_string() }),
        };
        let output_schema = match serde_json::to_value(schemars::schema_for!(Out)) {
            Ok(value) => value,
            Err(error) => serde_json::json!({ "error": error.to_string() }),
        };

        let call = Arc::new(move |args: Value| -> ToolResult<Value> {
            let typed_in: In =
                serde_json::from_value(args).map_err(|e| ToolError::InvalidInput(e.to_string()))?;
            let typed_out: Out = f(typed_in)?;
            serde_json::to_value(typed_out)
                .map_err(|e| ToolError::OutputSerialization(e.to_string()))
        });

        Self {
            name: name.into(),
            description: description.into(),
            input_schema,
            output_schema,
            call: Some(call),
        }
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

#[async_trait]
impl AgentTool for ToolDef {
    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> &str {
        &self.description
    }

    fn input_schema(&self) -> Value {
        self.input_schema.clone()
    }

    fn output_schema(&self) -> Value {
        self.output_schema.clone()
    }

    async fn invoke(&self, args: Value) -> Result<Value, ToolError> {
        let call = self.call.as_ref().ok_or_else(|| ToolError::Invocation {
            detail: format!("tool '{}' has no executable closure", self.name),
            cause: None,
        })?;
        call(args)
    }
}

fn default_output_schema() -> Value {
    serde_json::json!({})
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

#[cfg(test)]
mod agent_tool_trait {
    use std::sync::Arc;

    use crate::{AgentTool, ToolDef, ToolError};
    use serde_json::json;

    #[derive(serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
    struct Hit {
        url: String,
        score: f64,
    }

    #[tokio::test]
    async fn tooldef_function_constructs_with_derived_schemas() {
        let tool = ToolDef::function("hello", "greets the user", |s: String| {
            Ok::<_, ToolError>(format!("hi {s}"))
        });

        let expected_in =
            serde_json::to_value(schemars::schema_for!(String)).expect("schema serializes");
        let expected_out =
            serde_json::to_value(schemars::schema_for!(String)).expect("schema serializes");

        assert_eq!(tool.input_schema(), expected_in);
        assert_eq!(tool.output_schema(), expected_out);
    }

    #[tokio::test]
    async fn tooldef_invoke_round_trips_json() {
        let tool = ToolDef::function("hello", "greets the user", |s: String| {
            Ok::<_, ToolError>(format!("hi {s}"))
        });

        let out = tool.invoke(json!("steven")).await.unwrap();

        assert_eq!(out, json!("hi steven"));
    }

    #[tokio::test]
    async fn tooldef_invoke_rejects_mismatched_input() {
        let tool = ToolDef::function("hello", "greets the user", |s: String| {
            Ok::<_, ToolError>(format!("hi {s}"))
        });

        let err = tool.invoke(json!(42)).await.unwrap_err();

        assert!(matches!(err, ToolError::InvalidInput(_)));
        assert_eq!(err.code(), "SKALD_TOOL_422_INPUT");
    }

    #[tokio::test]
    async fn tooldef_invoke_wraps_closure_error_as_invocation() {
        let tool = ToolDef::function::<_, String, String>("boomy", "always fails", |_s: String| {
            Err::<String, _>(ToolError::Invocation {
                detail: "boom".into(),
                cause: None,
            })
        });

        let err = tool.invoke(json!("anything")).await.unwrap_err();

        assert!(matches!(&err, ToolError::Invocation { detail, .. } if detail == "boom"));
        assert_eq!(err.code(), "SKALD_TOOL_500_CALL");
    }

    #[test]
    fn agent_tool_is_object_safe() {
        let tool = ToolDef::function("hello", "", |s: String| Ok::<_, ToolError>(s));
        let _: Arc<dyn AgentTool> = Arc::new(tool);
    }

    #[test]
    fn agent_tool_name_and_description_are_owned_strings() {
        let tool = ToolDef::function(
            String::from("dynamic_name"),
            String::from("dyn desc"),
            |s: String| Ok::<_, ToolError>(s),
        );

        assert_eq!(tool.name(), "dynamic_name");
        assert_eq!(tool.description(), "dyn desc");
    }

    #[test]
    fn tooldef_input_schema_is_serde_json_value() {
        let tool = ToolDef::function("hello", "greets the user", |s: String| {
            Ok::<_, ToolError>(format!("hi {s}"))
        });

        let schema = tool.input_schema();

        assert!(schema.is_object());
        assert_eq!(schema.get("type").and_then(|v| v.as_str()), Some("string"));
    }

    #[tokio::test]
    async fn tooldef_output_schema_for_struct_with_jsonschema_derive() {
        let tool = ToolDef::function("search", "returns a Hit", |q: String| {
            Ok::<_, ToolError>(Hit {
                url: format!("https://example.com/{q}"),
                score: 0.5,
            })
        });

        let schema = tool.output_schema();
        let props = schema.get("properties").expect("schema has properties");
        assert!(
            props.get("url").is_some(),
            "expected url field in output schema"
        );
        assert!(
            props.get("score").is_some(),
            "expected score field in output schema"
        );

        let out = tool.invoke(json!("rust")).await.unwrap();
        assert!(out.get("url").is_some());
        assert!(out.get("score").is_some());
    }
}

#[cfg(test)]
mod tool_def_validate {
    use crate::{SkaldToolError, ToolDef};
    use serde_json::json;

    #[test]
    fn valid_tool_def_passes() {
        let tool = ToolDef::new(
            "lookup_weather",
            "Look up the weather forecast.",
            json!({
                "type": "object",
                "properties": {
                    "city": { "type": "string" }
                },
                "required": ["city"],
                "additionalProperties": false
            }),
        )
        .expect("valid tool declaration");

        assert_eq!(tool.name, "lookup_weather");
    }

    #[test]
    fn invalid_name_rejected() {
        let error = ToolDef::new(
            "weather-lookup",
            "Look up the weather forecast.",
            json!({ "type": "object" }),
        )
        .expect_err("invalid name is rejected");

        assert_invalid_schema(error);
    }

    #[test]
    fn invalid_meta_schema_rejected() {
        let error = ToolDef::new(
            "lookup_weather",
            "Look up the weather forecast.",
            json!({ "type": 7 }),
        )
        .expect_err("invalid schema type is rejected");

        assert_invalid_schema(error);
    }

    #[test]
    fn empty_description_rejected() {
        let error = ToolDef::new("lookup_weather", "  ", json!({ "type": "object" }))
            .expect_err("empty description is rejected");

        assert_invalid_schema(error);
    }

    #[test]
    fn parameters_schema_must_be_object() {
        let error = ToolDef::new(
            "lookup_weather",
            "Look up the weather forecast.",
            json!(true),
        )
        .expect_err("non-object schema is rejected");

        assert_invalid_schema(error);
    }

    fn assert_invalid_schema(error: SkaldToolError) {
        assert_eq!(error.code(), "SKALD_TOOL_400_INVALID_SCHEMA");
        assert!(matches!(error, SkaldToolError::InvalidSchema { .. }));
    }
}
