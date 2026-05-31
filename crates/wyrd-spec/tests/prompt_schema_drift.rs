use schemars::schema_for;
use wyrd_spec::{ParameterName, PromptRef, PromptSpec};

#[test]
fn prompt_spec_schema_matches_golden() {
    assert_schema_matches::<PromptSpec>("prompt_spec");
}

#[test]
fn prompt_ref_schema_matches_golden() {
    assert_schema_matches::<PromptRef>("prompt_ref");
}

#[test]
fn parameter_name_schema_matches_golden() {
    assert_schema_matches::<ParameterName>("parameter_name");
}

fn assert_schema_matches<T: schemars::JsonSchema>(name: &str) {
    let mut schema = schema_for!(T);
    schema.meta_schema = Some("https://json-schema.org/draft/2020-12/schema".to_owned());
    let actual = format!(
        "{}\n",
        serde_json::to_string_pretty(&schema).expect("schema serializes")
    );
    let expected = std::fs::read_to_string(format!("tests/schemas/{name}.json"))
        .expect("golden schema exists");
    assert_eq!(actual, expected, "schema drift: run mise run codegen:regen");
}
