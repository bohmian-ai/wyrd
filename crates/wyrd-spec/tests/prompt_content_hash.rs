mod prompt_support;

use prompt_support::{
    json_schema_response_type, mutate_first_text, openai_chat_request, prompt, prompt_spec,
};
use wyrd_spec::PromptSpec;

#[test]
fn hash_stable_when_version_changes() {
    let mut prompt = prompt(openai_chat_request("hello"), Vec::new());
    let first = PromptSpec::new(prompt.clone())
        .expect("valid")
        .content_hash();
    prompt.version = Some("v3".to_owned());
    let second = PromptSpec::new(prompt).expect("valid").content_hash();

    assert_eq!(first, second);
}

#[test]
fn hash_changes_on_model_request_variables_and_response_type_mutations() {
    let base_prompt = prompt(openai_chat_request("hello {{name}}"), vec!["name"]);
    let base = PromptSpec::new(base_prompt.clone())
        .expect("valid")
        .content_hash();

    let mut changed_model = base_prompt.clone();
    changed_model.model = "other-model".to_owned();
    assert_ne!(
        base,
        PromptSpec::new(changed_model)
            .expect("valid")
            .content_hash()
    );

    let mut changed_request = base_prompt.clone();
    mutate_first_text(&mut changed_request.request, "hello {{name}} again");
    assert_ne!(
        base,
        PromptSpec::new(changed_request)
            .expect("valid")
            .content_hash()
    );

    let mut changed_vars = base_prompt.clone();
    mutate_first_text(&mut changed_vars.request, "hello {{name}} {{city}}");
    changed_vars.variables.push("city".to_owned());
    assert_ne!(
        base,
        PromptSpec::new(changed_vars).expect("valid").content_hash()
    );

    let mut changed_response = base_prompt;
    changed_response.response_type = json_schema_response_type();
    assert_ne!(
        base,
        PromptSpec::new(changed_response)
            .expect("valid")
            .content_hash()
    );
}

#[test]
fn hash_byte_identical_across_runs_and_has_expected_format() {
    let first = prompt_spec(openai_chat_request("hello"), Vec::new()).content_hash();
    let second = prompt_spec(openai_chat_request("hello"), Vec::new()).content_hash();

    assert_eq!(first, second);
    assert!(first.starts_with("sha256:"));
    assert_eq!(first.len(), "sha256:".len() + 64);
    assert!(
        first["sha256:".len()..]
            .chars()
            .all(|ch| ch.is_ascii_hexdigit())
    );
}
