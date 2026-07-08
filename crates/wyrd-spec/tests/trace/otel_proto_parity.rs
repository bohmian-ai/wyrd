//! Lock the OTel-canonical trace surface against accidental drift.

use chrono::{Duration, TimeZone, Timelike, Utc};
use schemars::schema_for;
use serde_json::Value;

use wyrd_spec::DataTenantId;
use wyrd_spec::vala::ids::{SpanId, TraceId};
use wyrd_spec::vala::trace::{
    GenAiSpanRecord, InstrumentationScope, Resource, SpanKind, SpanRecord, SpanStatus,
    TraceSummaryRecord,
};

#[test]
fn span_kind_has_exactly_five_variants_per_otel_spec() {
    assert_eq!(
        schema_one_of_enum_values::<SpanKind>(),
        ["internal", "server", "client", "producer", "consumer"],
        "SpanKind must expose exactly 5 variants per OTel spec"
    );
}

#[test]
fn span_status_has_exactly_three_codes_per_otel_spec() {
    assert_eq!(
        span_status_schema_codes(),
        ["unset", "ok", "error"],
        "SpanStatus must expose exactly 3 codes per OTel spec"
    );
}

fn schema_one_of_enum_values<T: schemars::JsonSchema>() -> Vec<String> {
    let schema = serde_json::to_value(schema_for!(T)).expect("schema serializes to JSON");
    schema["oneOf"]
        .as_array()
        .expect("schema has oneOf variants")
        .iter()
        .map(|variant| {
            variant["enum"][0]
                .as_str()
                .expect("variant has one enum value")
                .to_string()
        })
        .collect()
}

fn span_status_schema_codes() -> Vec<String> {
    let schema = serde_json::to_value(schema_for!(SpanStatus)).expect("schema serializes to JSON");
    schema["oneOf"]
        .as_array()
        .expect("span status schema has oneOf variants")
        .iter()
        .map(status_code)
        .collect()
}

fn status_code(variant: &Value) -> String {
    variant["properties"]["code"]["enum"][0]
        .as_str()
        .expect("span status variant has a code enum value")
        .to_string()
}

#[test]
fn span_record_round_trips_otlp_shaped_json() {
    let serialized = r#"{
        "trace_id": "0123456789abcdef0123456789abcdef",
        "span_id": "0123456789abcdef",
        "parent_span_id": "fedcba9876543210",
        "flags": 65537,
        "trace_state": "vendor1=abc",
        "name": "GET /checkout",
        "kind": "server",
        "start_time": "2023-11-14T22:13:20Z",
        "end_time": "2023-11-14T22:13:21.500Z",
        "duration_ms": 1500,
        "status": {"code": "error", "description": "upstream timeout"},
        "attributes": {
            "http.method": "GET",
            "http.status_code": 504,
            "wyrd.tracing.label": "manual"
        },
        "dropped_attributes_count": 2,
        "events": [
            {
                "timestamp": "2023-11-14T22:13:21.000Z",
                "name": "exception",
                "attributes": {
                    "exception.type": "UpstreamTimeout"
                },
                "dropped_attributes_count": 0
            }
        ],
        "dropped_events_count": 1,
        "links": [
            {
                "trace_id": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                "span_id": "aaaaaaaaaaaaaaaa",
                "trace_state": "",
                "flags": 1,
                "attributes": {},
                "dropped_attributes_count": 0
            }
        ],
        "dropped_links_count": 0,
        "scope": {
            "name": "wyrd-tracing",
            "version": "0.3.1",
            "attributes": {}
        },
        "resource": {
            "service_name": "checkout",
            "service_namespace": "payments",
            "service_version": "1.2.3",
            "service_instance_id": "worker-0",
            "attributes": {
                "telemetry.sdk.name": "wyrd-tracing",
                "host.name": "worker-0"
            }
        }
    }"#;

    let record: SpanRecord =
        serde_json::from_str(serialized).expect("OTLP-shaped JSON must deserialize cleanly");
    record.validate().expect("OTLP-shaped record must validate");

    assert_eq!(record.flags, 65_537, "flags must preserve upper bits");
    assert_eq!(record.dropped_attributes_count, 2);
    assert_eq!(record.dropped_events_count, 1);
    assert_eq!(record.dropped_links_count, 0);
    assert_eq!(record.links[0].flags, 1, "SpanLink.flags must round-trip");

    let round_tripped = serde_json::to_string(&record).expect("span record serializes");
    let back: SpanRecord =
        serde_json::from_str(&round_tripped).expect("serialized span record deserializes");
    assert_eq!(back, record);

    assert!(
        round_tripped.contains("\"trace_id\":\"0123456789abcdef0123456789abcdef\""),
        "trace_id must serialize as hex string, got: {round_tripped}"
    );
    assert!(
        round_tripped.contains("\"span_id\":\"0123456789abcdef\""),
        "span_id must serialize as hex string, got: {round_tripped}"
    );
    assert!(
        !round_tripped.contains("\"trace_id\":["),
        "trace_id must NOT serialize as byte array, got: {round_tripped}"
    );
}

#[test]
fn span_record_minimal_otel_compliant_form_validates() {
    let start = Utc
        .timestamp_opt(1_700_000_000, 0)
        .single()
        .expect("timestamp is valid");
    let end = start + Duration::milliseconds(1);
    let record = SpanRecord {
        trace_id: TraceId::from_hex("0123456789abcdef0123456789abcdef").expect("trace id is valid"),
        span_id: SpanId::from_hex("0123456789abcdef").expect("span id is valid"),
        parent_span_id: None,
        flags: 0,
        trace_state: String::new(),
        name: "op".into(),
        kind: SpanKind::Internal,
        start_time: start,
        end_time: end,
        duration_ms: 1,
        status: SpanStatus::Unset,
        attributes: serde_json::Map::new(),
        dropped_attributes_count: 0,
        events: Vec::new(),
        dropped_events_count: 0,
        links: Vec::new(),
        dropped_links_count: 0,
        scope: InstrumentationScope {
            name: "wyrd-tracing".into(),
            version: None,
            attributes: serde_json::Map::new(),
        },
        resource: Resource {
            service_name: "x".into(),
            service_namespace: None,
            service_version: None,
            service_instance_id: None,
            attributes: serde_json::Map::new(),
        },
    };

    record.validate().expect("minimal OTel span validates");
}

#[test]
fn trace_summary_record_json_field_names_match_wire_spec() {
    let start = Utc
        .timestamp_opt(1_700_000_030, 0)
        .single()
        .expect("timestamp is valid");
    let end = start + Duration::milliseconds(2_500);
    let bucket_time = start.with_second(0).unwrap().with_nanosecond(0).unwrap();
    let record = TraceSummaryRecord {
        trace_id: TraceId::from_hex("0123456789abcdef0123456789abcdef").expect("valid trace id"),
        data_tenant_id: DataTenantId::new_v7(),
        bucket_time,
        start_time: start,
        end_time: end,
        duration_ms: 2_500,
        span_count: 4,
        error_count: 0,
        root_span_id: Some(SpanId::from_hex("0123456789abcdef").expect("valid span id")),
        root_name: Some("GET /".into()),
        root_kind: Some(SpanKind::Server),
        root_status: SpanStatus::Ok,
        root_scope: None,
        resource: Resource {
            service_name: "svc".into(),
            service_namespace: None,
            service_version: None,
            service_instance_id: None,
            attributes: serde_json::Map::new(),
        },
    };

    let j = serde_json::to_value(&record).expect("serializes");
    let obj = j.as_object().expect("is an object");

    assert!(obj.contains_key("trace_id"), "missing trace_id");
    assert!(obj.contains_key("data_tenant_id"), "missing data_tenant_id");
    assert!(
        obj.contains_key("bucket_time"),
        "missing bucket_time (Delta partition key)"
    );
    assert!(obj.contains_key("start_time"), "missing start_time");
    assert!(obj.contains_key("end_time"), "missing end_time");
    assert!(obj.contains_key("duration_ms"), "missing duration_ms");
    assert!(obj.contains_key("span_count"), "missing span_count");
    assert!(obj.contains_key("error_count"), "missing error_count");
    assert!(obj.contains_key("root_span_id"), "missing root_span_id");
    assert!(obj.contains_key("root_name"), "missing root_name");
    assert!(obj.contains_key("root_kind"), "missing root_kind");
    assert!(obj.contains_key("root_status"), "missing root_status");
    assert!(obj.contains_key("resource"), "missing resource");

    // Verify trace_id is hex, not a byte array.
    assert_eq!(
        obj["trace_id"].as_str().expect("trace_id is string"),
        "0123456789abcdef0123456789abcdef"
    );
}

#[test]
fn gen_ai_span_record_json_field_names_match_wire_spec() {
    let start = Utc
        .timestamp_opt(1_700_000_000, 0)
        .single()
        .expect("timestamp is valid");
    let end = start + Duration::milliseconds(1_000);
    let record = GenAiSpanRecord {
        trace_id: TraceId::from_hex("0123456789abcdef0123456789abcdef").expect("valid trace id"),
        span_id: SpanId::from_hex("0123456789abcdef").expect("valid span id"),
        parent_span_id: None,
        start_time: start,
        end_time: end,
        duration_ms: 1_000,
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

    let j = serde_json::to_value(&record).expect("serializes");
    let obj = j.as_object().expect("is an object");

    assert!(obj.contains_key("trace_id"), "missing trace_id");
    assert!(obj.contains_key("span_id"), "missing span_id");
    assert!(!obj.contains_key("data_tenant_id"), "data_tenant_id must not be a GenAiSpanRecord field (C-02)");
    assert!(obj.contains_key("provider_name"), "missing provider_name");
    assert!(obj.contains_key("operation_name"), "missing operation_name");
    assert!(obj.contains_key("request_model"), "missing request_model");
    assert!(obj.contains_key("eval_results"), "missing eval_results");
    assert!(obj.contains_key("extra"), "missing extra");
    assert!(obj.contains_key("resource"), "missing resource");
    assert!(obj.contains_key("status"), "missing status");

    // Verify IDs serialize as hex strings, not byte arrays.
    assert_eq!(
        obj["trace_id"].as_str().expect("trace_id is string"),
        "0123456789abcdef0123456789abcdef"
    );
    assert_eq!(
        obj["span_id"].as_str().expect("span_id is string"),
        "0123456789abcdef"
    );
}
