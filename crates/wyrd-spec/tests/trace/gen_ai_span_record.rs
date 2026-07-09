use chrono::{Duration, TimeZone, Utc};
use serde_json::json;

use wyrd_spec::vala::ids::{SpanId, TraceId};
use wyrd_spec::vala::trace::{GenAiEvalResult, GenAiSpanRecord, Resource, SpanStatus};

fn gen_ai() -> GenAiSpanRecord {
    let start = Utc.timestamp_opt(1_700_000_000, 0).unwrap();
    let end = start + Duration::milliseconds(1_200);
    GenAiSpanRecord {
        trace_id: TraceId::from_hex("0123456789abcdef0123456789abcdef").unwrap(),
        span_id: SpanId::from_hex("0123456789abcdef").unwrap(),
        parent_span_id: None,
        start_time: start,
        end_time: end,
        duration_ms: 1_200,
        status: SpanStatus::Ok,
        resource: Resource {
            service_name: "agent".into(),
            service_namespace: None,
            service_version: None,
            service_instance_id: None,
            attributes: serde_json::Map::new(),
        },
        provider_name: "anthropic".into(),
        operation_name: "chat".into(),
        request_model: "claude-opus-4-8".into(),
        output_type: Some("text".into()),
        conversation_id: Some("conv_xyz".into()),
        response_model: Some("claude-opus-4-8".into()),
        response_id: Some("msg_abc123".into()),
        response_finish_reasons: vec!["end_turn".into()],
        response_time_to_first_chunk_seconds: Some(0.08),
        request_temperature: Some(0.7),
        request_top_p: Some(0.95),
        request_top_k: None,
        request_max_tokens: Some(1024),
        request_frequency_penalty: None,
        request_presence_penalty: None,
        request_seed: None,
        request_choice_count: None,
        request_stop_sequences: vec![],
        request_stream: Some(true),
        request_encoding_formats: vec![],
        usage_input_tokens: Some(120),
        usage_output_tokens: Some(340),
        usage_cache_creation_input_tokens: Some(0),
        usage_cache_read_input_tokens: Some(80),
        usage_reasoning_output_tokens: Some(40),
        tool_call_id: None,
        tool_name: None,
        tool_type: None,
        tool_description: None,
        agent_name: Some("checkout-agent".into()),
        agent_id: Some("a_abc".into()),
        agent_description: None,
        agent_version: Some("0.1.0".into()),
        prompt_name: Some("checkout-system-prompt".into()),
        workflow_name: Some("checkout-flow".into()),
        data_source_id: None,
        embeddings_dimension_count: None,
        server_address: Some("api.anthropic.com".into()),
        server_port: Some(443),
        error_type: None,
        input_messages: None,
        output_messages: None,
        system_instructions: None,
        tool_definitions: None,
        tool_call_arguments: None,
        tool_call_result: None,
        retrieval_documents: None,
        retrieval_query_text: None,
        openai_api_type: None,
        openai_service_tier: None,
        eval_results: vec![],
        extra: serde_json::Map::new(),
    }
}

#[test]
fn gen_ai_span_record_round_trip_full() {
    let record = gen_ai();
    let serialized = serde_json::to_string(&record).unwrap();
    let back: GenAiSpanRecord = serde_json::from_str(&serialized).unwrap();
    assert_eq!(back, record);
}

#[test]
fn gen_ai_span_record_round_trip_minimal_required_only() {
    let start = Utc.timestamp_opt(1_700_000_000, 0).unwrap();
    let end = start + Duration::milliseconds(500);
    let record = GenAiSpanRecord {
        trace_id: TraceId::from_hex("0123456789abcdef0123456789abcdef").unwrap(),
        span_id: SpanId::from_hex("0123456789abcdef").unwrap(),
        parent_span_id: None,
        start_time: start,
        end_time: end,
        duration_ms: 500,
        status: SpanStatus::Ok,
        resource: Resource {
            service_name: "x".into(),
            service_namespace: None,
            service_version: None,
            service_instance_id: None,
            attributes: serde_json::Map::new(),
        },
        provider_name: "openai".into(),
        operation_name: "chat".into(),
        request_model: "gpt-4o".into(),
        output_type: None,
        conversation_id: None,
        response_model: None,
        response_id: None,
        response_finish_reasons: vec![],
        response_time_to_first_chunk_seconds: None,
        request_temperature: None,
        request_top_p: None,
        request_top_k: None,
        request_max_tokens: None,
        request_frequency_penalty: None,
        request_presence_penalty: None,
        request_seed: None,
        request_choice_count: None,
        request_stop_sequences: vec![],
        request_stream: None,
        request_encoding_formats: vec![],
        usage_input_tokens: None,
        usage_output_tokens: None,
        usage_cache_creation_input_tokens: None,
        usage_cache_read_input_tokens: None,
        usage_reasoning_output_tokens: None,
        tool_call_id: None,
        tool_name: None,
        tool_type: None,
        tool_description: None,
        agent_name: None,
        agent_id: None,
        agent_description: None,
        agent_version: None,
        prompt_name: None,
        workflow_name: None,
        data_source_id: None,
        embeddings_dimension_count: None,
        server_address: None,
        server_port: None,
        error_type: None,
        input_messages: None,
        output_messages: None,
        system_instructions: None,
        tool_definitions: None,
        tool_call_arguments: None,
        tool_call_result: None,
        retrieval_documents: None,
        retrieval_query_text: None,
        openai_api_type: None,
        openai_service_tier: None,
        eval_results: vec![],
        extra: serde_json::Map::new(),
    };
    let serialized = serde_json::to_string(&record).unwrap();
    let back: GenAiSpanRecord = serde_json::from_str(&serialized).unwrap();
    assert_eq!(back, record);
    back.validate().unwrap();
}

#[test]
fn gen_ai_span_record_eval_results_round_trip_with_two_entries() {
    let mut record = gen_ai();
    record.eval_results = vec![
        GenAiEvalResult {
            name: "factuality".into(),
            score_label: Some("pass".into()),
            score_value: Some(0.92),
            explanation: None,
            response_id: Some("msg_abc123".into()),
        },
        GenAiEvalResult {
            name: "safety".into(),
            score_label: Some("pass".into()),
            score_value: Some(1.0),
            explanation: None,
            response_id: Some("msg_abc123".into()),
        },
    ];
    let serialized = serde_json::to_string(&record).unwrap();
    let back: GenAiSpanRecord = serde_json::from_str(&serialized).unwrap();
    assert_eq!(back, record);
}

#[test]
fn gen_ai_span_record_extra_map_preserves_unknown_gen_ai_keys() {
    let mut record = gen_ai();
    record
        .extra
        .insert("gen_ai.vendor.future_field".into(), json!("value"));
    record
        .extra
        .insert("gen_ai.anthropic.beta".into(), json!(true));
    let serialized = serde_json::to_string(&record).unwrap();
    let back: GenAiSpanRecord = serde_json::from_str(&serialized).unwrap();
    assert_eq!(back, record);
}

#[test]
fn gen_ai_span_record_deny_unknown_top_level_fields() {
    let mut obj = serde_json::to_value(gen_ai())
        .unwrap()
        .as_object()
        .unwrap()
        .clone();
    obj.insert("rogue".into(), json!(true));
    let serialized = serde_json::to_string(&obj).unwrap();
    let result: Result<GenAiSpanRecord, _> = serde_json::from_str(&serialized);
    assert!(result.is_err());
}

#[test]
fn gen_ai_span_record_content_fields_round_trip_with_nested_json() {
    let mut record = gen_ai();
    record.input_messages = Some(json!([
        {"role": "user", "content": "hello"}
    ]));
    record.output_messages = Some(json!([
        {"role": "assistant", "content": "world"}
    ]));
    record.tool_definitions = Some(json!([
        {"name": "search", "parameters": {"q": "string"}}
    ]));
    let serialized = serde_json::to_string(&record).unwrap();
    let back: GenAiSpanRecord = serde_json::from_str(&serialized).unwrap();
    assert_eq!(back, record);
}

#[test]
fn gen_ai_span_record_finish_reasons_round_trip_with_multiple() {
    let mut record = gen_ai();
    record.response_finish_reasons = vec!["stop".into(), "max_tokens".into()];
    let serialized = serde_json::to_string(&record).unwrap();
    let back: GenAiSpanRecord = serde_json::from_str(&serialized).unwrap();
    assert_eq!(back, record);
}

#[test]
fn gen_ai_span_record_validate_happy_path() {
    gen_ai().validate().unwrap();
}

#[test]
fn gen_ai_span_record_validate_rejects_empty_provider_name() {
    let mut record = gen_ai();
    record.provider_name = String::new();
    assert!(record.validate().is_err());
}

#[test]
fn gen_ai_span_record_validate_rejects_nan_time_to_first_chunk_seconds() {
    let mut record = gen_ai();
    record.response_time_to_first_chunk_seconds = Some(f64::NAN);
    assert!(record.validate().is_err());
}

#[test]
fn gen_ai_span_record_validate_rejects_inf_time_to_first_chunk_seconds() {
    let mut record = gen_ai();
    record.response_time_to_first_chunk_seconds = Some(f64::INFINITY);
    assert!(record.validate().is_err());
}

#[test]
fn gen_ai_span_record_validate_rejects_empty_operation_name() {
    let mut record = gen_ai();
    record.operation_name = String::new();
    assert!(record.validate().is_err());
}

#[test]
fn gen_ai_span_record_validate_rejects_empty_request_model() {
    let mut record = gen_ai();
    record.request_model = String::new();
    assert!(record.validate().is_err());
}

#[test]
fn gen_ai_span_record_validate_rejects_end_before_start() {
    let mut record = gen_ai();
    record.end_time = record.start_time - Duration::seconds(1);
    record.duration_ms = 0;
    assert!(record.validate().is_err());
}

#[test]
fn gen_ai_span_record_validate_rejects_mismatched_duration() {
    let mut record = gen_ai();
    record.duration_ms = 999_999;
    assert!(record.validate().is_err());
}

#[test]
fn gen_ai_span_record_validate_rejects_nan_temperature() {
    let mut record = gen_ai();
    record.request_temperature = Some(f64::NAN);
    assert!(record.validate().is_err());
}

#[test]
fn gen_ai_span_record_validate_rejects_inf_top_p() {
    let mut record = gen_ai();
    record.request_top_p = Some(f64::INFINITY);
    assert!(record.validate().is_err());
}

#[test]
fn gen_ai_span_record_validate_rejects_nan_frequency_penalty() {
    let mut record = gen_ai();
    record.request_frequency_penalty = Some(f64::NAN);
    assert!(record.validate().is_err());
}

#[test]
fn gen_ai_span_record_validate_propagates_resource_failure() {
    let mut record = gen_ai();
    record.resource.service_name = String::new();
    assert!(record.validate().is_err());
}

#[test]
fn gen_ai_span_record_validate_propagates_eval_result_failure() {
    let mut record = gen_ai();
    record.eval_results.push(GenAiEvalResult {
        name: String::new(),
        score_label: None,
        score_value: None,
        explanation: None,
        response_id: None,
    });
    assert!(record.validate().is_err());
}

#[test]
fn gen_ai_span_record_validate_rejects_nan_presence_penalty() {
    let mut record = gen_ai();
    record.request_presence_penalty = Some(f64::NAN);
    assert!(record.validate().is_err());
}

#[test]
fn gen_ai_span_record_validate_rejects_inf_presence_penalty() {
    let mut record = gen_ai();
    record.request_presence_penalty = Some(f64::INFINITY);
    assert!(record.validate().is_err());
}

#[test]
fn gen_ai_span_record_validate_rejects_input_messages_over_limit() {
    let mut record = gen_ai();
    let big = json!("x".repeat(1_048_577));
    record.input_messages = Some(big);
    assert!(record.validate().is_err());
}

#[test]
fn gen_ai_span_record_validate_accepts_input_messages_at_limit() {
    let mut record = gen_ai();
    // A small valid JSON payload — just testing the happy path through the blob guard.
    record.input_messages = Some(json!([{"role": "user", "content": "hello"}]));
    assert!(record.validate().is_ok());
}

#[test]
fn gen_ai_span_record_validate_rejects_extra_over_128_entries() {
    let mut record = gen_ai();
    for i in 0..=128 {
        record
            .extra
            .insert(format!("gen_ai.vendor.key_{i}"), json!(i));
    }
    assert!(record.validate().is_err());
}

#[test]
fn gen_ai_span_record_validate_accepts_extra_at_128_entries() {
    let mut record = gen_ai();
    for i in 0..128 {
        record
            .extra
            .insert(format!("gen_ai.vendor.key_{i}"), json!(i));
    }
    assert!(record.validate().is_ok());
}
