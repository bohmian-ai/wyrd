use serde_json::{Value, json};
use skald_spec::{
    AnthropicMessagesRequest, AnthropicMessagesSettings, GoogleGenerateContentRequest,
    OpenAiChatRequest, OpenAiResponsesRequest,
};

#[test]
fn openai_settings_flatten_to_top_level_json() {
    let request: OpenAiChatRequest = serde_json::from_value(json!({
        "model": "gpt-4o",
        "messages": [{"role": "user", "content": "hello"}],
        "temperature": 0.25,
        "seed": 42,
        "logit_bias": {"123": -100}
    }))
    .unwrap();

    assert_eq!(request.settings.seed, Some(42));
    let value = serde_json::to_value(request).unwrap();
    assert_eq!(value["temperature"], json!(0.25));
    assert_eq!(value["seed"], json!(42));
    assert_eq!(value["logit_bias"], json!({"123": -100}));
    assert!(value.get("settings").is_none());
}

#[test]
fn openai_unmodeled_field_passthrough() {
    let request: OpenAiChatRequest = serde_json::from_value(json!({
        "model": "gpt-4o",
        "messages": [{"role": "user", "content": "hello"}],
        "future_knob": {"enabled": true}
    }))
    .unwrap();

    assert_eq!(
        request.settings.extra.get("future_knob"),
        Some(&json!({"enabled": true}))
    );
    assert_eq!(
        serde_json::to_value(request).unwrap()["future_knob"],
        json!({"enabled": true})
    );
}

#[test]
fn anthropic_default_max_tokens_is_wire_default() {
    assert_eq!(AnthropicMessagesSettings::default().max_tokens, 4096);

    let request = AnthropicMessagesRequest {
        model: "claude-sonnet-4-5".to_owned(),
        messages: Vec::new(),
        system: None,
        stream: None,
        tools: None,
        tool_choice: None,
        output_config: None,
        settings: AnthropicMessagesSettings::default(),
    };
    assert_eq!(serde_json::to_value(request).unwrap()["max_tokens"], 4096);
}

#[test]
fn google_generation_config_stays_nested() {
    let request: GoogleGenerateContentRequest = serde_json::from_value(json!({
        "contents": [{"role": "user", "parts": [{"text": "hello"}]}],
        "generation_config": {
            "temperature": 0.125,
            "thinking_config": {"include_thoughts": true},
            "future_generation_knob": "native"
        }
    }))
    .unwrap();

    let config = request.settings.generation_config.as_ref().unwrap();
    assert_eq!(config.temperature, Some(0.125));
    assert_eq!(
        config.extra.get("future_generation_knob"),
        Some(&Value::String("native".to_owned()))
    );
    let value = serde_json::to_value(request).unwrap();
    assert_eq!(value["generation_config"]["temperature"], json!(0.125));
    assert!(value.get("temperature").is_none());
}

#[test]
fn google_unknown_top_level_field_is_absorbed() {
    let request: GoogleGenerateContentRequest = serde_json::from_value(json!({
        "contents": [{"role": "user", "parts": [{"text": "hello"}]}],
        "native_future": 1
    }))
    .unwrap();

    assert_eq!(request.settings.extra.get("native_future"), Some(&json!(1)));
    assert_eq!(
        serde_json::to_value(request).unwrap()["native_future"],
        json!(1)
    );
}

#[test]
fn settings_requests_round_trip_each_provider() {
    let openai_chat: OpenAiChatRequest = serde_json::from_value(json!({
        "model": "gpt-4o",
        "messages": [{"role": "user", "content": "hello"}],
        "temperature": 0.25,
        "foo": "bar"
    }))
    .unwrap();
    round_trip(openai_chat);

    let openai_responses: OpenAiResponsesRequest = serde_json::from_value(json!({
        "model": "gpt-4o",
        "input": [{"type": "message", "role": "user", "content": [{"type": "input_text", "text": "hello"}]}],
        "max_output_tokens": 64,
        "foo": "bar"
    }))
    .unwrap();
    round_trip(openai_responses);

    let anthropic: AnthropicMessagesRequest = serde_json::from_value(json!({
        "model": "claude-sonnet-4-5",
        "messages": [{"role": "user", "content": [{"type": "text", "text": "hello"}]}],
        "max_tokens": 512,
        "foo": "bar"
    }))
    .unwrap();
    round_trip(anthropic);

    let google: GoogleGenerateContentRequest = serde_json::from_value(json!({
        "contents": [{"role": "user", "parts": [{"text": "hello"}]}],
        "cached_content": "cachedContents/abc",
        "foo": "bar"
    }))
    .unwrap();
    round_trip(google);
}

fn round_trip<T>(value: T)
where
    T: serde::Serialize + serde::de::DeserializeOwned + PartialEq + std::fmt::Debug,
{
    let json = serde_json::to_value(&value).unwrap();
    let reparsed: T = serde_json::from_value(json).unwrap();
    assert_eq!(reparsed, value);
}
