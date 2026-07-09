use chrono::{Duration, TimeZone, Utc};
use serde_json::json;

use wyrd_spec::vala::ids::{SpanId, TraceId};
use wyrd_spec::vala::trace::{
    InstrumentationScope, Resource, SpanEvent, SpanKind, SpanLink, SpanRecord, SpanStatus,
};

fn span() -> SpanRecord {
    let start = Utc.timestamp_opt(1_700_000_000, 0).unwrap();
    let end = start + Duration::milliseconds(150);
    SpanRecord {
        trace_id: TraceId::from_hex("0123456789abcdef0123456789abcdef").unwrap(),
        span_id: SpanId::from_hex("0123456789abcdef").unwrap(),
        parent_span_id: None,
        flags: 1,
        trace_state: String::new(),
        name: "GET /checkout".into(),
        kind: SpanKind::Server,
        start_time: start,
        end_time: end,
        duration_ms: 150,
        status: SpanStatus::Ok,
        attributes: serde_json::Map::new(),
        dropped_attributes_count: 0,
        events: vec![],
        dropped_events_count: 0,
        links: vec![],
        dropped_links_count: 0,
        scope: InstrumentationScope {
            name: "wyrd-tracing".into(),
            version: Some("0.3.1".into()),
            attributes: serde_json::Map::new(),
        },
        resource: Resource {
            service_name: "checkout".into(),
            service_namespace: None,
            service_version: None,
            service_instance_id: None,
            attributes: serde_json::Map::new(),
        },
    }
}

#[test]
fn span_record_round_trip_minimal() {
    let s = span();
    let j = serde_json::to_string(&s).unwrap();
    let back: SpanRecord = serde_json::from_str(&j).unwrap();
    assert_eq!(back, s);
}

#[test]
fn span_record_round_trip_full_otel_surface() {
    let mut s = span();
    let parent = SpanId::from_hex("fedcba9876543210").unwrap();
    s.parent_span_id = Some(parent);
    s.trace_state = "vendor1=abc,vendor2=def".to_string();
    s.flags = 1;
    s.status = SpanStatus::Error {
        description: Some("upstream timeout".into()),
    };
    s.attributes.insert("http.method".into(), json!("GET"));
    s.attributes.insert("http.status_code".into(), json!(504));
    s.events.push(SpanEvent {
        timestamp: s.start_time + Duration::milliseconds(50),
        name: "exception".into(),
        attributes: {
            let mut attributes = serde_json::Map::new();
            attributes.insert("exception.type".into(), json!("UpstreamTimeout"));
            attributes
        },
        dropped_attributes_count: 0,
    });
    s.links.push(SpanLink {
        trace_id: TraceId::from_hex("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa").unwrap(),
        span_id: SpanId::from_hex("aaaaaaaaaaaaaaaa").unwrap(),
        trace_state: String::new(),
        flags: 0,
        attributes: serde_json::Map::new(),
        dropped_attributes_count: 0,
    });
    s.resource.service_namespace = Some("payments".into());
    s.resource.service_version = Some("1.2.3".into());
    s.resource.service_instance_id = Some("worker-0".into());
    s.resource
        .attributes
        .insert("host.name".into(), json!("worker-0"));
    s.scope
        .attributes
        .insert("ext.runtime".into(), json!("rust"));

    let j = serde_json::to_string(&s).unwrap();
    let back: SpanRecord = serde_json::from_str(&j).unwrap();
    assert_eq!(back, s);
    back.validate().unwrap();
}

#[test]
fn span_record_deny_unknown_fields() {
    let j = serde_json::to_value(span()).unwrap();
    let mut obj = j.as_object().unwrap().clone();
    obj.insert("rogue".into(), json!(true));
    let s = serde_json::to_string(&obj).unwrap();
    let r: Result<SpanRecord, _> = serde_json::from_str(&s);
    assert!(r.is_err());
}

#[test]
fn span_kind_round_trip_all_five() {
    for kind in [
        SpanKind::Internal,
        SpanKind::Server,
        SpanKind::Client,
        SpanKind::Producer,
        SpanKind::Consumer,
    ] {
        let serialized = serde_json::to_string(&kind).unwrap();
        let back: SpanKind = serde_json::from_str(&serialized).unwrap();
        assert_eq!(back, kind);
    }
}

#[test]
fn span_kind_serializes_snake_case() {
    assert_eq!(
        serde_json::to_string(&SpanKind::Server).unwrap(),
        "\"server\""
    );
    assert_eq!(
        serde_json::to_string(&SpanKind::Internal).unwrap(),
        "\"internal\""
    );
}

#[test]
fn span_kind_rejects_unknown_variant() {
    let r: Result<SpanKind, _> = serde_json::from_str("\"other\"");
    assert!(r.is_err());
}

#[test]
fn span_status_round_trip_unset() {
    let status = SpanStatus::Unset;
    let serialized = serde_json::to_string(&status).unwrap();
    assert_eq!(serialized, r#"{"code":"unset"}"#);
    let back: SpanStatus = serde_json::from_str(&serialized).unwrap();
    assert_eq!(back, status);
}

#[test]
fn span_status_round_trip_ok() {
    let status = SpanStatus::Ok;
    let serialized = serde_json::to_string(&status).unwrap();
    assert_eq!(serialized, r#"{"code":"ok"}"#);
    let back: SpanStatus = serde_json::from_str(&serialized).unwrap();
    assert_eq!(back, status);
}

#[test]
fn span_status_round_trip_error_no_description() {
    let status = SpanStatus::Error { description: None };
    let serialized = serde_json::to_string(&status).unwrap();
    assert_eq!(serialized, r#"{"code":"error"}"#);
    let back: SpanStatus = serde_json::from_str(&serialized).unwrap();
    assert_eq!(back, status);
}

#[test]
fn span_status_round_trip_error_with_description() {
    let status = SpanStatus::Error {
        description: Some("upstream timeout".into()),
    };
    let serialized = serde_json::to_string(&status).unwrap();
    assert_eq!(
        serialized,
        r#"{"code":"error","description":"upstream timeout"}"#
    );
    let back: SpanStatus = serde_json::from_str(&serialized).unwrap();
    assert_eq!(back, status);
}

#[test]
fn span_status_rejects_unknown_code() {
    let r: Result<SpanStatus, _> = serde_json::from_str(r#"{"code":"other"}"#);
    assert!(r.is_err());
}

#[test]
fn span_record_service_name_helper_returns_resource_service_name() {
    let s = span();
    assert_eq!(s.service_name(), "checkout");
}

#[test]
fn span_record_validate_happy_path() {
    span().validate().unwrap();
}

#[test]
fn span_record_validate_rejects_empty_name() {
    let mut s = span();
    s.name = String::new();
    assert!(s.validate().is_err());
}

#[test]
fn span_record_validate_rejects_overlong_name() {
    let mut s = span();
    s.name = "a".repeat(257);
    assert!(s.validate().is_err());
}

#[test]
fn span_record_validate_rejects_end_before_start() {
    let mut s = span();
    s.end_time = s.start_time - Duration::seconds(1);
    s.duration_ms = 0;
    assert!(s.validate().is_err());
}

#[test]
fn span_record_validate_rejects_mismatched_duration_ms() {
    let mut s = span();
    s.duration_ms = 999;
    assert!(s.validate().is_err());
}

#[test]
fn span_record_validate_rejects_overlong_trace_state() {
    let mut s = span();
    s.trace_state = "a".repeat(513);
    assert!(s.validate().is_err());
}

#[test]
fn span_record_validate_accepts_512_byte_trace_state() {
    let mut s = span();
    s.trace_state = "a".repeat(512);
    s.validate().unwrap();
}

#[test]
fn span_record_validate_rejects_event_before_start() {
    let mut s = span();
    let bad_ts = s.start_time - Duration::milliseconds(1);
    s.events.push(SpanEvent {
        timestamp: bad_ts,
        name: "x".into(),
        attributes: serde_json::Map::new(),
        dropped_attributes_count: 0,
    });
    assert!(s.validate().is_err());
}

#[test]
fn span_record_validate_rejects_event_after_end() {
    let mut s = span();
    let bad_ts = s.end_time + Duration::milliseconds(1);
    s.events.push(SpanEvent {
        timestamp: bad_ts,
        name: "x".into(),
        attributes: serde_json::Map::new(),
        dropped_attributes_count: 0,
    });
    assert!(s.validate().is_err());
}

#[test]
fn span_record_validate_accepts_event_at_window_boundaries() {
    let mut s = span();
    s.events.push(SpanEvent {
        timestamp: s.start_time,
        name: "at-start".into(),
        attributes: serde_json::Map::new(),
        dropped_attributes_count: 0,
    });
    s.events.push(SpanEvent {
        timestamp: s.end_time,
        name: "at-end".into(),
        attributes: serde_json::Map::new(),
        dropped_attributes_count: 0,
    });
    s.validate().unwrap();
}

#[test]
fn span_record_validate_propagates_event_validate_failure() {
    let mut s = span();
    s.events.push(SpanEvent {
        timestamp: s.start_time + Duration::milliseconds(10),
        name: String::new(),
        attributes: serde_json::Map::new(),
        dropped_attributes_count: 0,
    });
    assert!(s.validate().is_err());
}

#[test]
fn span_record_validate_propagates_link_validate_failure() {
    let mut s = span();
    s.links.push(SpanLink {
        trace_id: TraceId::from_hex("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa").unwrap(),
        span_id: SpanId::from_hex("aaaaaaaaaaaaaaaa").unwrap(),
        trace_state: "x".repeat(513),
        flags: 0,
        attributes: serde_json::Map::new(),
        dropped_attributes_count: 0,
    });
    assert!(s.validate().is_err());
}

#[test]
fn span_record_validate_propagates_resource_validate_failure() {
    let mut s = span();
    s.resource.service_name = String::new();
    assert!(s.validate().is_err());
}

#[test]
fn span_record_validate_propagates_scope_validate_failure() {
    let mut s = span();
    s.scope.name = String::new();
    assert!(s.validate().is_err());
}

#[test]
fn span_record_deserialize_rejects_all_zero_parent_span_id() {
    let mut j = serde_json::to_value(span()).unwrap();
    let obj = j.as_object_mut().unwrap();
    obj.insert("parent_span_id".into(), json!("0000000000000000"));
    let serialized = serde_json::to_string(&obj).unwrap();
    let r: Result<SpanRecord, _> = serde_json::from_str(&serialized);
    assert!(r.is_err());
}

#[test]
fn span_record_attributes_preserve_nested_arrays_and_objects() {
    let mut s = span();
    s.attributes
        .insert("nested".into(), json!({"arr": [1, 2], "obj": {"k": "v"}}));
    let serialized = serde_json::to_string(&s).unwrap();
    let back: SpanRecord = serde_json::from_str(&serialized).unwrap();
    assert_eq!(back, s);
}

#[test]
fn span_record_flags_preserves_upper_bits_round_trip() {
    let mut s = span();
    s.flags = 0x0001_0001;
    let serialized = serde_json::to_string(&s).unwrap();
    let back: SpanRecord = serde_json::from_str(&serialized).unwrap();
    assert_eq!(back.flags, 0x0001_0001);
}

#[test]
fn span_record_dropped_counts_default_zero_on_wire() {
    let mut obj = serde_json::to_value(span()).unwrap();
    let object = obj.as_object_mut().unwrap();
    object.remove("dropped_attributes_count");
    object.remove("dropped_events_count");
    object.remove("dropped_links_count");
    let serialized = serde_json::to_string(&obj).unwrap();
    let back: SpanRecord = serde_json::from_str(&serialized).unwrap();
    assert_eq!(back.dropped_attributes_count, 0);
    assert_eq!(back.dropped_events_count, 0);
    assert_eq!(back.dropped_links_count, 0);
}

#[test]
fn span_record_dropped_counts_round_trip_nonzero() {
    let mut s = span();
    s.dropped_attributes_count = 3;
    s.dropped_events_count = 7;
    s.dropped_links_count = 11;
    let serialized = serde_json::to_string(&s).unwrap();
    let back: SpanRecord = serde_json::from_str(&serialized).unwrap();
    assert_eq!(back.dropped_attributes_count, 3);
    assert_eq!(back.dropped_events_count, 7);
    assert_eq!(back.dropped_links_count, 11);
}

#[test]
fn span_record_validate_rejects_attributes_over_256() {
    let mut s = span();
    for i in 0..=256 {
        s.attributes
            .insert(format!("key_{i}"), serde_json::Value::Bool(true));
    }
    assert!(s.validate().is_err());
}

#[test]
fn span_record_validate_accepts_attributes_at_256() {
    let mut s = span();
    for i in 0..256 {
        s.attributes
            .insert(format!("key_{i}"), serde_json::Value::Bool(true));
    }
    assert!(s.validate().is_ok());
}
