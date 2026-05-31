mod common;

use serde_json::json;
use skald_spec::wire::anthropic_messages::{AnthropicContentBlock, AnthropicDocumentSource};
use skald_spec::wire::google_generate::GooglePart;
use skald_spec::wire::openai_chat::{OpenAiContentPart, OpenAiMessageContent};
use skald_spec::{
    MediaKind, MediaRef, Prompt, ProviderName, ProviderRequest, ResponseType, SkaldError,
};

fn prompt(request: ProviderRequest) -> Prompt {
    Prompt::new(request, "model", None, ResponseType::Text).expect("prompt is valid")
}

fn openai_prompt(text: &str) -> Prompt {
    let mut request = common::openai_chat_request();
    request.messages.truncate(2);
    request.messages[0].content = Some(OpenAiMessageContent::Text("system".to_owned()));
    request.messages[1].content = Some(OpenAiMessageContent::Text(text.to_owned()));
    prompt(ProviderRequest::OpenAiChatCompletion(request))
}

fn anthropic_prompt(text: &str) -> Prompt {
    let mut request = common::anthropic_request();
    request.system = None;
    request.messages[0].content = vec![AnthropicContentBlock::Text {
        text: text.to_owned(),
        cache_control: None,
        citations: None,
    }];
    prompt(ProviderRequest::AnthropicMessage(request))
}

fn google_prompt(text: &str) -> Prompt {
    let mut request = common::google_request();
    request.contents[0].parts = vec![GooglePart::Text {
        text: text.to_owned(),
    }];
    prompt(ProviderRequest::GeminiGenerateContent(request))
}

#[test]
fn media_variables_populated_and_split_at_construction() {
    let prompt = openai_prompt("before ${media:logo} after ${media:logo}");
    assert_eq!(prompt.media_variables, vec!["logo"]);

    let ProviderRequest::OpenAiChatCompletion(request) = prompt.request else {
        panic!("expected OpenAI request");
    };
    let Some(OpenAiMessageContent::Parts(parts)) = &request.messages[1].content else {
        panic!("media placeholder should force native parts");
    };
    assert!(matches!(parts[0], OpenAiContentPart::Text { ref text } if text == "before "));
    assert!(matches!(parts[1], OpenAiContentPart::Text { ref text } if text == "${media:logo}"));
    assert!(matches!(parts[2], OpenAiContentPart::Text { ref text } if text == " after "));
    assert!(matches!(parts[3], OpenAiContentPart::Text { ref text } if text == "${media:logo}"));
}

#[test]
fn media_placeholder_in_system_message_is_rejected() {
    let mut request = common::openai_chat_request();
    request.messages[0].content = Some(OpenAiMessageContent::Text("system ${media:x}".to_owned()));
    let err = Prompt::new(
        ProviderRequest::OpenAiChatCompletion(request),
        "model",
        None,
        ResponseType::Text,
    )
    .unwrap_err();
    assert_eq!(
        err,
        SkaldError::MediaInSystemMessage {
            name: "x".to_owned()
        }
    );
    assert_eq!(err.code(), "SKALD_SPEC_400_MEDIA_IN_SYSTEM_MESSAGE");
}

#[test]
fn bind_media_replaces_all_openai_placeholders_and_preserves_text_variables() {
    let bound = openai_prompt("Hello {{name}} ${media:logo} ${media:logo}")
        .bind_media(
            "logo",
            &MediaRef::image_url("https://example.com/logo.png", None),
        )
        .unwrap();
    assert_eq!(bound.variables, vec!["name"]);
    assert!(bound.media_variables.is_empty());
    let ProviderRequest::OpenAiChatCompletion(request) = bound.request else {
        panic!("expected OpenAI request");
    };
    let Some(OpenAiMessageContent::Parts(parts)) = &request.messages[1].content else {
        panic!("expected parts");
    };
    assert!(matches!(parts[1], OpenAiContentPart::ImageUrl { .. }));
    assert!(matches!(parts[3], OpenAiContentPart::ImageUrl { .. }));
}

#[test]
fn render_fails_when_media_variable_is_unbound() {
    let err = openai_prompt("${media:logo}").render(&[]).unwrap_err();
    assert_eq!(
        err,
        SkaldError::MissingMediaVariable {
            name: "logo".to_owned()
        }
    );
    assert_eq!(err.code(), "SKALD_SPEC_422_MISSING_MEDIA_VARIABLE");
}

#[test]
fn missing_media_placeholder_returns_stable_error() {
    let err = openai_prompt("plain")
        .bind_media(
            "logo",
            &MediaRef::image_url("https://example.com/logo.png", None),
        )
        .unwrap_err();
    assert_eq!(
        err,
        SkaldError::MediaPlaceholderNotFound {
            name: "logo".to_owned()
        }
    );
    assert_eq!(err.code(), "SKALD_SPEC_422_MEDIA_PLACEHOLDER_NOT_FOUND");
}

#[test]
fn openai_media_matrix() {
    let image = openai_prompt("${media:image}")
        .bind_media("image", &MediaRef::image_base64("image/png", "AAAA"))
        .unwrap();
    let ProviderRequest::OpenAiChatCompletion(image) = image.request else {
        panic!("expected OpenAI request");
    };
    let Some(OpenAiMessageContent::Parts(parts)) = &image.messages[1].content else {
        panic!("expected parts");
    };
    assert!(matches!(parts[0], OpenAiContentPart::ImageUrl { .. }));

    let document = openai_prompt("${media:doc}")
        .bind_media("doc", &MediaRef::document_base64("application/pdf", "BBBB"))
        .unwrap();
    let ProviderRequest::OpenAiChatCompletion(document) = document.request else {
        panic!("expected OpenAI request");
    };
    let Some(OpenAiMessageContent::Parts(parts)) = &document.messages[1].content else {
        panic!("expected parts");
    };
    assert!(matches!(parts[0], OpenAiContentPart::File { .. }));

    let err = openai_prompt("${media:doc}")
        .bind_media(
            "doc",
            &MediaRef::document_url("https://example.com/doc.pdf", None),
        )
        .unwrap_err();
    assert_eq!(
        err,
        SkaldError::UnsupportedMediaForProvider {
            provider: ProviderName::OpenAi,
            kind: MediaKind::Document,
        }
    );
}

#[test]
fn anthropic_accepts_image_and_document_url_and_base64() {
    let image = anthropic_prompt("${media:image}")
        .bind_media(
            "image",
            &MediaRef::image_url("https://example.com/image.png", None),
        )
        .unwrap();
    let ProviderRequest::AnthropicMessage(image) = image.request else {
        panic!("expected Anthropic request");
    };
    assert!(matches!(
        image.messages[0].content[0],
        AnthropicContentBlock::Image { .. }
    ));

    let document = anthropic_prompt("${media:doc}")
        .bind_media("doc", &MediaRef::document_base64("application/pdf", "AAAA"))
        .unwrap();
    let ProviderRequest::AnthropicMessage(document) = document.request else {
        panic!("expected Anthropic request");
    };
    assert!(matches!(
        document.messages[0].content[0],
        AnthropicContentBlock::Document {
            source: AnthropicDocumentSource::Base64 { .. },
            ..
        }
    ));
}

#[test]
fn google_media_matrix_and_rejections() {
    let inline = google_prompt("${media:image}")
        .bind_media("image", &MediaRef::image_base64("image/png", "AAAA"))
        .unwrap();
    let ProviderRequest::GeminiGenerateContent(inline) = inline.request else {
        panic!("expected Google request");
    };
    assert!(matches!(
        inline.contents[0].parts[0],
        GooglePart::InlineData { .. }
    ));

    let file = google_prompt("${media:image}")
        .bind_media(
            "image",
            &MediaRef::image_url("gs://bucket/image.png", Some("image/png".to_owned())),
        )
        .unwrap();
    let ProviderRequest::GeminiGenerateContent(file) = file.request else {
        panic!("expected Google request");
    };
    assert!(matches!(
        file.contents[0].parts[0],
        GooglePart::FileData { .. }
    ));

    let https = google_prompt("${media:image}")
        .bind_media(
            "image",
            &MediaRef::image_url(
                "https://example.com/image.png",
                Some("image/png".to_owned()),
            ),
        )
        .unwrap_err();
    assert_eq!(
        https,
        SkaldError::UnsupportedMediaForProvider {
            provider: ProviderName::Google,
            kind: MediaKind::Image,
        }
    );

    let missing_mime = google_prompt("${media:image}")
        .bind_media("image", &MediaRef::image_file("files/abc", None))
        .unwrap_err();
    assert!(matches!(missing_mime, SkaldError::InvalidMediaType(_)));
}

#[test]
fn text_binding_does_not_touch_media_and_media_binding_does_not_touch_text() {
    let after_text = openai_prompt("Hi {{name}} ${media:logo}")
        .bind(&[("name", "Ada")])
        .unwrap();
    assert!(
        serde_json::to_value(&after_text.request)
            .unwrap()
            .to_string()
            .contains("${media:logo}")
    );

    let after_media = after_text
        .bind_media(
            "logo",
            &MediaRef::image_url("https://example.com/logo.png", None),
        )
        .unwrap();
    assert!(
        serde_json::to_value(&after_media.request)
            .unwrap()
            .to_string()
            .contains("Ada")
    );
}

#[test]
fn serde_default_accepts_prompts_without_media_variables() {
    let value = json!({
        "request": common::openai_chat_request(),
        "model": "model",
        "variables": ["name"],
        "response_type": "text"
    });
    let prompt: Prompt = serde_json::from_value(value).unwrap();
    assert!(prompt.media_variables.is_empty());
}
