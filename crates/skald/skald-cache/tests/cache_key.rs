use serde_json::json;
use skald_cache::{CacheKey, SkaldCacheError};
use skald_spec::wire::anthropic_messages::AnthropicMessagesRequest;
use skald_spec::wire::openai_chat::OpenAiChatRequest;
use skald_spec::{ProviderName, ProviderRequest};

fn openai_chat(prompt_cache_key: &str) -> ProviderRequest {
    ProviderRequest::OpenAiChatCompletion(
        serde_json::from_value::<OpenAiChatRequest>(json!({
            "model": "gpt-4o-mini",
            "messages": [{ "role": "user", "content": "cache this" }],
            "prompt_cache_key": prompt_cache_key
        }))
        .expect("valid OpenAI chat request"),
    )
}

fn anthropic_message(cache_control: bool) -> ProviderRequest {
    let content = if cache_control {
        json!([{
            "type": "text",
            "text": "cache this",
            "cache_control": { "type": "ephemeral" }
        }])
    } else {
        json!([{ "type": "text", "text": "do not cache" }])
    };

    ProviderRequest::AnthropicMessage(
        serde_json::from_value::<AnthropicMessagesRequest>(json!({
            "model": "claude-sonnet-4-5",
            "max_tokens": 256,
            "messages": [{ "role": "user", "content": content }]
        }))
        .expect("valid Anthropic messages request"),
    )
}

#[test]
fn key_includes_content_hash_provider_scope() {
    let key = CacheKey::from_request(&openai_chat("tenant-a:prompt-1"))
        .expect("cache key derivation succeeds")
        .expect("prompt_cache_key creates a key");

    assert_eq!(key.provider, ProviderName::OpenAi);
    assert_eq!(key.model, "gpt-4o-mini");
    assert_eq!(key.request_prefix_hash.len(), 64);
}

#[test]
fn key_equality_matches() {
    let left = CacheKey::from_request(&anthropic_message(true))
        .expect("cache key derivation succeeds")
        .expect("cache_control creates a key");
    let right = CacheKey::from_request(&anthropic_message(true))
        .expect("cache key derivation succeeds")
        .expect("cache_control creates a key");

    assert_eq!(left, right);
}

#[test]
fn key_hash_distinct_for_different_scopes() {
    let openai = CacheKey::from_request(&openai_chat("shared-prefix"))
        .expect("cache key derivation succeeds")
        .expect("prompt_cache_key creates a key");
    let anthropic = CacheKey::from_request(&anthropic_message(true))
        .expect("cache key derivation succeeds")
        .expect("cache_control creates a key");

    assert_ne!(openai.provider, anthropic.provider);
    assert_ne!(openai, anthropic);
}

#[test]
fn anthropic_without_cache_control_returns_none() {
    let key =
        CacheKey::from_request(&anthropic_message(false)).expect("cache key derivation succeeds");

    assert!(key.is_none());
}

#[test]
fn raw_request_returns_opaque_request_error() {
    let body = serde_json::value::RawValue::from_string(r#"{"model":"x"}"#.to_owned())
        .expect("raw JSON is valid");
    let request = ProviderRequest::RawV1 {
        provider: ProviderName::Custom("local".to_owned()),
        body,
    };

    let error = CacheKey::from_request(&request).expect_err("raw request is opaque");

    assert_eq!(error, SkaldCacheError::OpaqueRequest);
    assert_eq!(error.code(), "SKALD_CACHE_400_OPAQUE_REQUEST");
}
