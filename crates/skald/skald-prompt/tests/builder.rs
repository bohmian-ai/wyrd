use serde_json::json;
use skald_prompt::messages::{
    anthropic_image_base64_block, google_file_data_part, openai_file_id_part,
};
use skald_prompt::{
    AnthropicOptions, GeminiOptions, OpenAiChatOptions, OpenAiResponsesOptions, ResponseFormat,
    anthropic, gemini, openai_chat, openai_responses, raw, vertex,
};
use skald_spec::wire::anthropic_messages::{AnthropicContentBlock, AnthropicImageSource};
use skald_spec::wire::google_generate::GooglePart;
use skald_spec::wire::openai_chat::{OpenAiContentPart, OpenAiResponseFormat};
use skald_spec::wire::openai_responses::OpenAiTextResponseFormat;
use skald_spec::{ProviderName, ProviderRequest, ResponseType};

#[test]
fn openai_chat_emits_native_variant_and_response_format() {
    let schema = json!({
        "type": "object",
        "properties": {
            "answer": { "type": "string" }
        },
        "required": ["answer"]
    });
    let prompt = openai_chat(
        "gpt-4o",
        OpenAiChatOptions {
            system: Some("Answer exactly.".to_owned()),
            messages: vec!["Hello {{name}}".to_owned()],
            response_format: Some(ResponseFormat::json_schema("answer", schema.clone()).unwrap()),
            variables: vec!["name".to_owned()],
            version: Some("v1".to_owned()),
            ..OpenAiChatOptions::default()
        },
    )
    .unwrap();

    assert_eq!(
        prompt.native().response_type,
        ResponseType::JsonSchema {
            name: "answer".to_owned(),
            schema
        }
    );
    let ProviderRequest::OpenAiChatCompletion(request) = &prompt.native().request else {
        panic!("expected OpenAI chat request");
    };
    assert_eq!(request.messages.len(), 2);
    assert!(matches!(
        request.response_format,
        Some(OpenAiResponseFormat::JsonSchema { .. })
    ));
}

#[test]
fn openai_responses_emits_native_text_format() {
    let prompt = openai_responses(
        "gpt-4.1",
        OpenAiResponsesOptions {
            messages: vec!["Summarize this".to_owned()],
            response_format: Some(ResponseFormat::json_object()),
            ..OpenAiResponsesOptions::default()
        },
    )
    .unwrap();

    let ProviderRequest::OpenAiResponses(request) = &prompt.native().request else {
        panic!("expected OpenAI Responses request");
    };
    let format = request.text.as_ref().and_then(|text| text.format.as_ref());
    assert!(matches!(format, Some(OpenAiTextResponseFormat::JsonObject)));
}

#[test]
fn role_helpers_append_native_messages_without_neutral_storage() {
    let prompt = anthropic(
        "claude-sonnet-4",
        AnthropicOptions {
            messages: vec!["first".to_owned()],
            ..AnthropicOptions::default()
        },
    )
    .unwrap()
    .with_assistant("second")
    .unwrap()
    .with_tool_result("toolu_1", "done", false)
    .unwrap();

    let ProviderRequest::AnthropicMessage(request) = &prompt.native().request else {
        panic!("expected Anthropic request");
    };
    assert_eq!(request.messages.len(), 3);
    assert!(matches!(
        request.messages[2].content[0],
        AnthropicContentBlock::ToolResult { .. }
    ));
}

#[test]
fn google_and_vertex_emit_distinct_native_variants() {
    let gemini_prompt = gemini(
        "gemini-2.5-pro",
        GeminiOptions {
            messages: vec!["hello".to_owned()],
            ..GeminiOptions::default()
        },
    )
    .unwrap();
    assert!(matches!(
        gemini_prompt.native().request,
        ProviderRequest::GeminiGenerateContent(_)
    ));

    let vertex_prompt = vertex(
        "gemini-2.5-pro",
        GeminiOptions {
            messages: vec!["hello".to_owned()],
            ..GeminiOptions::default()
        },
    )
    .unwrap();
    assert!(matches!(
        vertex_prompt.native().request,
        ProviderRequest::Vertex(_)
    ));
}

#[test]
fn raw_preserves_provider_and_body_bytes() {
    let prompt = raw(
        ProviderName::Custom("local".to_owned()),
        "local-model",
        br#"{"a":1}"#,
    )
    .unwrap();

    let ProviderRequest::RawV1 { provider, body } = &prompt.native().request else {
        panic!("expected raw request");
    };
    assert_eq!(provider, &ProviderName::Custom("local".to_owned()));
    assert_eq!(body.get(), r#"{"a":1}"#);
}

#[test]
fn native_content_helpers_build_provider_wire_parts() {
    assert!(matches!(
        openai_file_id_part("file_123"),
        OpenAiContentPart::File { .. }
    ));
    assert!(matches!(
        anthropic_image_base64_block("image/png", "AAAA"),
        AnthropicContentBlock::Image {
            source: AnthropicImageSource::Base64 { .. },
            ..
        }
    ));
    assert!(matches!(
        google_file_data_part("application/pdf", "gs://bucket/file.pdf"),
        GooglePart::FileData { .. }
    ));
}
