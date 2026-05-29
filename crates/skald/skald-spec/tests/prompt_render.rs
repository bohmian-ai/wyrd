mod common;

use serde_json::{Value, json};
use skald_spec::wire::anthropic_messages::AnthropicContentBlock;
use skald_spec::{Prompt, ProviderRequest, ResponseType, SkaldError};

fn prompt(request: ProviderRequest, variables: Vec<&str>) -> Prompt {
    Prompt {
        request,
        model: "model".to_string(),
        version: None,
        variables: variables.into_iter().map(str::to_string).collect(),
        response_type: ResponseType::Text,
    }
}

#[test]
fn render_substitutes_anthropic_variables() {
    let request = ProviderRequest::AnthropicMessage(common::anthropic_request());
    let rendered = prompt(request, vec!["topic", "name"])
        .render(&[("topic", "rules"), ("name", "world")])
        .unwrap();

    let ProviderRequest::AnthropicMessage(rendered) = rendered else {
        panic!("render should preserve provider variant");
    };
    let AnthropicContentBlock::Text { text, .. } = &rendered.messages[0].content[0] else {
        panic!("expected text block");
    };
    assert_eq!(text, "Hello world");
}

#[test]
fn render_repeated_reference_substitutes_all_occurrences() {
    let mut request = common::openai_chat_request();
    request.messages[0].content = Some(skald_spec::wire::openai_chat::OpenAiMessageContent::Text(
        "{{name}} {{name}} {{name}}".to_string(),
    ));
    let rendered = prompt(ProviderRequest::OpenAiChatCompletion(request), vec!["name"])
        .render(&[("name", "Ada")])
        .unwrap();
    let json = serde_json::to_string(&rendered).unwrap();
    assert!(json.contains("Ada Ada Ada"));
}

#[test]
fn render_no_variables_returns_request_unchanged() {
    let request = ProviderRequest::GeminiGenerateContent(common::google_request());
    let rendered = prompt(request.clone(), Vec::new()).render(&[]).unwrap();
    assert_eq!(rendered, request);
}

#[test]
fn render_missing_variable_value_returns_skald_error() {
    let err = prompt(
        ProviderRequest::AnthropicMessage(common::anthropic_request()),
        vec!["name"],
    )
    .render(&[])
    .unwrap_err();
    assert_eq!(err, SkaldError::MissingVariable("name".to_string()));
    assert_eq!(err.code(), "SKALD_SPEC_422_MISSING_VARIABLE");
}

#[test]
fn bind_returns_new_prompt_and_tracks_remaining_variables() {
    let request = ProviderRequest::AnthropicMessage(common::anthropic_request());
    let original = prompt(request, vec!["name", "topic"]);
    let bound = original.bind(&[("name", "Ada")]).unwrap();

    assert_eq!(original.variables, vec!["name", "topic"]);
    assert_eq!(bound.variables, vec!["topic"]);
    let ProviderRequest::AnthropicMessage(request) = &bound.request else {
        panic!("bind should preserve provider variant");
    };
    let AnthropicContentBlock::Text { text, .. } = &request.messages[0].content[0] else {
        panic!("expected text block");
    };
    assert_eq!(text, "Hello Ada");
}

#[test]
fn bind_mut_supports_incremental_parameter_injection() {
    let request = ProviderRequest::OpenAiChatCompletion(common::openai_chat_request());
    let mut prompt = prompt(request, vec!["name"]);
    prompt.bind_mut(&[("name", "Grace")]).unwrap();

    assert!(prompt.variables.is_empty());
    let rendered = prompt.render(&[]).unwrap();
    assert!(serde_json::to_string(&rendered).unwrap().contains("Grace"));
}

#[test]
fn render_escapes_json_special_characters_and_blocks_injection() {
    let request = ProviderRequest::OpenAiChatCompletion(common::openai_chat_request());
    let rendered = prompt(request, vec!["name"])
        .render(&[("name", "\"quoted\", \"new_field\": \"x {{still_text}}")])
        .unwrap();
    let value = serde_json::to_value(&rendered).unwrap();
    assert_eq!(value.get("new_field"), None);
    assert!(
        serde_json::to_string(&rendered)
            .unwrap()
            .contains("\\\"quoted\\\", \\\"new_field\\\": \\\"x {{still_text}}")
    );
}

#[test]
fn dollar_syntax_is_not_matched() {
    let mut request = common::google_request();
    if let skald_spec::wire::google_generate::GooglePart::Text { text } =
        &mut request.contents[0].parts[0]
    {
        *text = "Hello ${name}".to_string();
    }
    let rendered = prompt(
        ProviderRequest::GeminiGenerateContent(request),
        vec!["name"],
    )
    .render(&[("name", "Ada")])
    .unwrap();
    let value: Value = serde_json::to_value(rendered).unwrap();
    assert!(value.to_string().contains("${name}"));
}

#[test]
fn render_same_type_in_same_type_out_for_all_live_requests() {
    assert!(matches!(
        prompt(
            ProviderRequest::OpenAiChatCompletion(common::openai_chat_request()),
            vec!["name"]
        )
        .render(&[("name", "Ada")])
        .unwrap(),
        ProviderRequest::OpenAiChatCompletion(_)
    ));
    assert!(matches!(
        prompt(
            ProviderRequest::OpenAiResponses(common::openai_responses_request()),
            vec!["name"]
        )
        .render(&[("name", "Ada")])
        .unwrap(),
        ProviderRequest::OpenAiResponses(_)
    ));
    assert!(matches!(
        prompt(
            ProviderRequest::GeminiGenerateContent(common::google_request()),
            vec!["name"]
        )
        .render(&[("name", "Ada")])
        .unwrap(),
        ProviderRequest::GeminiGenerateContent(_)
    ));
}

#[test]
fn response_type_json_schema_roundtrips() {
    let response_type = ResponseType::JsonSchema {
        name: "answer".to_string(),
        schema: json!({"type": "object"}),
    };
    let json = serde_json::to_string(&response_type).unwrap();
    assert_eq!(
        serde_json::from_str::<ResponseType>(&json).unwrap(),
        response_type
    );
}
