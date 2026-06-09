use serde_json::json;
use skald_spec::MessageNum;
use skald_spec::wire::anthropic_messages::{AnthropicContentBlock, AnthropicMessage};
use skald_spec::wire::google_generate::GoogleContent;
use skald_spec::wire::openai_chat::OpenAiChatMessage;
use skald_tool::{
    anthropic_tool_result_block, anthropic_tool_use_block, google_function_call_part,
    google_function_response_part, openai_function_tool_call, openai_tool_result_message,
};

#[test]
fn openai_tool_call_and_result_round_trip_through_message_num() {
    let call = openai_function_tool_call("call_1", "lookup_weather", json!({ "city": "Austin" }));
    let assistant = OpenAiChatMessage {
        role: "assistant".to_owned(),
        content: None,
        name: None,
        tool_calls: Some(vec![call.clone()]),
        tool_call_id: None,
        refusal: None,
        annotations: Vec::new(),
        audio: None,
    };
    let result = openai_tool_result_message("call_1", "{\"forecast\":\"sunny\"}");

    assert_eq!(call.function.name, "lookup_weather");
    assert_round_trips(MessageNum::OpenAi(Box::new(assistant)));
    assert_round_trips(MessageNum::OpenAi(Box::new(result)));
}

#[test]
fn anthropic_tool_use_and_result_round_trip_through_message_num() {
    let use_block =
        anthropic_tool_use_block("toolu_1", "lookup_weather", json!({ "city": "Austin" }));
    let result_block = anthropic_tool_result_block("toolu_1", "sunny", Some(false));
    let message = AnthropicMessage {
        role: "user".to_owned(),
        content: vec![use_block, result_block],
    };

    assert!(matches!(
        &message.content[0],
        AnthropicContentBlock::ToolUse { name, .. } if name == "lookup_weather"
    ));
    assert_round_trips(MessageNum::Anthropic(message));
}

#[test]
fn google_function_call_and_response_round_trip_through_message_num() {
    let content = GoogleContent {
        role: "model".to_owned(),
        parts: vec![
            google_function_call_part("lookup_weather", json!({ "city": "Austin" })),
            google_function_response_part("lookup_weather", json!({ "forecast": "sunny" })),
        ],
    };

    assert_round_trips(MessageNum::Gemini(content));
}

fn assert_round_trips(message: MessageNum) {
    let value = serde_json::to_value(&message).expect("message serializes");
    let decoded: MessageNum = serde_json::from_value(value).expect("message deserializes");

    assert_eq!(decoded, message);
}
