use wyrd_client::transport::config::{MOCK_DEFAULT_LABEL, MockConfig};

#[test]
fn mock_config_default_uses_default_label_and_no_failure() {
    let m = MockConfig::default();
    assert_eq!(m.label, MOCK_DEFAULT_LABEL);
    assert_eq!(m.label, "default");
    assert!(m.fail_on_drain.is_none());
}

#[test]
fn mock_config_default_round_trips() {
    let m = MockConfig::default();
    let s = serde_json::to_string(&m).unwrap();
    let back: MockConfig = serde_json::from_str(&s).unwrap();
    assert_eq!(m, back);
}

#[test]
fn mock_config_no_fail_round_trips() {
    let m = MockConfig {
        label: "test-buffer".to_string(),
        fail_on_drain: None,
    };
    let s = serde_json::to_string(&m).unwrap();
    let back: MockConfig = serde_json::from_str(&s).unwrap();
    assert_eq!(m, back);
}

#[test]
fn mock_config_no_fail_omits_fail_on_drain_field() {
    let m = MockConfig {
        label: "buf".to_string(),
        fail_on_drain: None,
    };
    let s = serde_json::to_string(&m).unwrap();
    assert!(!s.contains("fail_on_drain"));
}

#[test]
fn mock_config_with_fail_on_drain_round_trips() {
    let m = MockConfig {
        label: "buf".to_string(),
        fail_on_drain: Some(2),
    };
    let s = serde_json::to_string(&m).unwrap();
    let back: MockConfig = serde_json::from_str(&s).unwrap();
    assert_eq!(m, back);
}

#[test]
fn mock_config_fail_on_drain_first_call() {
    let m = MockConfig {
        label: "fail-first".to_string(),
        fail_on_drain: Some(1),
    };
    let s = serde_json::to_string(&m).unwrap();
    let back: MockConfig = serde_json::from_str(&s).unwrap();
    assert_eq!(back.fail_on_drain, Some(1));
}

#[test]
fn mock_config_rejects_unknown_fields() {
    let r: Result<MockConfig, _> =
        serde_json::from_str(r#"{"label":"buf","fail_on_drain":null,"extra":true}"#);
    assert!(r.is_err());
}

#[test]
fn mock_fail_on_drain_zero_round_trips_and_is_distinct_from_none() {
    // Some(0) is documented as never-fail: the drain counter starts at 1
    // after increment, so n=0 is never matched. Verify the value round-trips
    // cleanly and is distinguishable from None at the config level.
    let cfg = MockConfig {
        fail_on_drain: Some(0),
        ..MockConfig::default()
    };
    let s = serde_json::to_string(&cfg).unwrap();
    let back: MockConfig = serde_json::from_str(&s).unwrap();
    assert_eq!(back.fail_on_drain, Some(0));
    assert_ne!(back.fail_on_drain, None);
}

#[test]
fn mock_config_distinct_labels_are_not_equal() {
    let a = MockConfig {
        label: "a".to_string(),
        fail_on_drain: None,
    };
    let b = MockConfig {
        label: "b".to_string(),
        fail_on_drain: None,
    };
    assert_ne!(a, b);
}
