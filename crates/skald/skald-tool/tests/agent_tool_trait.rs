use std::sync::Arc;

use serde_json::json;
use skald_tool::{AgentTool, ToolDef, ToolError};

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
