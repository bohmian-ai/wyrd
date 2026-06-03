//! Lock the OTel-canonical trace surface against accidental drift.

use chrono::{Duration, TimeZone, Utc};
use schemars::schema_for;
use serde_json::Value;

use wyrd_spec::vala::ids::{DataTenantId, SpanId, TraceId};
use wyrd_spec::vala::trace::{InstrumentationScope, Resource, SpanKind, SpanRecord, SpanStatus};

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
        },
        "data_tenant_id": "acme"
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
        data_tenant_id: DataTenantId::new("acme").expect("tenant id is valid"),
    };

    record.validate().expect("minimal OTel span validates");
}
