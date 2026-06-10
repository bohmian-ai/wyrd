use serde_json::value::RawValue;
use wyrd_observe::{REDACTED_PLACEHOLDER, RedactionPolicy};

fn raw(json: &str) -> Box<RawValue> {
    RawValue::from_string(json.to_owned()).expect("test JSON must be valid")
}

#[test]
fn top_level_object_blocked_fields_are_replaced() {
    let policy = RedactionPolicy::strict();
    let out = policy.redact_tool_args(
        "login",
        &raw(r#"{"username":"alice","password":"hunter2","api_key":"sk-abc"}"#),
    );

    assert!(out.get().contains("\"username\":\"alice\""));
    assert!(out.get().contains(REDACTED_PLACEHOLDER));
    assert!(!out.get().contains("hunter2"));
    assert!(!out.get().contains("sk-abc"));
}

#[test]
fn case_insensitive_match() {
    let policy = RedactionPolicy::strict();
    let out = policy.redact_tool_args(
        "login",
        &raw(r#"{"Password":"x","API_KEY":"y","Authorization":"z"}"#),
    );

    assert!(!out.get().contains("\"x\""));
    assert!(!out.get().contains("\"y\""));
    assert!(!out.get().contains("\"z\""));
}

#[test]
fn nested_object_blocked_fields_are_replaced() {
    let policy = RedactionPolicy::strict();
    let out = policy.redact_tool_args(
        "auth",
        &raw(r#"{"creds":{"username":"alice","password":"hunter2"}}"#),
    );

    assert!(out.get().contains("\"username\":\"alice\""));
    assert!(!out.get().contains("hunter2"));
}

#[test]
fn array_of_objects_is_walked() {
    let policy = RedactionPolicy::strict();
    let out = policy.redact_tool_args("batch", &raw(r#"{"items":[{"token":"a"},{"token":"b"}]}"#));

    assert!(!out.get().contains("\"a\""));
    assert!(!out.get().contains("\"b\""));
    assert_eq!(out.get().matches(REDACTED_PLACEHOLDER).count(), 2);
}

#[test]
fn empty_blocklist_is_a_pass_through() {
    let mut policy = RedactionPolicy::strict();
    policy.blocked_arg_fields.clear();
    let out = policy.redact_tool_args("login", &raw(r#"{"password":"hunter2"}"#));

    assert!(out.get().contains("hunter2"));
}

#[test]
fn top_level_non_object_is_returned_unchanged() {
    let policy = RedactionPolicy::strict();
    let out = policy.redact_tool_args("noop", &raw("42"));
    let out_str = policy.redact_tool_args("noop", &raw("\"hello\""));

    assert_eq!(out.get().trim(), "42");
    assert_eq!(out_str.get().trim(), "\"hello\"");
}

#[test]
fn top_level_array_walks_into_each_element() {
    let policy = RedactionPolicy::strict();
    let out = policy.redact_tool_args("batch", &raw(r#"[{"password":"a"},{"password":"b"}]"#));

    assert!(!out.get().contains("\"a\""));
    assert!(!out.get().contains("\"b\""));
}

#[test]
fn replaced_value_does_not_recurse_into_placeholder() {
    let policy = RedactionPolicy::strict();
    let out = policy.redact_tool_args(
        "complex",
        &raw(r#"{"password":{"nested":{"password":"x"}}}"#),
    );

    assert!(!out.get().contains("nested"));
    assert!(!out.get().contains("\"x\""));
}

#[test]
fn response_text_can_be_stripped_or_truncated() {
    let mut policy = RedactionPolicy::strict();
    policy.strip_response_text = true;
    assert_eq!(policy.redact_response_text("secret"), REDACTED_PLACEHOLDER);

    policy.strip_response_text = false;
    policy.max_content_chars = 4;
    assert_eq!(policy.redact_response_text("abcdef"), "abcd... [truncated]");
}
