use wyrd_client::transport::{QueueConfig, TransportConfig};

#[test]
fn queue_config_default_predecessor_parity() {
    let config = QueueConfig::default();
    assert_eq!(config.flush_max_rows, 10_000);
    assert_eq!(config.flush_interval_ms, 5_000);
    assert_eq!(config.channel_capacity, 100);
    assert!(config.sample_ratio.is_none());
    assert!(matches!(config.transport, TransportConfig::Grpc(_)));
}

#[test]
fn queue_config_default_round_trips() {
    let config = QueueConfig::default();
    let serialized = serde_json::to_string(&config).expect("queue config serializes");
    let back: QueueConfig = serde_json::from_str(&serialized).expect("queue config deserializes");
    assert_eq!(config, back);
}

#[test]
fn queue_config_validate_rejects_zero_bounds() {
    let config = QueueConfig {
        flush_max_rows: 0,
        ..QueueConfig::default()
    };
    assert!(config.validate().is_err());

    let config = QueueConfig {
        flush_interval_ms: 0,
        ..QueueConfig::default()
    };
    assert!(config.validate().is_err());

    let config = QueueConfig {
        channel_capacity: 0,
        ..QueueConfig::default()
    };
    assert!(config.validate().is_err());
}

#[test]
fn queue_config_validate_accepts_lower_bound_one() {
    let config = QueueConfig {
        flush_max_rows: 1,
        flush_interval_ms: 1,
        channel_capacity: 1,
        ..QueueConfig::default()
    };
    assert!(config.validate().is_ok());
}

#[test]
fn queue_config_validate_accepts_sample_ratio_edges() {
    for ratio in [None, Some(0.0), Some(0.5), Some(1.0)] {
        let config = QueueConfig {
            sample_ratio: ratio,
            ..QueueConfig::default()
        };
        assert!(config.validate().is_ok(), "ratio: {ratio:?}");
    }
}

#[test]
fn queue_config_validate_rejects_invalid_sample_ratios() {
    for ratio in [Some(-0.1), Some(1.1), Some(f64::NAN), Some(f64::INFINITY)] {
        let config = QueueConfig {
            sample_ratio: ratio,
            ..QueueConfig::default()
        };
        assert!(config.validate().is_err(), "ratio: {ratio:?}");
    }
}

#[test]
fn queue_config_sample_ratio_omits_field_when_none() {
    let config = QueueConfig {
        sample_ratio: None,
        ..QueueConfig::default()
    };
    let serialized = serde_json::to_string(&config).expect("queue config serializes");
    assert!(!serialized.contains("sample_ratio"));
}

#[test]
fn queue_config_sample_ratio_present_when_some() {
    let config = QueueConfig {
        sample_ratio: Some(0.25),
        ..QueueConfig::default()
    };
    let serialized = serde_json::to_string(&config).expect("queue config serializes");
    assert!(serialized.contains("sample_ratio"));
    let back: QueueConfig = serde_json::from_str(&serialized).expect("queue config deserializes");
    assert_eq!(back.sample_ratio, Some(0.25));
}
