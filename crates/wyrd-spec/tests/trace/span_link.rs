use serde_json::json;

use wyrd_spec::vala::ids::{SpanId, TraceId};
use wyrd_spec::vala::trace::SpanLink;

fn link() -> SpanLink {
    SpanLink {
        trace_id: TraceId::from_hex("0123456789abcdef0123456789abcdef").unwrap(),
        span_id: SpanId::from_hex("0123456789abcdef").unwrap(),
        trace_state: String::new(),
        flags: 0,
        attributes: serde_json::Map::new(),
        dropped_attributes_count: 0,
    }
}

#[test]
fn span_link_round_trip() {
    let l = link();
    let s = serde_json::to_string(&l).unwrap();
    let back: SpanLink = serde_json::from_str(&s).unwrap();
    assert_eq!(back, l);
}

#[test]
fn span_link_round_trip_with_trace_state() {
    let l = SpanLink {
        trace_state: "vendor1=abc,vendor2=def".to_string(),
        ..link()
    };
    let s = serde_json::to_string(&l).unwrap();
    let back: SpanLink = serde_json::from_str(&s).unwrap();
    assert_eq!(back, l);
}

#[test]
fn span_link_trace_state_defaults_empty() {
    let s = r#"{
        "trace_id": "0123456789abcdef0123456789abcdef",
        "span_id": "0123456789abcdef",
        "attributes": {}
    }"#;
    let l: SpanLink = serde_json::from_str(s).unwrap();
    assert_eq!(l.trace_state, "");
}

#[test]
fn span_link_attributes_default_empty() {
    let s = r#"{
        "trace_id": "0123456789abcdef0123456789abcdef",
        "span_id": "0123456789abcdef"
    }"#;
    let l: SpanLink = serde_json::from_str(s).unwrap();
    assert!(l.attributes.is_empty());
}

#[test]
fn span_link_attributes_round_trip_with_nested_json() {
    let mut attrs = serde_json::Map::new();
    attrs.insert("sampling.priority".into(), json!(1));
    attrs.insert("ext.tags".into(), json!(["a", "b"]));
    let l = SpanLink {
        attributes: attrs,
        ..link()
    };
    let s = serde_json::to_string(&l).unwrap();
    let back: SpanLink = serde_json::from_str(&s).unwrap();
    assert_eq!(back, l);
}

#[test]
fn span_link_deny_unknown_fields() {
    let s = r#"{
        "trace_id": "0123456789abcdef0123456789abcdef",
        "span_id": "0123456789abcdef",
        "rogue": true
    }"#;
    let r: Result<SpanLink, _> = serde_json::from_str(s);
    assert!(r.is_err());
}

#[test]
fn span_link_deserialize_rejects_all_zero_trace_id() {
    let s = r#"{
        "trace_id": "00000000000000000000000000000000",
        "span_id": "0123456789abcdef"
    }"#;
    let r: Result<SpanLink, _> = serde_json::from_str(s);
    assert!(r.is_err());
}

#[test]
fn span_link_deserialize_rejects_all_zero_span_id() {
    let s = r#"{
        "trace_id": "0123456789abcdef0123456789abcdef",
        "span_id": "0000000000000000"
    }"#;
    let r: Result<SpanLink, _> = serde_json::from_str(s);
    assert!(r.is_err());
}

#[test]
fn span_link_validate_rejects_overlong_trace_state() {
    let l = SpanLink {
        trace_state: "a".repeat(513),
        ..link()
    };
    assert!(l.validate().is_err());
}

#[test]
fn span_link_validate_accepts_512_byte_trace_state() {
    let l = SpanLink {
        trace_state: "a".repeat(512),
        ..link()
    };
    l.validate().unwrap();
}

#[test]
fn span_link_validate_rejects_overlarge_attributes() {
    let mut attrs = serde_json::Map::new();
    for i in 0..129 {
        attrs.insert(format!("k{i}"), json!(i));
    }
    let l = SpanLink {
        attributes: attrs,
        ..link()
    };
    assert!(l.validate().is_err());
}

#[test]
fn span_link_flags_defaults_zero() {
    let s = r#"{
        "trace_id": "0123456789abcdef0123456789abcdef",
        "span_id": "0123456789abcdef"
    }"#;
    let l: SpanLink = serde_json::from_str(s).unwrap();
    assert_eq!(l.flags, 0);
}

#[test]
fn span_link_flags_round_trip_with_w3c_sampled_bit() {
    let l = SpanLink {
        flags: 0x01,
        ..link()
    };
    let s = serde_json::to_string(&l).unwrap();
    let back: SpanLink = serde_json::from_str(&s).unwrap();
    assert_eq!(back, l);
    assert_eq!(back.flags, 0x01);
}

#[test]
fn span_link_flags_round_trip_upper_bits_preserved() {
    let l = SpanLink {
        flags: 0x0001_0001,
        ..link()
    };
    let s = serde_json::to_string(&l).unwrap();
    let back: SpanLink = serde_json::from_str(&s).unwrap();
    assert_eq!(back.flags, 0x0001_0001);
}
