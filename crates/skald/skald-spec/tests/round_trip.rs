mod common;

use serde::{Serialize, de::DeserializeOwned};
use serde_json::{json, value::RawValue};
use skald_spec::wire::anthropic_messages::AnthropicStopReason;
use skald_spec::{MessageNum, ProviderName, ProviderRequest, ProviderResponse};

fn round_trip<T>(value: &T)
where
    T: Serialize + DeserializeOwned + PartialEq + std::fmt::Debug,
{
    let json = serde_json::to_string(value).unwrap();
    let reparsed: T = serde_json::from_str(&json).unwrap();
    assert_eq!(&reparsed, value);
}

#[test]
fn provider_requests_roundtrip() {
    for request in common::provider_request_variants() {
        round_trip(&request);
    }
}

#[test]
fn provider_responses_roundtrip() {
    for response in common::provider_response_variants() {
        round_trip(&response);
    }
}

#[test]
fn messages_roundtrip() {
    for message in common::message_variants() {
        round_trip(&message);
    }
}

#[test]
fn per_provider_wire_roundtrips() {
    round_trip(&common::openai_chat_request());
    round_trip(&common::openai_chat_response());
    round_trip(&common::openai_responses_request());
    round_trip(&common::openai_responses_response());
    round_trip(&common::anthropic_request());
    for reason in [
        AnthropicStopReason::EndTurn,
        AnthropicStopReason::MaxTokens,
        AnthropicStopReason::StopSequence,
        AnthropicStopReason::ToolUse,
        AnthropicStopReason::PauseTurn,
        AnthropicStopReason::Refusal,
    ] {
        round_trip(&common::anthropic_response(reason));
    }
    round_trip(&common::google_request());
    round_trip(&common::google_response(
        skald_spec::wire::google_generate::GoogleFinishReason::Stop,
    ));
    round_trip(&common::vertex_generate_request());
    round_trip(&common::vertex_predict_request());
    round_trip(&common::vertex_predict_response());
}

#[test]
fn raw_v1_request_roundtrip_preserves_provider_and_body() {
    for provider in [
        ProviderName::OpenAi,
        ProviderName::Anthropic,
        ProviderName::Google,
        ProviderName::Vertex,
        ProviderName::Custom("acme".to_string()),
    ] {
        let request = common::raw_request(provider.clone());
        let json = serde_json::to_string(&request).unwrap();
        let reparsed: ProviderRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(reparsed, request);
        assert_eq!(reparsed.provider(), provider);
    }
}

#[test]
fn raw_v1_response_and_message_roundtrip_preserve_body() {
    let response =
        ProviderResponse::RawV1(RawValue::from_string(json!({"x": 1}).to_string()).unwrap());
    round_trip(&response);

    let message = MessageNum::RawV1(RawValue::from_string(json!({"y": 2}).to_string()).unwrap());
    round_trip(&message);
}
