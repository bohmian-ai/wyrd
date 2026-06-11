use wyrd_client::transport::config::{MOCK_DEFAULT_LABEL, MockConfig};

#[test]
fn mock_config_default_uses_default_label_and_no_failure() {
    let config = MockConfig::default();
    assert_eq!(config.label, MOCK_DEFAULT_LABEL);
    assert_eq!(config.label, "default");
    assert!(config.fail_on_flush.is_none());
}

#[test]
fn mock_config_default_round_trips() {
    let config = MockConfig::default();
    let serialized = serde_json::to_string(&config).expect("mock serializes");
    let back: MockConfig = serde_json::from_str(&serialized).expect("mock deserializes");
    assert_eq!(config, back);
}

#[test]
fn mock_config_omits_fail_on_flush_when_none() {
    let config = MockConfig {
        label: "buf".to_string(),
        fail_on_flush: None,
    };
    let serialized = serde_json::to_string(&config).expect("mock serializes");
    assert!(!serialized.contains("fail_on_flush"));
}

#[test]
fn mock_config_with_fail_on_flush_round_trips() {
    let config = MockConfig {
        label: "buf".to_string(),
        fail_on_flush: Some(2),
    };
    let serialized = serde_json::to_string(&config).expect("mock serializes");
    let back: MockConfig = serde_json::from_str(&serialized).expect("mock deserializes");
    assert_eq!(config, back);
}

#[test]
fn mock_config_rejects_unknown_fields() {
    let result: Result<MockConfig, _> =
        serde_json::from_str(r#"{"label":"buf","fail_on_flush":null,"extra":true}"#);
    assert!(result.is_err());
}
