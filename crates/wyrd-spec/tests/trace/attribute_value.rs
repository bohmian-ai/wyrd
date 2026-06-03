use std::collections::BTreeMap;

use wyrd_spec::vala::trace::AttributeValue;

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
