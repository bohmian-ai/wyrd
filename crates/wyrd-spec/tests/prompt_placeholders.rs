mod prompt_support;

use prompt_support::{
    anthropic_with_system, anthropic_with_tool_values, openai_chat_request, prompt_spec,
};
use wyrd_spec::extract_text_placeholders;

#[test]
fn extracts_placeholder_in_anthropic_system_text() {
    let spec = prompt_spec(anthropic_with_system("system {{topic}}"), vec!["topic"]);

    assert_eq!(
        extract_text_placeholders(&spec).expect("extracts"),
        vec!["topic"]
    );
}

#[test]
fn extracts_placeholder_in_user_message_text() {
    let spec = prompt_spec(openai_chat_request("hello {{name}}"), vec!["name"]);

    assert_eq!(
        extract_text_placeholders(&spec).expect("extracts"),
        vec!["name"]
    );
}

#[test]
fn extracts_placeholder_in_tool_input_and_result_strings() {
    let spec = prompt_spec(anthropic_with_tool_values(), vec!["city"]);

    assert_eq!(
        extract_text_placeholders(&spec).expect("extracts"),
        vec!["city"]
    );
}

#[test]
fn extracts_first_seen_order_stable_and_deduplicated() {
    let spec = prompt_spec(
        openai_chat_request("{{first}} {{second}} {{first}}"),
        vec!["first", "second"],
    );

    assert_eq!(
        extract_text_placeholders(&spec).expect("extracts"),
        vec!["first", "second"]
    );
    assert_eq!(
        extract_text_placeholders(&spec).expect("extracts again"),
        vec!["first", "second"]
    );
}
