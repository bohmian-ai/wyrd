use serde_json::json;
use skald_spec::TokenUsage;
use skald_spec::wire::openai_chat::{
    OpenAiAudioFormat, OpenAiChatRequest, OpenAiChatToolChoice, OpenAiPredictionPayload,
    OpenAiResponseFormat, OpenAiTool, OpenAiUsage, OpenAiVoice,
};
use skald_spec::wire::openai_responses::{
    OpenAiResponsesRequest, OpenAiResponsesResponse, OpenAiResponsesStreamEvent,
    OpenAiResponsesToolChoice, OpenAiTextResponseFormat,
};

#[test]
fn chat_request_uses_typed_openai_fields() {
    let body = json!({
        "model": "gpt-4o-audio-preview",
        "messages": [
            {"role": "user", "content": "hello"}
        ],
        "audio": {"voice": "alloy", "format": "mp3"},
        "prediction": {
            "type": "content",
            "content": [{"type": "text", "text": "known output"}]
        },
        "tool_choice": {"type": "function", "function": {"name": "answer"}},
        "tools": [
            {
                "type": "function",
                "function": {
                    "name": "answer",
                    "description": "answer the prompt",
                    "parameters": {"type": "object"},
                    "strict": true
                }
            },
            {
                "type": "custom",
                "custom": {
                    "name": "freeform",
                    "format": {
                        "type": "grammar",
                        "grammar": {"definition": "start: WORD", "syntax": "lark"}
                    }
                }
            }
        ],
        "metadata": {"purpose": "review"},
        "modalities": ["text", "audio"],
        "stream_options": {"include_usage": true, "include_obfuscation": false},
        "logit_bias": {"42": -1},
        "response_format": {
            "type": "json_schema",
            "json_schema": {
                "name": "answer",
                "schema": {"type": "object"},
                "strict": true
            }
        }
    });

    let request: OpenAiChatRequest = serde_json::from_value(body.clone()).unwrap();

    assert!(matches!(
        request.settings.audio.as_ref().unwrap().voice,
        OpenAiVoice::BuiltIn(_)
    ));
    assert_eq!(
        request.settings.audio.as_ref().unwrap().format,
        OpenAiAudioFormat::Mp3
    );
    assert!(matches!(
        request.settings.prediction.as_ref().unwrap().content,
        OpenAiPredictionPayload::Parts(_)
    ));
    assert!(matches!(
        request.tool_choice.as_ref().unwrap(),
        OpenAiChatToolChoice::Function(_)
    ));
    assert!(matches!(
        request.tools.as_ref().unwrap().first().unwrap(),
        OpenAiTool::Function { .. }
    ));
    assert_eq!(
        request
            .settings
            .metadata
            .as_ref()
            .unwrap()
            .get("purpose")
            .unwrap(),
        "review"
    );
    assert!(matches!(
        request.response_format.as_ref().unwrap(),
        OpenAiResponseFormat::JsonSchema { .. }
    ));

    let round_trip = serde_json::to_value(request).unwrap();
    assert_eq!(round_trip, body);
}

#[test]
fn responses_request_uses_typed_openai_fields() {
    let body = json!({
        "model": "gpt-5.4",
        "input": [
            {
                "type": "message",
                "role": "user",
                "content": [{"type": "input_text", "text": "hello"}]
            }
        ],
        "reasoning": {"effort": "high", "summary": "concise"},
        "text": {
            "format": {
                "type": "json_schema",
                "name": "answer",
                "schema": {"type": "object"},
                "strict": true
            }
        },
        "tool_choice": {"type": "mcp", "server_label": "deepwiki", "name": "lookup"},
        "tools": [
            {
                "type": "function",
                "name": "answer",
                "parameters": {"type": "object"},
                "strict": true
            },
            {
                "type": "custom",
                "name": "freeform",
                "format": {
                    "type": "grammar",
                    "grammar": {"definition": "start: WORD", "syntax": "regex"}
                }
            }
        ],
        "metadata": {"purpose": "review"}
    });

    let request: OpenAiResponsesRequest = serde_json::from_value(body.clone()).unwrap();

    assert!(request.settings.reasoning.is_some());
    assert!(matches!(
        request.text.as_ref().unwrap().format.as_ref().unwrap(),
        OpenAiTextResponseFormat::JsonSchema { .. }
    ));
    assert!(matches!(
        request.tool_choice.as_ref().unwrap(),
        OpenAiResponsesToolChoice::Mcp(_)
    ));
    assert_eq!(
        request
            .settings
            .metadata
            .as_ref()
            .unwrap()
            .get("purpose")
            .unwrap(),
        "review"
    );

    let round_trip = serde_json::to_value(request).unwrap();
    assert_eq!(round_trip, body);
}

#[test]
fn nested_openai_chat_contracts_reject_unknown_fields() {
    let invalid_audio: Result<OpenAiChatRequest, _> = serde_json::from_value(json!({
        "model": "gpt-4o-audio-preview",
        "messages": [{"role": "user", "content": "hello"}],
        "audio": {"voice": "alloy", "format": "mp3", "extra": true}
    }));

    assert!(invalid_audio.is_err());
}

#[test]
fn openai_usage_details_are_typed() {
    let usage: OpenAiUsage = serde_json::from_value(json!({
        "prompt_tokens": 10,
        "completion_tokens": 5,
        "total_tokens": 15,
        "prompt_tokens_details": {"audio_tokens": 1, "cached_tokens": 4},
        "completion_tokens_details": {
            "accepted_prediction_tokens": 2,
            "audio_tokens": 0,
            "reasoning_tokens": 3,
            "rejected_prediction_tokens": 1
        }
    }))
    .unwrap();

    let normalized = TokenUsage::from(usage);
    assert_eq!(normalized.cache_read_input_tokens, 4);
    assert_eq!(normalized.reasoning_tokens, 3);
}

#[test]
fn responses_response_accepts_openapi_usage_details() {
    let response: OpenAiResponsesResponse = serde_json::from_value(json!({
        "id": "resp_123",
        "object": "response",
        "created_at": 1741476777,
        "status": "completed",
        "model": "gpt-4o-2024-08-06",
        "output": [
            {
                "type": "message",
                "id": "msg_123",
                "status": "completed",
                "role": "assistant",
                "content": [
                    {"type": "output_text", "text": "hello", "annotations": []}
                ]
            }
        ],
        "usage": {
            "input_tokens": 8,
            "input_tokens_details": {"cached_tokens": 2},
            "output_tokens": 6,
            "output_tokens_details": {"reasoning_tokens": 4},
            "total_tokens": 14
        },
        "previous_response_id": null
    }))
    .unwrap();

    let usage = response.usage.unwrap();
    assert_eq!(usage.input_tokens_details.unwrap().cached_tokens, 2);
    assert_eq!(usage.output_tokens_details.unwrap().reasoning_tokens, 4);
}

#[test]
fn responses_stream_events_use_openapi_discriminators() {
    let event: OpenAiResponsesStreamEvent = serde_json::from_value(json!({
        "type": "response.failed",
        "sequence_number": 3,
        "response": {
            "id": "resp_123",
            "object": "response",
            "created_at": 1740855869,
            "status": "failed",
            "completed_at": null,
            "error": {
                "code": "server_error",
                "message": "The model failed to generate a response."
            },
            "model": "gpt-4o-2024-08-06",
            "output": [],
            "usage": {
                "input_tokens": 0,
                "input_tokens_details": {"cached_tokens": 0},
                "output_tokens": 0,
                "output_tokens_details": {"reasoning_tokens": 0},
                "total_tokens": 0
            }
        }
    }))
    .unwrap();

    assert!(matches!(
        event,
        OpenAiResponsesStreamEvent::ResponseFailed { .. }
    ));
    assert_eq!(
        serde_json::to_value(OpenAiResponsesStreamEvent::ResponseCompleted {
            response: OpenAiResponsesResponse {
                id: "resp_123".to_string(),
                object: "response".to_string(),
                model: "gpt-4o-2024-08-06".to_string(),
                status: "completed".to_string(),
                created_at: 1740855869,
                output: Vec::new(),
                usage: None,
                previous_response_id: None,
            },
        })
        .unwrap()["type"],
        "response.completed"
    );
}
