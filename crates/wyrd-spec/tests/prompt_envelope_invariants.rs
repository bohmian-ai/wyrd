mod prompt_support;

use prompt_support::{json_object_schema, openai_chat_request, prompt};
use skald_spec::{ProviderRequest, ResponseType};
use wyrd_spec::PromptSpec;

#[test]
fn invariant_invalid_variable_name_rejected() {
    let err = PromptSpec::new(prompt(
        openai_chat_request("hello {{name}}"),
        vec!["bad-name"],
    ))
    .expect_err("invalid name rejected");

    assert_eq!(err.code(), "WYRD_PROMPT_400_INVALID_VARIABLE_NAME");
}

#[test]
fn invariant_duplicate_variable_rejected() {
    let err = PromptSpec::new(prompt(
        openai_chat_request("hello {{name}}"),
        vec!["name", "name"],
    ))
    .expect_err("duplicate rejected");

    assert_eq!(err.code(), "WYRD_PROMPT_409_DUPLICATE_VARIABLE");
}

#[test]
fn invariant_undeclared_placeholder_rejected() {
    let err = PromptSpec::new(prompt(openai_chat_request("hello {{name}}"), Vec::new()))
        .expect_err("undeclared placeholder rejected");

    assert_eq!(err.code(), "WYRD_PROMPT_422_UNDECLARED_PLACEHOLDER");
}

#[test]
fn invariant_unreferenced_variable_rejected() {
    let err = PromptSpec::new(prompt(openai_chat_request("hello"), vec!["name"]))
        .expect_err("unreferenced variable rejected");

    assert_eq!(err.code(), "WYRD_PROMPT_422_UNREFERENCED_VARIABLE");
}

#[test]
fn invariant_empty_model_rejected() {
    let mut prompt = prompt(openai_chat_request("hello"), Vec::new());
    prompt.model = "   ".to_owned();
    let err = PromptSpec::new(prompt).expect_err("empty model rejected");

    assert_eq!(err.code(), "WYRD_PROMPT_400_EMPTY_MODEL");
}

#[test]
fn invariant_non_object_json_schema_rejected() {
    let mut prompt = prompt(openai_chat_request("hello"), Vec::new());
    prompt.response_type = ResponseType::JsonSchema {
        name: "bad".to_owned(),
        schema: serde_json::json!(["not", "object"]),
    };
    let err = PromptSpec::new(prompt).expect_err("schema rejected");

    assert_eq!(err.code(), "WYRD_PROMPT_400_INVALID_RESPONSE_SCHEMA");
}

#[test]
fn invariant_object_json_schema_accepts() {
    let mut prompt = prompt(openai_chat_request("hello"), Vec::new());
    prompt.response_type = ResponseType::JsonSchema {
        name: "ok".to_owned(),
        schema: json_object_schema(),
    };

    assert!(PromptSpec::new(prompt).is_ok());
}

#[test]
fn raw_v1_with_no_placeholders_validates() {
    let raw = ProviderRequest::RawV1 {
        provider: skald_spec::ProviderName::Custom("acme".to_owned()),
        body: serde_json::value::to_raw_value(&serde_json::json!({"x": 1})).expect("raw"),
    };

    assert!(PromptSpec::new(prompt(raw, Vec::new())).is_ok());
}
