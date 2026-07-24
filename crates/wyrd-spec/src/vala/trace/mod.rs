//! Trace primitive surface — OTel-compliant span, summary, and GenAI span
//! records. The shapes here are durable contracts; runtime (OTel SDK
//! singletons, OTLP receiver, exporters, propagation glue) lives in
//! `wyrd-telemetry` (PR4.1) and `vala-ingest` (PR4.2).
//!
//! Span attributes are stored as opaque `serde_json::Map<String,
//! serde_json::Value>` on every record. Downstream (`vala-bifrost`)
//! projects them to `Utf8` Iceberg/Parquet columns and keeps them
//! opaque: DataFusion 54.0.0 reads a requested key from the projected
//! `Utf8` column, with no pre-extraction or JSON shredding into typed
//! sidecar columns. The contract does not pre-extract tag or baggage
//! attributes into separate records — see the trace-primitive README's
//! "Storage doctrine" section for the rationale.
//!
//! Module layout (filled in by subsequent commits):
//!
//! - `attributes` — `wyrd.*` + `gen_ai.*` attribute key constants.
//! - `attribute_value` — typed projection of OTel `AnyValue`.
//! - `span_event`, `span_link` — OTel-canonical nested types.
//! - `resource`, `instrumentation_scope` — OTel-canonical producer typing.
//! - `span` — `SpanRecord` + `SpanKind` + `SpanStatus`.
//! - `trace_summary` — `TraceSummaryRecord`.
//! - `gen_ai_eval_result` — one evaluation result attached to a GenAI span.
//! - `gen_ai_span` — `GenAiSpanRecord` (full OTel GenAI semconv surface).
//!
//! Trace and span ids live in [`crate::vala::ids`] (commit 02 of the
//! trace-primitive plan promotes them from `vala::eval::ids`).

use chrono::{DateTime, Utc};

/// Returns `(end - start)` floored to whole milliseconds.
///
/// Sub-millisecond durations return `0`; OLAP predicates on `duration_ms > 0`
/// will not match sub-millisecond spans.
pub(crate) fn duration_ms_from_timestamps(start: DateTime<Utc>, end: DateTime<Utc>) -> u64 {
    (end - start).num_milliseconds().max(0) as u64
}

pub mod attribute_value;
pub mod attributes;
pub mod gen_ai_eval_result;
pub mod gen_ai_span;
pub mod instrumentation_scope;
pub mod resource;
pub mod span;
pub mod span_event;
pub mod span_link;
pub mod trace_summary;

pub use attribute_value::AttributeValue;
pub use gen_ai_eval_result::GenAiEvalResult;
pub use gen_ai_span::GenAiSpanRecord;
pub use instrumentation_scope::InstrumentationScope;
pub use resource::Resource;
pub use span::{SpanKind, SpanRecord, SpanStatus};
pub use span_event::SpanEvent;
pub use span_link::SpanLink;
pub use trace_summary::TraceSummaryRecord;
// `attributes` is a constants module; consumers use `attributes::FOO`,
// not a glob re-export.

#[cfg(test)]
mod ids_tests {
    use crate::vala::ids::{EntityUid, SpanId, TraceId};

    #[test]
    fn trace_id_from_hex_accepts_canonical() {
        let id = TraceId::from_hex("0123456789abcdef0123456789abcdef").expect("valid hex");
        assert_eq!(id.to_hex(), "0123456789abcdef0123456789abcdef");
    }

    #[test]
    fn trace_id_from_hex_accepts_uppercase() {
        let id =
            TraceId::from_hex("0123456789ABCDEF0123456789ABCDEF").expect("uppercase hex accepted");
        assert_eq!(id.to_hex(), "0123456789abcdef0123456789abcdef");
    }

    #[test]
    fn trace_id_from_hex_rejects_short() {
        assert!(TraceId::from_hex("0123456789abcdef0123456789abcde").is_err());
    }

    #[test]
    fn trace_id_from_hex_rejects_long() {
        assert!(TraceId::from_hex("0123456789abcdef0123456789abcdef0").is_err());
    }

    #[test]
    fn trace_id_from_hex_rejects_non_hex() {
        assert!(TraceId::from_hex("g123456789abcdef0123456789abcdef").is_err());
    }

    #[test]
    fn trace_id_from_hex_rejects_all_zero() {
        assert!(TraceId::from_hex("00000000000000000000000000000000").is_err());
    }

    #[test]
    fn trace_id_from_bytes_accepts_nonzero() {
        let id = TraceId::from_bytes([1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16])
            .expect("nonzero bytes valid");
        assert_eq!(id.to_hex(), "0102030405060708090a0b0c0d0e0f10");
    }

    #[test]
    fn trace_id_from_bytes_rejects_all_zero() {
        assert!(TraceId::from_bytes([0u8; 16]).is_err());
    }

    #[test]
    fn trace_id_zero_const_is_all_zero_bytes() {
        assert_eq!(TraceId::ZERO.as_bytes(), &[0u8; 16]);
    }

    #[test]
    fn trace_id_serializes_as_hex_string() {
        let id = TraceId::from_hex("0123456789abcdef0123456789abcdef").expect("valid trace id");
        let json = serde_json::to_string(&id).expect("trace id serializes");
        assert_eq!(json, "\"0123456789abcdef0123456789abcdef\"");
        let back: TraceId = serde_json::from_str(&json).expect("trace id deserializes");
        assert_eq!(back, id);
    }

    #[test]
    fn trace_id_round_trips_via_byte_array_deserialize_only() {
        let json = "[1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16]";
        let id: TraceId = serde_json::from_str(json).expect("byte array deserializes");
        let reserialized = serde_json::to_string(&id).expect("trace id serializes");
        assert_eq!(reserialized, "\"0102030405060708090a0b0c0d0e0f10\"");
    }

    #[test]
    fn trace_id_deserialize_accepts_hex_string() {
        let json = "\"0123456789abcdef0123456789abcdef\"";
        let id: TraceId = serde_json::from_str(json).expect("hex string deserializes");
        assert_eq!(id.to_hex(), "0123456789abcdef0123456789abcdef");
    }

    #[test]
    fn trace_id_deserialize_hex_string_rejects_all_zero() {
        let result: Result<TraceId, _> =
            serde_json::from_str("\"00000000000000000000000000000000\"");
        assert!(result.is_err());
    }

    #[test]
    fn trace_id_deserialize_byte_array_rejects_all_zero() {
        let result: Result<TraceId, _> = serde_json::from_str("[0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0]");
        assert!(result.is_err());
    }

    #[test]
    fn span_id_from_hex_accepts_canonical() {
        let id = SpanId::from_hex("0123456789abcdef").expect("valid hex");
        assert_eq!(id.to_hex(), "0123456789abcdef");
    }

    #[test]
    fn span_id_from_hex_accepts_uppercase() {
        let id = SpanId::from_hex("0123456789ABCDEF").expect("uppercase hex accepted");
        assert_eq!(id.to_hex(), "0123456789abcdef");
    }

    #[test]
    fn span_id_from_hex_rejects_short() {
        assert!(SpanId::from_hex("0123456789abcde").is_err());
    }

    #[test]
    fn span_id_from_hex_rejects_long() {
        assert!(SpanId::from_hex("0123456789abcdef0").is_err());
    }

    #[test]
    fn span_id_from_hex_rejects_non_hex() {
        assert!(SpanId::from_hex("g123456789abcdef").is_err());
    }

    #[test]
    fn span_id_from_hex_rejects_all_zero() {
        assert!(SpanId::from_hex("0000000000000000").is_err());
    }

    #[test]
    fn span_id_from_bytes_accepts_nonzero() {
        let id = SpanId::from_bytes([1, 2, 3, 4, 5, 6, 7, 8]).expect("nonzero valid");
        assert_eq!(id.to_hex(), "0102030405060708");
    }

    #[test]
    fn span_id_from_bytes_rejects_all_zero() {
        assert!(SpanId::from_bytes([0u8; 8]).is_err());
    }

    #[test]
    fn span_id_zero_const_is_all_zero_bytes() {
        assert_eq!(SpanId::ZERO.as_bytes(), &[0u8; 8]);
    }

    #[test]
    fn span_id_serializes_as_hex_string() {
        let id = SpanId::from_hex("0123456789abcdef").expect("valid span id");
        let json = serde_json::to_string(&id).expect("span id serializes");
        assert_eq!(json, "\"0123456789abcdef\"");
        let back: SpanId = serde_json::from_str(&json).expect("span id deserializes");
        assert_eq!(back, id);
    }

    #[test]
    fn span_id_round_trips_via_byte_array_deserialize_only() {
        let id: SpanId =
            serde_json::from_str("[1,2,3,4,5,6,7,8]").expect("byte array deserializes");
        let reserialized = serde_json::to_string(&id).expect("span id serializes");
        assert_eq!(reserialized, "\"0102030405060708\"");
    }

    #[test]
    fn span_id_deserialize_accepts_hex_string() {
        let id: SpanId = serde_json::from_str("\"0123456789abcdef\"").expect("hex deserializes");
        assert_eq!(id.to_hex(), "0123456789abcdef");
    }

    #[test]
    fn span_id_deserialize_hex_string_rejects_all_zero() {
        let result: Result<SpanId, _> = serde_json::from_str("\"0000000000000000\"");
        assert!(result.is_err());
    }

    #[test]
    fn span_id_deserialize_byte_array_rejects_all_zero() {
        let result: Result<SpanId, _> = serde_json::from_str("[0,0,0,0,0,0,0,0]");
        assert!(result.is_err());
    }

    #[test]
    fn vala_eval_ids_traceid_resolves_to_vala_ids_traceid() {
        let a: crate::vala::eval::ids::TraceId =
            TraceId::from_hex("0123456789abcdef0123456789abcdef").expect("valid trace id");
        let b: crate::vala::ids::TraceId =
            TraceId::from_hex("0123456789abcdef0123456789abcdef").expect("valid trace id");
        assert_eq!(a, b);
    }

    #[test]
    fn vala_eval_ids_dataclass_ids_still_resolve() {
        let _: crate::vala::eval::ids::SessionId =
            crate::vala::eval::ids::SessionId(uuid::Uuid::nil());
        let _: crate::vala::eval::ids::RecordId =
            crate::vala::eval::ids::RecordId(uuid::Uuid::nil());
        let _: crate::vala::eval::ids::WorkflowUid =
            crate::vala::eval::ids::WorkflowUid(uuid::Uuid::nil());
        let _: crate::vala::eval::ids::EntityUid =
            crate::vala::eval::ids::EntityUid::new("u").expect("valid entity uid");
    }

    #[test]
    fn trace_id_display_matches_to_hex() {
        let id = TraceId::from_hex("0123456789abcdef0123456789abcdef").expect("valid trace id");
        assert_eq!(format!("{id}"), "0123456789abcdef0123456789abcdef");
    }

    #[test]
    fn trace_id_from_str_parses_valid_hex() {
        let id: TraceId = "0123456789abcdef0123456789abcdef"
            .parse()
            .expect("valid trace id");
        assert_eq!(id.to_hex(), "0123456789abcdef0123456789abcdef");
    }

    #[test]
    fn trace_id_from_str_rejects_invalid() {
        let result = "not-a-trace-id".parse::<TraceId>();
        assert!(result.is_err());
    }

    #[test]
    fn span_id_display_matches_to_hex() {
        let id = SpanId::from_hex("0123456789abcdef").expect("valid span id");
        assert_eq!(format!("{id}"), "0123456789abcdef");
    }

    #[test]
    fn span_id_from_str_parses_valid_hex() {
        let id: SpanId = "0123456789abcdef".parse().expect("valid span id");
        assert_eq!(id.to_hex(), "0123456789abcdef");
    }

    #[test]
    fn span_id_from_str_rejects_invalid() {
        let result = "not-a-span-id".parse::<SpanId>();
        assert!(result.is_err());
    }

    #[test]
    fn entity_uid_new_accepts_valid() {
        let uid = EntityUid::new("agent-42").expect("valid entity uid");
        assert_eq!(uid.as_str(), "agent-42");
    }

    #[test]
    fn entity_uid_new_rejects_empty() {
        assert!(EntityUid::new("").is_err());
    }

    #[test]
    fn entity_uid_new_rejects_overlong() {
        assert!(EntityUid::new("a".repeat(513)).is_err());
    }

    #[test]
    fn entity_uid_accepts_max_length() {
        EntityUid::new("a".repeat(512)).expect("512 chars is valid");
    }

    #[test]
    fn entity_uid_serde_round_trip() {
        let uid = EntityUid::new("agent-42").expect("valid entity uid");
        let json = serde_json::to_string(&uid).expect("serializes");
        assert_eq!(json, "\"agent-42\"");
        let back: EntityUid = serde_json::from_str(&json).expect("deserializes");
        assert_eq!(uid, back);
    }

    #[test]
    fn entity_uid_deserialize_rejects_empty() {
        let result: Result<EntityUid, _> = serde_json::from_str("\"\"");
        assert!(result.is_err());
    }
}

#[cfg(test)]
mod attributes_tests {
    use crate::vala::trace::attributes::*;

    #[test]
    fn wyrd_namespace_constants_have_wyrd_prefix() {
        for key in WYRD_KEYS {
            assert!(key.starts_with("wyrd."), "{key} does not have wyrd. prefix");
        }
    }

    #[test]
    fn gen_ai_namespace_constants_have_gen_ai_prefix() {
        for key in GEN_AI_KEYS {
            assert!(
                key.starts_with("gen_ai."),
                "{key} does not have gen_ai. prefix"
            );
        }
    }

    #[test]
    fn tag_prefix_ends_with_dot() {
        assert!(TAG_PREFIX.ends_with('.'));
        assert!(TAG_PREFIX.starts_with("wyrd.tracing.tag."));
    }

    #[test]
    fn baggage_prefix_matches_w3c() {
        assert_eq!(BAGGAGE_PREFIX, "baggage.");
    }

    #[test]
    fn gen_ai_evaluation_event_name_is_canonical() {
        assert_eq!(GEN_AI_EVALUATION_EVENT, "gen_ai.evaluation.result");
    }

    #[test]
    fn gen_ai_provider_name_matches_otel_spec() {
        assert_eq!(GEN_AI_PROVIDER_NAME, "gen_ai.provider.name");
    }

    #[test]
    fn gen_ai_system_instructions_uses_underscore_not_dot() {
        assert_eq!(GEN_AI_SYSTEM_INSTRUCTIONS, "gen_ai.system_instructions");
    }

    #[test]
    fn gen_ai_cache_token_keys_use_dot_subnamespace() {
        assert_eq!(
            GEN_AI_USAGE_CACHE_CREATION_INPUT_TOKENS,
            "gen_ai.usage.cache_creation.input_tokens"
        );
        assert_eq!(
            GEN_AI_USAGE_CACHE_READ_INPUT_TOKENS,
            "gen_ai.usage.cache_read.input_tokens"
        );
    }

    #[test]
    fn gen_ai_reasoning_output_tokens_key_matches_otel_spec() {
        assert_eq!(
            GEN_AI_USAGE_REASONING_OUTPUT_TOKENS,
            "gen_ai.usage.reasoning.output_tokens"
        );
    }

    #[test]
    fn server_and_error_keys_have_no_gen_ai_prefix() {
        assert_eq!(SERVER_ADDRESS, "server.address");
        assert_eq!(SERVER_PORT, "server.port");
        assert_eq!(ERROR_TYPE, "error.type");
    }

    #[test]
    fn wyrd_keys_count_locked() {
        assert_eq!(WYRD_KEYS.len(), 9);
    }

    #[test]
    fn gen_ai_keys_count_locked() {
        assert_eq!(GEN_AI_KEYS.len(), 54);
    }

    #[test]
    fn otel_referenced_keys_count_locked() {
        assert_eq!(OTEL_REFERENCED_KEYS.len(), 3);
    }

    #[test]
    fn openai_keys_count_locked() {
        assert_eq!(OPENAI_KEYS.len(), 4);
        for key in OPENAI_KEYS {
            assert!(
                key.starts_with("openai."),
                "{key} not in openai.* namespace"
            );
            assert!(
                !GEN_AI_KEYS.contains(key),
                "{key} must not also be in GEN_AI_KEYS"
            );
        }
    }

    #[test]
    fn mcp_keys_count_locked() {
        assert_eq!(MCP_KEYS.len(), 4);
        for key in MCP_KEYS {
            assert!(key.starts_with("mcp."), "{key} not in mcp.* namespace");
            assert!(
                !GEN_AI_KEYS.contains(key),
                "{key} must not also be in GEN_AI_KEYS"
            );
        }
    }

    #[test]
    fn otel_referenced_keys_do_not_overlap_gen_ai_keys() {
        for key in OTEL_REFERENCED_KEYS {
            assert!(
                !GEN_AI_KEYS.contains(key),
                "{key} is in OTEL_REFERENCED_KEYS; it must NOT also appear in GEN_AI_KEYS"
            );
            assert!(
                !key.starts_with("gen_ai."),
                "{key} is OTEL_REFERENCED_KEYS but starts with gen_ai."
            );
        }
    }
}

#[cfg(test)]
mod attribute_value_tests {
    use std::collections::BTreeMap;

    use crate::vala::trace::AttributeValue;

    #[test]
    fn attribute_value_string_round_trip() {
        let value = AttributeValue::String("hello".into());
        let serialized = serde_json::to_string(&value).unwrap();
        assert_eq!(serialized, "\"hello\"");
        let back: AttributeValue = serde_json::from_str(&serialized).unwrap();
        assert_eq!(back, value);
    }

    #[test]
    fn attribute_value_bool_round_trip() {
        let value = AttributeValue::Bool(true);
        let serialized = serde_json::to_string(&value).unwrap();
        assert_eq!(serialized, "true");
        let back: AttributeValue = serde_json::from_str(&serialized).unwrap();
        assert_eq!(back, value);
    }

    #[test]
    fn attribute_value_int_round_trip() {
        let value = AttributeValue::Int(42);
        let serialized = serde_json::to_string(&value).unwrap();
        assert_eq!(serialized, "42");
        let back: AttributeValue = serde_json::from_str(&serialized).unwrap();
        assert_eq!(back, value);
    }

    #[test]
    fn attribute_value_double_round_trip_finite() {
        let value = AttributeValue::Double(1.5);
        let serialized = serde_json::to_string(&value).unwrap();
        assert_eq!(serialized, "1.5");
        let back: AttributeValue = serde_json::from_str(&serialized).unwrap();
        assert_eq!(back, value);
    }

    #[test]
    fn attribute_value_bytes_serializes_as_base64() {
        let value = AttributeValue::Bytes(vec![0, 1, 2, 3, 4]);
        let serialized = serde_json::to_string(&value).unwrap();
        assert_eq!(serialized, "\"AAECAwQ=\"");
    }

    #[test]
    fn attribute_value_bytes_does_not_round_trip_through_serde() {
        let value = AttributeValue::Bytes(vec![0, 1, 2, 3, 4]);
        let serialized = serde_json::to_string(&value).unwrap();
        let back: AttributeValue = serde_json::from_str(&serialized).unwrap();
        assert_eq!(back, AttributeValue::String("AAECAwQ=".to_string()));
        assert_ne!(back, value);
    }

    #[test]
    fn attribute_value_bytes_does_not_round_trip_through_value() {
        let value = AttributeValue::Bytes(vec![0, 1, 2, 3, 4]);
        let json = value.to_json();
        let back = AttributeValue::from_json(&json);
        assert_eq!(back, AttributeValue::String("AAECAwQ=".to_string()));
        assert_ne!(back, value);
    }

    #[test]
    fn attribute_value_array_round_trip() {
        let value = AttributeValue::Array(vec![
            AttributeValue::Int(1),
            AttributeValue::String("x".into()),
        ]);
        let serialized = serde_json::to_string(&value).unwrap();
        let back: AttributeValue = serde_json::from_str(&serialized).unwrap();
        assert_eq!(back, value);
    }

    #[test]
    fn attribute_value_kvlist_round_trip_ordered() {
        let mut map = BTreeMap::new();
        map.insert("b".to_string(), AttributeValue::Int(2));
        map.insert("a".to_string(), AttributeValue::Int(1));
        let value = AttributeValue::KvList(map);
        let serialized = serde_json::to_string(&value).unwrap();
        assert_eq!(serialized, r#"{"a":1,"b":2}"#);
        let back: AttributeValue = serde_json::from_str(&serialized).unwrap();
        assert_eq!(back, value);
    }

    #[test]
    fn attribute_value_validate_rejects_nan() {
        let value = AttributeValue::Double(f64::NAN);
        assert!(value.validate().is_err());
    }

    #[test]
    fn attribute_value_validate_rejects_pos_inf() {
        let value = AttributeValue::Double(f64::INFINITY);
        assert!(value.validate().is_err());
    }

    #[test]
    fn attribute_value_validate_recurses_into_array() {
        let value = AttributeValue::Array(vec![AttributeValue::Double(f64::NAN)]);
        assert!(value.validate().is_err());
    }

    #[test]
    fn attribute_value_validate_recurses_into_kvlist() {
        let mut map = BTreeMap::new();
        map.insert("k".to_string(), AttributeValue::Double(f64::NAN));
        let value = AttributeValue::KvList(map);
        assert!(value.validate().is_err());
    }

    #[test]
    fn attribute_value_validate_accepts_zero() {
        AttributeValue::Double(0.0).validate().unwrap();
        AttributeValue::Double(-0.0).validate().unwrap();
    }

    #[test]
    fn attribute_value_to_json_round_trip_via_from_json() {
        let cases = vec![
            AttributeValue::String("hi".into()),
            AttributeValue::Bool(false),
            AttributeValue::Int(-7),
            AttributeValue::Double(std::f64::consts::PI),
            AttributeValue::Array(vec![AttributeValue::Int(1)]),
        ];

        for value in cases {
            let json = value.to_json();
            let back = AttributeValue::from_json(&json);
            assert_eq!(back, value, "round-trip via JSON failed for {value:?}");
        }
    }

    #[test]
    fn attribute_value_from_json_nullmap_to_empty_string() {
        let value = AttributeValue::from_json(&serde_json::Value::Null);
        assert_eq!(value, AttributeValue::String(String::new()));
    }

    #[test]
    fn attribute_value_to_json_nan_becomes_null() {
        let value = AttributeValue::Double(f64::NAN);
        assert_eq!(value.to_json(), serde_json::Value::Null);
    }

    #[test]
    fn attribute_value_variant_count_matches_otel_anyvalue() {
        use std::mem::discriminant;

        let values = [
            AttributeValue::String(String::new()),
            AttributeValue::Bool(false),
            AttributeValue::Int(0),
            AttributeValue::Double(0.0),
            AttributeValue::Bytes(vec![]),
            AttributeValue::Array(vec![]),
            AttributeValue::KvList(Default::default()),
        ];
        let mut seen = Vec::new();
        for value in &values {
            let discriminant = discriminant(value);
            assert!(
                !seen.contains(&discriminant),
                "duplicate discriminant for {value:?}"
            );
            seen.push(discriminant);
        }
        assert_eq!(seen.len(), 7);
    }
}

#[cfg(test)]
mod span_event_tests {
    use chrono::{TimeZone, Utc};
    use serde_json::json;

    use crate::vala::trace::SpanEvent;

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
}

#[cfg(test)]
mod span_link_tests {
    use serde_json::json;

    use crate::vala::ids::{SpanId, TraceId};
    use crate::vala::trace::SpanLink;

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
}

#[cfg(test)]
mod resource_tests {
    use serde_json::json;

    use crate::vala::trace::Resource;

    fn full_resource() -> Resource {
        let mut attrs = serde_json::Map::new();
        attrs.insert("telemetry.sdk.name".into(), json!("wyrd-tracing"));
        attrs.insert("host.name".into(), json!("worker-0"));
        attrs.insert("process.pid".into(), json!(42_000));
        Resource {
            service_name: "checkout".into(),
            service_namespace: Some("payments".into()),
            service_version: Some("1.2.3".into()),
            service_instance_id: Some("worker-0:7fda".into()),
            attributes: attrs,
        }
    }

    #[test]
    fn resource_round_trip_full() {
        let r = full_resource();
        let s = serde_json::to_string(&r).unwrap();
        let back: Resource = serde_json::from_str(&s).unwrap();
        assert_eq!(back, r);
    }

    #[test]
    fn resource_round_trip_minimal() {
        let r = Resource {
            service_name: "checkout".into(),
            service_namespace: None,
            service_version: None,
            service_instance_id: None,
            attributes: serde_json::Map::new(),
        };
        let s = serde_json::to_string(&r).unwrap();
        assert!(!s.contains("service_namespace"));
        assert!(!s.contains("service_version"));
        assert!(!s.contains("service_instance_id"));
        let back: Resource = serde_json::from_str(&s).unwrap();
        assert_eq!(back, r);
    }

    #[test]
    fn resource_attributes_defaults_to_empty() {
        let s = r#"{"service_name": "checkout"}"#;
        let r: Resource = serde_json::from_str(s).unwrap();
        assert!(r.attributes.is_empty());
    }

    #[test]
    fn resource_attributes_preserve_nested_json() {
        let mut attrs = serde_json::Map::new();
        attrs.insert("custom.array".into(), json!([1, 2, 3]));
        attrs.insert("custom.object".into(), json!({"k": "v"}));
        let r = Resource {
            service_name: "x".into(),
            service_namespace: None,
            service_version: None,
            service_instance_id: None,
            attributes: attrs,
        };
        let s = serde_json::to_string(&r).unwrap();
        let back: Resource = serde_json::from_str(&s).unwrap();
        assert_eq!(back, r);
    }

    #[test]
    fn resource_deny_unknown_fields() {
        let s = r#"{"service_name": "x", "rogue": true}"#;
        let r: Result<Resource, _> = serde_json::from_str(s);
        assert!(r.is_err());
    }

    #[test]
    fn resource_validate_rejects_empty_service_name() {
        let r = Resource {
            service_name: String::new(),
            service_namespace: None,
            service_version: None,
            service_instance_id: None,
            attributes: serde_json::Map::new(),
        };
        assert!(r.validate().is_err());
    }

    #[test]
    fn resource_validate_rejects_overlong_service_name() {
        let r = Resource {
            service_name: "a".repeat(257),
            service_namespace: None,
            service_version: None,
            service_instance_id: None,
            attributes: serde_json::Map::new(),
        };
        assert!(r.validate().is_err());
    }

    #[test]
    fn resource_validate_rejects_overlong_optional() {
        let r = Resource {
            service_name: "x".into(),
            service_namespace: Some("a".repeat(257)),
            service_version: None,
            service_instance_id: None,
            attributes: serde_json::Map::new(),
        };
        assert!(r.validate().is_err());
    }

    #[test]
    fn resource_validate_rejects_overlarge_attributes() {
        let mut attrs = serde_json::Map::new();
        for i in 0..257 {
            attrs.insert(format!("k{i}"), json!(i));
        }
        let r = Resource {
            service_name: "x".into(),
            service_namespace: None,
            service_version: None,
            service_instance_id: None,
            attributes: attrs,
        };
        assert!(r.validate().is_err());
    }

    #[test]
    fn resource_validate_happy_path() {
        full_resource().validate().unwrap();
    }
}

#[cfg(test)]
mod instrumentation_scope_tests {
    use serde_json::json;

    use crate::vala::trace::InstrumentationScope;

    fn scope() -> InstrumentationScope {
        InstrumentationScope {
            name: "wyrd-tracing".into(),
            version: Some("0.3.1".into()),
            attributes: serde_json::Map::new(),
        }
    }

    #[test]
    fn instrumentation_scope_round_trip() {
        let s = scope();
        let j = serde_json::to_string(&s).unwrap();
        let back: InstrumentationScope = serde_json::from_str(&j).unwrap();
        assert_eq!(back, s);
    }

    #[test]
    fn instrumentation_scope_round_trip_no_version() {
        let s = InstrumentationScope {
            name: "wyrd-tracing".into(),
            version: None,
            attributes: serde_json::Map::new(),
        };
        let j = serde_json::to_string(&s).unwrap();
        assert!(!j.contains("version"));
        let back: InstrumentationScope = serde_json::from_str(&j).unwrap();
        assert_eq!(back, s);
    }

    #[test]
    fn instrumentation_scope_attributes_default_empty() {
        let s = r#"{"name": "lib"}"#;
        let scope: InstrumentationScope = serde_json::from_str(s).unwrap();
        assert!(scope.attributes.is_empty());
    }

    #[test]
    fn instrumentation_scope_round_trip_with_attributes() {
        let mut attrs = serde_json::Map::new();
        attrs.insert("ext.runtime".into(), json!("rust"));
        let s = InstrumentationScope {
            name: "wyrd-tracing".into(),
            version: Some("0.3.1".into()),
            attributes: attrs,
        };
        let j = serde_json::to_string(&s).unwrap();
        let back: InstrumentationScope = serde_json::from_str(&j).unwrap();
        assert_eq!(back, s);
    }

    #[test]
    fn instrumentation_scope_deny_unknown_fields() {
        let s = r#"{"name": "lib", "rogue": true}"#;
        let r: Result<InstrumentationScope, _> = serde_json::from_str(s);
        assert!(r.is_err());
    }

    #[test]
    fn instrumentation_scope_validate_rejects_empty_name() {
        let s = InstrumentationScope {
            name: String::new(),
            version: None,
            attributes: serde_json::Map::new(),
        };
        assert!(s.validate().is_err());
    }

    #[test]
    fn instrumentation_scope_validate_rejects_overlong_name() {
        let s = InstrumentationScope {
            name: "a".repeat(257),
            version: None,
            attributes: serde_json::Map::new(),
        };
        assert!(s.validate().is_err());
    }

    #[test]
    fn instrumentation_scope_validate_rejects_overlong_version() {
        let s = InstrumentationScope {
            name: "lib".into(),
            version: Some("v".repeat(65)),
            attributes: serde_json::Map::new(),
        };
        assert!(s.validate().is_err());
    }

    #[test]
    fn instrumentation_scope_validate_rejects_overlarge_attributes() {
        let mut attrs = serde_json::Map::new();
        for i in 0..33 {
            attrs.insert(format!("k{i}"), json!(i));
        }
        let s = InstrumentationScope {
            name: "lib".into(),
            version: None,
            attributes: attrs,
        };
        assert!(s.validate().is_err());
    }

    #[test]
    fn instrumentation_scope_validate_happy_path() {
        scope().validate().unwrap();
    }
}

#[cfg(test)]
mod span_record_tests {
    use chrono::{Duration, TimeZone, Utc};
    use serde_json::json;

    use crate::vala::ids::{SpanId, TraceId};
    use crate::vala::trace::{
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
}

#[cfg(test)]
mod trace_summary_record_tests {
    use chrono::{DateTime, Duration, TimeZone, Timelike, Utc};

    use crate::DataTenantId;
    use crate::vala::ids::{SpanId, TraceId};
    use crate::vala::trace::{
        InstrumentationScope, Resource, SpanKind, SpanStatus, TraceSummaryRecord,
    };

    /// Floor `t` to the start of the minute it falls in.
    fn floor_to_minute(t: DateTime<Utc>) -> DateTime<Utc> {
        t.with_second(0).unwrap().with_nanosecond(0).unwrap()
    }

    fn summary() -> TraceSummaryRecord {
        let start = Utc.timestamp_opt(1_700_000_030, 0).unwrap();
        let end = start + Duration::milliseconds(2_500);
        TraceSummaryRecord {
            trace_id: TraceId::from_hex("0123456789abcdef0123456789abcdef").unwrap(),
            data_tenant_id: DataTenantId::new_v7(),
            bucket_time: floor_to_minute(start),
            start_time: start,
            end_time: end,
            duration_ms: 2_500,
            span_count: 8,
            error_count: 1,
            root_span_id: Some(SpanId::from_hex("0123456789abcdef").unwrap()),
            root_name: Some("GET /checkout".into()),
            root_kind: Some(SpanKind::Server),
            root_status: SpanStatus::Error {
                description: Some("upstream timeout".into()),
            },
            root_scope: Some(InstrumentationScope {
                name: "wyrd-tracing".into(),
                version: Some("0.3.1".into()),
                attributes: serde_json::Map::new(),
            }),
            resource: Resource {
                service_name: "checkout".into(),
                service_namespace: Some("payments".into()),
                service_version: Some("1.2.3".into()),
                service_instance_id: Some("worker-0".into()),
                attributes: serde_json::Map::new(),
            },
        }
    }

    #[test]
    fn trace_summary_record_round_trip_full() {
        let s = summary();
        let j = serde_json::to_string(&s).unwrap();
        let back: TraceSummaryRecord = serde_json::from_str(&j).unwrap();
        assert_eq!(back, s);
    }

    #[test]
    fn trace_summary_record_round_trip_anonymous() {
        let mut s = summary();
        s.root_span_id = None;
        s.root_name = None;
        s.root_kind = None;
        s.root_status = SpanStatus::Unset;
        s.root_scope = None;
        let j = serde_json::to_string(&s).unwrap();
        assert!(!j.contains("root_span_id"));
        assert!(!j.contains("root_name"));
        assert!(!j.contains("root_kind"));
        assert!(!j.contains("root_scope"));
        let back: TraceSummaryRecord = serde_json::from_str(&j).unwrap();
        assert_eq!(back, s);
    }

    #[test]
    fn trace_summary_record_deny_unknown_fields() {
        let mut obj = serde_json::to_value(summary())
            .unwrap()
            .as_object()
            .unwrap()
            .clone();
        obj.insert("rogue".into(), serde_json::json!(true));
        let s = serde_json::to_string(&obj).unwrap();
        let r: Result<TraceSummaryRecord, _> = serde_json::from_str(&s);
        assert!(r.is_err());
    }

    #[test]
    fn trace_summary_record_validate_happy_path() {
        summary().validate().unwrap();
    }

    #[test]
    fn trace_summary_record_validate_rejects_zero_span_count() {
        let mut s = summary();
        s.span_count = 0;
        s.error_count = 0;
        assert!(s.validate().is_err());
    }

    #[test]
    fn trace_summary_record_validate_rejects_error_count_exceeding_span_count() {
        let mut s = summary();
        s.span_count = 3;
        s.error_count = 4;
        assert!(s.validate().is_err());
    }

    #[test]
    fn trace_summary_record_validate_accepts_error_count_equal_to_span_count() {
        let mut s = summary();
        s.span_count = 3;
        s.error_count = 3;
        s.validate().unwrap();
    }

    #[test]
    fn trace_summary_record_validate_rejects_end_before_start() {
        let mut s = summary();
        s.end_time = s.start_time - Duration::seconds(1);
        s.duration_ms = 0;
        assert!(s.validate().is_err());
    }

    #[test]
    fn trace_summary_record_validate_rejects_mismatched_duration() {
        let mut s = summary();
        s.duration_ms = 999;
        assert!(s.validate().is_err());
    }

    #[test]
    fn trace_summary_record_validate_rejects_bucket_after_start() {
        let mut s = summary();
        s.bucket_time = s.start_time + Duration::seconds(1);
        assert!(s.validate().is_err());
    }

    #[test]
    fn trace_summary_record_validate_accepts_bucket_equal_to_start() {
        let mut s = summary();
        s.bucket_time = s.start_time;
        s.validate().unwrap();
    }

    #[test]
    fn trace_summary_record_validate_propagates_resource_failure() {
        let mut s = summary();
        s.resource.service_name = String::new();
        assert!(s.validate().is_err());
    }

    #[test]
    fn trace_summary_record_validate_propagates_root_scope_failure() {
        let mut s = summary();
        s.root_scope = Some(InstrumentationScope {
            name: String::new(),
            version: None,
            attributes: serde_json::Map::new(),
        });
        assert!(s.validate().is_err());
    }

    #[test]
    fn trace_summary_record_validate_rejects_empty_root_name() {
        let mut s = summary();
        s.root_name = Some(String::new());
        assert!(s.validate().is_err());
    }

    #[test]
    fn trace_summary_record_validate_rejects_overlong_root_name() {
        let mut s = summary();
        s.root_name = Some("a".repeat(257));
        assert!(s.validate().is_err());
    }

    #[test]
    fn trace_summary_record_validate_accepts_unset_root_status() {
        let mut s = summary();
        s.root_span_id = None;
        s.root_name = None;
        s.root_kind = None;
        s.root_status = SpanStatus::Unset;
        s.root_scope = None;
        s.validate().unwrap();
    }

    #[test]
    fn trace_summary_record_bucket_time_at_minute_boundary_round_trips() {
        let t = Utc.timestamp_opt(1_700_000_040, 0).unwrap();
        let floored = floor_to_minute(t);
        assert_eq!(floored, t);
    }

    #[test]
    fn trace_summary_record_bucket_time_below_start_within_minute() {
        let s = summary();
        let diff = s.start_time - s.bucket_time;
        assert!(diff <= Duration::seconds(60));
    }
}

#[cfg(test)]
mod gen_ai_eval_result_tests {
    use crate::vala::trace::GenAiEvalResult;

    fn eval() -> GenAiEvalResult {
        GenAiEvalResult {
            name: "factuality".into(),
            score_label: Some("pass".into()),
            score_value: Some(0.92),
            explanation: Some("matches grounding documents".into()),
            response_id: Some("resp_abc123".into()),
        }
    }

    #[test]
    fn gen_ai_eval_result_round_trip_full() {
        let result = eval();
        let json = serde_json::to_string(&result).unwrap();
        let back: GenAiEvalResult = serde_json::from_str(&json).unwrap();
        assert_eq!(back, result);
    }

    #[test]
    fn gen_ai_eval_result_round_trip_minimal() {
        let result = GenAiEvalResult {
            name: "safety".into(),
            score_label: None,
            score_value: None,
            explanation: None,
            response_id: None,
        };
        let json = serde_json::to_string(&result).unwrap();
        assert_eq!(json, r#"{"name":"safety"}"#);
        let back: GenAiEvalResult = serde_json::from_str(&json).unwrap();
        assert_eq!(back, result);
    }

    #[test]
    fn gen_ai_eval_result_deny_unknown_fields() {
        let serialized = r#"{"name":"x","rogue":true}"#;
        let result: Result<GenAiEvalResult, _> = serde_json::from_str(serialized);
        assert!(result.is_err());
    }

    #[test]
    fn gen_ai_eval_result_validate_happy_path() {
        eval().validate().unwrap();
    }

    #[test]
    fn gen_ai_eval_result_validate_rejects_empty_name() {
        let result = GenAiEvalResult {
            name: String::new(),
            score_label: None,
            score_value: None,
            explanation: None,
            response_id: None,
        };
        assert!(result.validate().is_err());
    }

    #[test]
    fn gen_ai_eval_result_validate_rejects_overlong_name() {
        let mut result = eval();
        result.name = "a".repeat(257);
        assert!(result.validate().is_err());
    }

    #[test]
    fn gen_ai_eval_result_validate_rejects_nan_score() {
        let mut result = eval();
        result.score_value = Some(f64::NAN);
        assert!(result.validate().is_err());
    }

    #[test]
    fn gen_ai_eval_result_validate_rejects_pos_inf_score() {
        let mut result = eval();
        result.score_value = Some(f64::INFINITY);
        assert!(result.validate().is_err());
    }

    #[test]
    fn gen_ai_eval_result_validate_rejects_neg_inf_score() {
        let mut result = eval();
        result.score_value = Some(f64::NEG_INFINITY);
        assert!(result.validate().is_err());
    }

    #[test]
    fn gen_ai_eval_result_validate_accepts_negative_score() {
        let mut result = eval();
        result.score_value = Some(-3.5);
        result.validate().unwrap();
    }

    #[test]
    fn gen_ai_eval_result_validate_rejects_overlong_score_label() {
        let mut result = eval();
        result.score_label = Some("a".repeat(65));
        assert!(result.validate().is_err());
    }

    #[test]
    fn gen_ai_eval_result_validate_rejects_overlong_explanation() {
        let mut result = eval();
        result.explanation = Some("a".repeat(4097));
        assert!(result.validate().is_err());
    }

    #[test]
    fn gen_ai_eval_result_validate_accepts_4096_char_explanation() {
        let mut result = eval();
        result.explanation = Some("a".repeat(4096));
        result.validate().unwrap();
    }

    #[test]
    fn gen_ai_eval_result_validate_rejects_overlong_response_id() {
        let mut result = eval();
        result.response_id = Some("a".repeat(129));
        assert!(result.validate().is_err());
    }
}

#[cfg(test)]
mod gen_ai_span_record_tests {
    use chrono::{Duration, TimeZone, Utc};
    use serde_json::json;

    use crate::vala::ids::{SpanId, TraceId};
    use crate::vala::trace::{GenAiEvalResult, GenAiSpanRecord, Resource, SpanStatus};

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
            retrieval_top_k: None,
            request_reasoning_level: None,
            conversation_compacted: None,
            prompt_version: None,
            memory_store_id: None,
            memory_record_id: None,
            memory_record_count: None,
            memory_query_text: None,
            memory_records: None,
            openai_api_type: None,
            openai_request_service_tier: None,
            openai_response_service_tier: None,
            openai_response_system_fingerprint: None,
            mcp_session_id: None,
            mcp_method_name: None,
            mcp_protocol_version: None,
            mcp_resource_uri: None,
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
            retrieval_top_k: None,
            request_reasoning_level: None,
            conversation_compacted: None,
            prompt_version: None,
            memory_store_id: None,
            memory_record_id: None,
            memory_record_count: None,
            memory_query_text: None,
            memory_records: None,
            openai_api_type: None,
            openai_request_service_tier: None,
            openai_response_service_tier: None,
            openai_response_system_fingerprint: None,
            mcp_session_id: None,
            mcp_method_name: None,
            mcp_protocol_version: None,
            mcp_resource_uri: None,
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
}

#[cfg(test)]
mod key_array_sync_tests {
    //! Sync tests for the attribute-key catalogs.

    use crate::vala::trace::attributes::*;

    #[test]
    fn wyrd_keys_in_sync_with_constants() {
        let declared = [
            FUNCTION_TYPE,
            FUNCTION_NAME,
            TRACING_INPUT,
            TRACING_OUTPUT,
            TRACING_LABEL,
            EVAL_RECORD_UID,
            EVAL_PROFILE_UID,
            SERVICE_CARD_UID,
            DATA_TENANT_ID,
        ];

        for key in &declared {
            assert!(
                WYRD_KEYS.contains(key),
                "WYRD_KEYS missing {key}; add it to attributes.rs WYRD_KEYS array"
            );
        }
        assert_eq!(
            WYRD_KEYS.len(),
            declared.len(),
            "WYRD_KEYS has {} entries but {} constants are declared above",
            WYRD_KEYS.len(),
            declared.len()
        );
    }

    #[test]
    fn gen_ai_keys_in_sync_with_constants() {
        let declared = [
            GEN_AI_PROVIDER_NAME,
            GEN_AI_OPERATION_NAME,
            GEN_AI_OUTPUT_TYPE,
            GEN_AI_CONVERSATION_ID,
            GEN_AI_REQUEST_MODEL,
            GEN_AI_REQUEST_TEMPERATURE,
            GEN_AI_REQUEST_TOP_P,
            GEN_AI_REQUEST_TOP_K,
            GEN_AI_REQUEST_MAX_TOKENS,
            GEN_AI_REQUEST_FREQUENCY_PENALTY,
            GEN_AI_REQUEST_PRESENCE_PENALTY,
            GEN_AI_REQUEST_SEED,
            GEN_AI_REQUEST_CHOICE_COUNT,
            GEN_AI_REQUEST_STOP_SEQUENCES,
            GEN_AI_REQUEST_STREAM,
            GEN_AI_REQUEST_ENCODING_FORMATS,
            GEN_AI_RESPONSE_MODEL,
            GEN_AI_RESPONSE_ID,
            GEN_AI_RESPONSE_FINISH_REASONS,
            GEN_AI_RESPONSE_TIME_TO_FIRST_CHUNK,
            GEN_AI_USAGE_INPUT_TOKENS,
            GEN_AI_USAGE_OUTPUT_TOKENS,
            GEN_AI_USAGE_CACHE_CREATION_INPUT_TOKENS,
            GEN_AI_USAGE_CACHE_READ_INPUT_TOKENS,
            GEN_AI_USAGE_REASONING_OUTPUT_TOKENS,
            GEN_AI_TOOL_CALL_ID,
            GEN_AI_TOOL_NAME,
            GEN_AI_TOOL_TYPE,
            GEN_AI_TOOL_DESCRIPTION,
            GEN_AI_TOOL_DEFINITIONS,
            GEN_AI_TOOL_CALL_ARGUMENTS,
            GEN_AI_TOOL_CALL_RESULT,
            GEN_AI_AGENT_NAME,
            GEN_AI_AGENT_ID,
            GEN_AI_AGENT_DESCRIPTION,
            GEN_AI_AGENT_VERSION,
            GEN_AI_PROMPT_NAME,
            GEN_AI_WORKFLOW_NAME,
            GEN_AI_DATA_SOURCE_ID,
            GEN_AI_EMBEDDINGS_DIMENSION_COUNT,
            GEN_AI_INPUT_MESSAGES,
            GEN_AI_OUTPUT_MESSAGES,
            GEN_AI_SYSTEM_INSTRUCTIONS,
            GEN_AI_RETRIEVAL_DOCUMENTS,
            GEN_AI_RETRIEVAL_QUERY_TEXT,
            GEN_AI_RETRIEVAL_TOP_K,
            GEN_AI_REQUEST_REASONING_LEVEL,
            GEN_AI_CONVERSATION_COMPACTED,
            GEN_AI_PROMPT_VERSION,
            GEN_AI_MEMORY_STORE_ID,
            GEN_AI_MEMORY_RECORD_ID,
            GEN_AI_MEMORY_RECORD_COUNT,
            GEN_AI_MEMORY_QUERY_TEXT,
            GEN_AI_MEMORY_RECORDS,
        ];

        for key in &declared {
            assert!(
                GEN_AI_KEYS.contains(key),
                "GEN_AI_KEYS missing {key}; add it to attributes.rs GEN_AI_KEYS array"
            );
        }
        assert_eq!(
            GEN_AI_KEYS.len(),
            declared.len(),
            "GEN_AI_KEYS has {} entries but {} typed-column constants are declared above. \
             If you added a typed column to GenAiSpanRecord, also add the constant to \
             GEN_AI_KEYS.",
            GEN_AI_KEYS.len(),
            declared.len()
        );
    }

    #[test]
    fn otel_referenced_keys_in_sync() {
        let declared = [SERVER_ADDRESS, SERVER_PORT, ERROR_TYPE];

        for key in &declared {
            assert!(
                OTEL_REFERENCED_KEYS.contains(key),
                "OTEL_REFERENCED_KEYS missing {key}"
            );
        }
        assert_eq!(OTEL_REFERENCED_KEYS.len(), declared.len());
    }

    #[test]
    fn gen_ai_key_exact_strings_match_otel_spec() {
        assert_eq!(GEN_AI_PROVIDER_NAME, "gen_ai.provider.name");
        assert_eq!(GEN_AI_OPERATION_NAME, "gen_ai.operation.name");
        assert_eq!(GEN_AI_OUTPUT_TYPE, "gen_ai.output.type");
        assert_eq!(GEN_AI_CONVERSATION_ID, "gen_ai.conversation.id");
        assert_eq!(GEN_AI_SYSTEM_INSTRUCTIONS, "gen_ai.system_instructions");
        assert_eq!(
            GEN_AI_USAGE_CACHE_CREATION_INPUT_TOKENS,
            "gen_ai.usage.cache_creation.input_tokens"
        );
        assert_eq!(
            GEN_AI_USAGE_CACHE_READ_INPUT_TOKENS,
            "gen_ai.usage.cache_read.input_tokens"
        );
        assert_eq!(
            GEN_AI_USAGE_REASONING_OUTPUT_TOKENS,
            "gen_ai.usage.reasoning.output_tokens"
        );
        assert_eq!(GEN_AI_TOOL_DESCRIPTION, "gen_ai.tool.description");
        assert_eq!(
            GEN_AI_RESPONSE_TIME_TO_FIRST_CHUNK,
            "gen_ai.response.time_to_first_chunk"
        );
        assert_eq!(SERVER_ADDRESS, "server.address");
        assert_eq!(SERVER_PORT, "server.port");
        assert_eq!(ERROR_TYPE, "error.type");
    }

    #[test]
    fn evaluation_event_constants_are_not_in_gen_ai_keys() {
        for key in [
            GEN_AI_EVALUATION_NAME,
            GEN_AI_EVALUATION_SCORE_LABEL,
            GEN_AI_EVALUATION_SCORE_VALUE,
            GEN_AI_EVALUATION_EXPLANATION,
        ] {
            assert!(
                !GEN_AI_KEYS.contains(&key),
                "{key} should NOT be in GEN_AI_KEYS (event attribute, not span attribute)"
            );
        }
    }
}

#[cfg(test)]
mod otel_proto_parity_tests {
    //! Lock the OTel-canonical trace surface against accidental drift.

    use chrono::{Duration, TimeZone, Timelike, Utc};
    use schemars::schema_for;
    use serde_json::Value;

    use crate::DataTenantId;
    use crate::vala::ids::{SpanId, TraceId};
    use crate::vala::trace::{
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
        let schema =
            serde_json::to_value(schema_for!(SpanStatus)).expect("schema serializes to JSON");
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
            trace_id: TraceId::from_hex("0123456789abcdef0123456789abcdef")
                .expect("trace id is valid"),
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
            trace_id: TraceId::from_hex("0123456789abcdef0123456789abcdef")
                .expect("valid trace id"),
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
            trace_id: TraceId::from_hex("0123456789abcdef0123456789abcdef")
                .expect("valid trace id"),
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
            retrieval_top_k: None,
            request_reasoning_level: None,
            conversation_compacted: None,
            prompt_version: None,
            memory_store_id: None,
            memory_record_id: None,
            memory_record_count: None,
            memory_query_text: None,
            memory_records: None,
            openai_api_type: None,
            openai_request_service_tier: None,
            openai_response_service_tier: None,
            openai_response_system_fingerprint: None,
            mcp_session_id: None,
            mcp_method_name: None,
            mcp_protocol_version: None,
            mcp_resource_uri: None,
            eval_results: vec![],
            extra: serde_json::Map::new(),
        };

        let j = serde_json::to_value(&record).expect("serializes");
        let obj = j.as_object().expect("is an object");

        assert!(obj.contains_key("trace_id"), "missing trace_id");
        assert!(obj.contains_key("span_id"), "missing span_id");
        assert!(
            !obj.contains_key("data_tenant_id"),
            "data_tenant_id must not be a GenAiSpanRecord field (C-02)"
        );
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
}

#[cfg(test)]
mod schema_drift_tests {
    //! Golden-file schema drift gate for trace fixtures.

    use std::env;
    use std::fs;
    use std::path::PathBuf;

    use pretty_assertions::assert_eq;
    use schemars::schema_for;

    use crate::vala::trace::{
        AttributeValue, GenAiEvalResult, GenAiSpanRecord, InstrumentationScope, Resource,
        SpanEvent, SpanKind, SpanLink, SpanRecord, SpanStatus, TraceSummaryRecord,
    };

    fn fixture_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/trace/schemas")
    }

    fn assert_schema_matches<T: schemars::JsonSchema>(name: &str) {
        let mut schema = schema_for!(T);
        schema.meta_schema = Some("https://json-schema.org/draft/2020-12/schema".to_string());
        let actual = format!(
            "{}\n",
            serde_json::to_string_pretty(&schema).expect("schema serializes")
        );
        let path = fixture_dir().join(format!("{name}.schema.json"));

        if env::var_os("WYRD_SCHEMA_BLESS").is_some() {
            fs::create_dir_all(fixture_dir()).expect("trace schema fixture directory is created");
            fs::write(&path, &actual).expect("trace schema fixture is written");
            return;
        }

        let expected = fs::read_to_string(&path).unwrap_or_else(|error| {
            panic!(
                "missing golden schema at {path:?}: {error}; regenerate with \
                 `WYRD_SCHEMA_BLESS=1 cargo test -p wyrd-spec --all-features --test trace schema_drift`",
            )
        });
        assert_eq!(
            actual, expected,
            "schema drift in {name}.schema.json; run the trace schema bless command"
        );
    }

    #[test]
    fn span_record_schema() {
        assert_schema_matches::<SpanRecord>("span_record");
    }

    #[test]
    fn span_kind_schema() {
        assert_schema_matches::<SpanKind>("span_kind");
    }

    #[test]
    fn span_status_schema() {
        assert_schema_matches::<SpanStatus>("span_status");
    }

    #[test]
    fn span_event_schema() {
        assert_schema_matches::<SpanEvent>("span_event");
    }

    #[test]
    fn span_link_schema() {
        assert_schema_matches::<SpanLink>("span_link");
    }

    #[test]
    fn resource_schema() {
        assert_schema_matches::<Resource>("resource");
    }

    #[test]
    fn instrumentation_scope_schema() {
        assert_schema_matches::<InstrumentationScope>("instrumentation_scope");
    }

    #[test]
    fn trace_summary_record_schema() {
        assert_schema_matches::<TraceSummaryRecord>("trace_summary_record");
    }

    #[test]
    fn gen_ai_span_record_schema() {
        assert_schema_matches::<GenAiSpanRecord>("gen_ai_span_record");
    }

    #[test]
    fn gen_ai_eval_result_schema() {
        assert_schema_matches::<GenAiEvalResult>("gen_ai_eval_result");
    }

    #[test]
    fn attribute_value_schema() {
        assert_schema_matches::<AttributeValue>("attribute_value");
    }
}
