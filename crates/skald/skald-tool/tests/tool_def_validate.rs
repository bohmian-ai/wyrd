use serde_json::json;
use skald_tool::{SkaldToolError, ToolDef};

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
