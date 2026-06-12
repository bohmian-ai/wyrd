use wyrd_client::error::WyrdClientError;
use wyrd_client::transport::{GrpcConfig, QueueConfig, TransportConfig};

#[test]
fn queue_config_default_predecessor_parity() {
    let q = QueueConfig::default();
    assert_eq!(q.flush_max_rows, 10_000);
    assert_eq!(q.flush_interval_ms, 5_000);
    assert_eq!(q.channel_capacity, 100);
    assert!(q.sample_ratio.is_none());
    assert!(matches!(q.transport, TransportConfig::Grpc(_)));
}

#[test]
fn queue_config_default_round_trips() {
    let q = QueueConfig::default();
    let s = serde_json::to_string(&q).unwrap();
    let back: QueueConfig = serde_json::from_str(&s).unwrap();
    assert_eq!(q, back);
}

#[test]
fn queue_config_validate_rejects_flush_max_rows_zero() {
    let q = QueueConfig {
        flush_max_rows: 0,
        ..QueueConfig::default()
    };
    assert_config_error(
        q.validate().unwrap_err(),
        "queue_config.flush_max_rows",
        "must be >= 1",
    );
}

#[test]
fn queue_config_validate_accepts_flush_max_rows_one() {
    let q = QueueConfig {
        flush_max_rows: 1,
        ..QueueConfig::default()
    };
    assert!(q.validate().is_ok());
}

#[test]
fn queue_config_validate_rejects_channel_capacity_zero() {
    let q = QueueConfig {
        channel_capacity: 0,
        ..QueueConfig::default()
    };
    assert_config_error(
        q.validate().unwrap_err(),
        "queue_config.channel_capacity",
        "must be >= 1",
    );
}

#[test]
fn queue_config_validate_accepts_channel_capacity_one() {
    let q = QueueConfig {
        channel_capacity: 1,
        ..QueueConfig::default()
    };
    assert!(q.validate().is_ok());
}

#[test]
fn queue_config_validate_rejects_sample_ratio_below_zero() {
    let q = QueueConfig {
        sample_ratio: Some(-0.1),
        ..QueueConfig::default()
    };
    assert!(q.validate().is_err());
}

#[test]
fn queue_config_validate_rejects_sample_ratio_above_one() {
    let q = QueueConfig {
        sample_ratio: Some(1.1),
        ..QueueConfig::default()
    };
    assert!(q.validate().is_err());
}

#[test]
fn queue_config_validate_accepts_sample_ratio_zero() {
    let q = QueueConfig {
        sample_ratio: Some(0.0),
        ..QueueConfig::default()
    };
    assert!(q.validate().is_ok());
}

#[test]
fn queue_config_validate_accepts_sample_ratio_one() {
    let q = QueueConfig {
        sample_ratio: Some(1.0),
        ..QueueConfig::default()
    };
    assert!(q.validate().is_ok());
}

#[test]
fn queue_config_validate_accepts_sample_ratio_midpoint() {
    let q = QueueConfig {
        sample_ratio: Some(0.5),
        ..QueueConfig::default()
    };
    assert!(q.validate().is_ok());
}

#[test]
fn queue_config_validate_accepts_sample_ratio_none() {
    let q = QueueConfig {
        sample_ratio: None,
        ..QueueConfig::default()
    };
    assert!(q.validate().is_ok());
}

#[test]
fn queue_config_sample_ratio_omits_field_when_none() {
    let q = QueueConfig {
        sample_ratio: None,
        ..QueueConfig::default()
    };
    let s = serde_json::to_string(&q).unwrap();
    assert!(!s.contains("sample_ratio"));
}

#[test]
fn queue_config_sample_ratio_present_when_some() {
    let q = QueueConfig {
        sample_ratio: Some(0.25),
        ..QueueConfig::default()
    };
    let s = serde_json::to_string(&q).unwrap();
    assert!(s.contains("sample_ratio"));
    let back: QueueConfig = serde_json::from_str(&s).unwrap();
    assert_eq!(back.sample_ratio.map(f64::to_bits), Some(0.25f64.to_bits()));
}

#[test]
fn queue_config_validate_rejects_flush_interval_ms_zero() {
    let q = QueueConfig {
        flush_interval_ms: 0,
        ..QueueConfig::default()
    };
    assert_config_error(
        q.validate().unwrap_err(),
        "queue_config.flush_interval_ms",
        "must be >= 1",
    );
}

#[test]
fn queue_config_validate_accepts_flush_interval_ms_one() {
    let q = QueueConfig {
        flush_interval_ms: 1,
        ..QueueConfig::default()
    };
    assert!(q.validate().is_ok());
}

#[test]
fn queue_config_validate_rejects_sample_ratio_nan() {
    let q = QueueConfig {
        sample_ratio: Some(f64::NAN),
        ..QueueConfig::default()
    };
    let err = q.validate().unwrap_err().to_string();
    assert!(err.contains("queue_config.sample_ratio: must be in [0.0, 1.0]"));
}

#[test]
fn queue_config_validate_rejects_sample_ratio_infinity() {
    let q = QueueConfig {
        sample_ratio: Some(f64::INFINITY),
        ..QueueConfig::default()
    };
    assert!(q.validate().is_err());
}

#[test]
fn queue_config_validate_does_not_validate_inner_transport() {
    let q = QueueConfig {
        transport: TransportConfig::Grpc(GrpcConfig {
            endpoint: String::new(),
            ..GrpcConfig::default()
        }),
        ..QueueConfig::default()
    };
    assert!(q.validate().is_ok());
}

fn assert_config_error(err: WyrdClientError, expected_field: &str, expected_reason: &str) {
    let WyrdClientError::Config { field, reason } = err else {
        panic!("expected WyrdClientError::Config");
    };
    assert_eq!(field, expected_field);
    assert_eq!(reason, expected_reason);
}
