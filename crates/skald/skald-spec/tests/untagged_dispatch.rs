mod common;

use serde_json::json;
use skald_spec::{MessageNum, ProviderRequest, ProviderResponse};

#[test]
fn provider_request_untagged_dispatch_per_provider() {
    assert!(matches!(
        serde_json::from_value::<ProviderRequest>(
            serde_json::to_value(common::openai_chat_request()).unwrap()
        )
        .unwrap(),
        ProviderRequest::OpenAiChatCompletion(_)
    ));
    assert!(matches!(
        serde_json::from_value::<ProviderRequest>(
            serde_json::to_value(common::anthropic_request()).unwrap()
        )
        .unwrap(),
        ProviderRequest::AnthropicMessage(_)
    ));
    assert!(matches!(
        serde_json::from_value::<ProviderRequest>(
            serde_json::to_value(common::google_request()).unwrap()
        )
        .unwrap(),
        ProviderRequest::GeminiGenerateContent(_)
    ));
}

#[test]
fn vertex_generate_body_is_transparent_google_shape() {
    assert!(matches!(
        serde_json::from_value::<ProviderRequest>(
            serde_json::to_value(common::vertex_generate_request()).unwrap()
        )
        .unwrap(),
        ProviderRequest::GeminiGenerateContent(_)
    ));
}

#[test]
fn raw_v1_is_last_fallback_for_wrapped_unknown_request() {
    let body = json!({"provider": "open_ai", "body": {"untyped": true}});
    assert!(matches!(
        serde_json::from_value::<ProviderRequest>(body).unwrap(),
        ProviderRequest::RawV1 { .. }
    ));
}

#[test]
fn deny_unknown_fields_blocks_cross_variant_confusion() {
    let body = json!({
        "provider": "open_ai",
        "body": {
            "model": "gpt-4o",
            "messages": [{"role": "user", "content": "hi"}],
            "unknown": true
        }
    });
    assert!(matches!(
        serde_json::from_value::<ProviderRequest>(body).unwrap(),
        ProviderRequest::RawV1 { .. }
    ));
}

#[test]
fn message_num_untagged_dispatch_per_provider() {
    assert!(matches!(
        serde_json::from_value::<MessageNum>(
            serde_json::to_value(common::openai_message()).unwrap()
        )
        .unwrap(),
        MessageNum::OpenAi(_)
    ));
    assert!(matches!(
        serde_json::from_value::<MessageNum>(
            serde_json::to_value(common::anthropic_message()).unwrap()
        )
        .unwrap(),
        MessageNum::Anthropic(_)
    ));
    assert!(matches!(
        serde_json::from_value::<MessageNum>(
            serde_json::to_value(common::google_message()).unwrap()
        )
        .unwrap(),
        MessageNum::Gemini(_)
    ));
}

#[test]
fn provider_response_untagged_dispatch_per_provider() {
    assert!(matches!(
        serde_json::from_value::<ProviderResponse>(
            serde_json::to_value(common::openai_chat_response()).unwrap()
        )
        .unwrap(),
        ProviderResponse::OpenAiChatCompletion(_)
    ));
    assert!(matches!(
        serde_json::from_value::<ProviderResponse>(
            serde_json::to_value(common::anthropic_response(
                skald_spec::wire::anthropic_messages::AnthropicStopReason::EndTurn
            ))
            .unwrap()
        )
        .unwrap(),
        ProviderResponse::AnthropicMessage(_)
    ));
    assert!(matches!(
        serde_json::from_value::<ProviderResponse>(
            serde_json::to_value(common::google_response(
                skald_spec::wire::google_generate::GoogleFinishReason::Stop
            ))
            .unwrap()
        )
        .unwrap(),
        ProviderResponse::GeminiGenerateContent(_)
    ));
}
