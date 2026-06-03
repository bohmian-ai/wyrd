use serde_json::json;

use wyrd_spec::vala::trace::InstrumentationScope;

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
