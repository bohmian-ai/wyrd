use chrono::{TimeZone, Utc};
use serde_json::json;

use wyrd_spec::vala::trace::SpanEvent;

fn ev(name: &str) -> SpanEvent {
    SpanEvent {
        timestamp: Utc.timestamp_opt(1_700_000_000, 0).unwrap(),
        name: name.to_string(),
        attributes: serde_json::Map::new(),
        dropped_attributes_count: 0,
    }
}

#[test]
fn span_event_round_trip() {
    let e = ev("exception");
    let s = serde_json::to_string(&e).unwrap();
    let back: SpanEvent = serde_json::from_str(&s).unwrap();
    assert_eq!(back, e);
}

#[test]
fn span_event_dropped_attributes_defaults_to_zero() {
    let s = r#"{
        "timestamp": "2023-11-14T22:13:20Z",
        "name": "exception",
        "attributes": {}
    }"#;
    let e: SpanEvent = serde_json::from_str(s).unwrap();
    assert_eq!(e.dropped_attributes_count, 0);
}

#[test]
fn span_event_attributes_default_to_empty_map() {
    let s = r#"{
        "timestamp": "2023-11-14T22:13:20Z",
        "name": "exception"
    }"#;
    let e: SpanEvent = serde_json::from_str(s).unwrap();
    assert!(e.attributes.is_empty());
}

#[test]
fn span_event_attributes_round_trip_nested_json() {
    let mut attrs = serde_json::Map::new();
    attrs.insert("exception.type".into(), json!("ValueError"));
    attrs.insert("exception.stacktrace".into(), json!(["frame_a", "frame_b"]));
    let e = SpanEvent {
        attributes: attrs,
        ..ev("exception")
    };
    let s = serde_json::to_string(&e).unwrap();
    let back: SpanEvent = serde_json::from_str(&s).unwrap();
    assert_eq!(back, e);
}

#[test]
fn span_event_deny_unknown_fields() {
    let s = r#"{
        "timestamp": "2023-11-14T22:13:20Z",
        "name": "exception",
        "rogue": true
    }"#;
    let r: Result<SpanEvent, _> = serde_json::from_str(s);
    assert!(r.is_err());
}

#[test]
fn span_event_validate_rejects_empty_name() {
    let e = ev("");
    assert!(e.validate().is_err());
}

#[test]
fn span_event_validate_rejects_overlong_name() {
    let e = SpanEvent {
        name: "a".repeat(257),
        ..ev("x")
    };
    assert!(e.validate().is_err());
}

#[test]
fn span_event_validate_accepts_256_char_name() {
    let e = SpanEvent {
        name: "a".repeat(256),
        ..ev("x")
    };
    e.validate().unwrap();
}

#[test]
fn span_event_validate_rejects_overlarge_attributes() {
    let mut attrs = serde_json::Map::new();
    for i in 0..129 {
        attrs.insert(format!("k{i}"), json!(i));
    }
    let e = SpanEvent {
        attributes: attrs,
        ..ev("x")
    };
    assert!(e.validate().is_err());
}
