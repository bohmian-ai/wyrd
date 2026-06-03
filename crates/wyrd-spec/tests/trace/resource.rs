use serde_json::json;

use wyrd_spec::vala::trace::Resource;

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
