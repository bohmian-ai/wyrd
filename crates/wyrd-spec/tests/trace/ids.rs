use wyrd_spec::vala::ids::{EntityUid, SpanId, TraceId};

#[test]
fn trace_id_from_hex_accepts_canonical() {
    let id = TraceId::from_hex("0123456789abcdef0123456789abcdef").expect("valid hex");
    assert_eq!(id.to_hex(), "0123456789abcdef0123456789abcdef");
}

#[test]
fn trace_id_from_hex_accepts_uppercase() {
    let id = TraceId::from_hex("0123456789ABCDEF0123456789ABCDEF").expect("uppercase hex accepted");
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
    let result: Result<TraceId, _> = serde_json::from_str("\"00000000000000000000000000000000\"");
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
    let id: SpanId = serde_json::from_str("[1,2,3,4,5,6,7,8]").expect("byte array deserializes");
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
    let a: wyrd_spec::vala::eval::ids::TraceId =
        TraceId::from_hex("0123456789abcdef0123456789abcdef").expect("valid trace id");
    let b: wyrd_spec::vala::ids::TraceId =
        TraceId::from_hex("0123456789abcdef0123456789abcdef").expect("valid trace id");
    assert_eq!(a, b);
}

#[test]
fn vala_eval_ids_dataclass_ids_still_resolve() {
    let _: wyrd_spec::vala::eval::ids::SessionId =
        wyrd_spec::vala::eval::ids::SessionId(uuid::Uuid::nil());
    let _: wyrd_spec::vala::eval::ids::RecordId =
        wyrd_spec::vala::eval::ids::RecordId(uuid::Uuid::nil());
    let _: wyrd_spec::vala::eval::ids::WorkflowUid =
        wyrd_spec::vala::eval::ids::WorkflowUid(uuid::Uuid::nil());
    let _: wyrd_spec::vala::eval::ids::EntityUid =
        wyrd_spec::vala::eval::ids::EntityUid::new("u").expect("valid entity uid");
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
