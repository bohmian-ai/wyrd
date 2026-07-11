use wyrd_client::{
    error::WyrdClientError,
    transport::{GrpcConfig, HttpConfig, MockConfig, TransportConfig},
};

#[test]
fn transport_config_default_is_grpc() {
    let d = TransportConfig::default();
    assert!(matches!(d, TransportConfig::Grpc(_)));
}

#[test]
fn transport_config_default_grpc_uses_grpc_default() {
    let d = TransportConfig::default();
    if let TransportConfig::Grpc(g) = d {
        assert_eq!(g, GrpcConfig::default());
    } else {
        panic!("expected Grpc variant");
    }
}

#[test]
fn transport_config_grpc_tag_content_shape() {
    let c = TransportConfig::Grpc(GrpcConfig::default());
    let s = serde_json::to_string(&c).unwrap();
    assert!(s.contains(r#""transport":"grpc""#));
    assert!(s.contains(r#""params":"#));
}

#[test]
fn transport_config_grpc_round_trips() {
    let c = TransportConfig::Grpc(GrpcConfig::default());
    let s = serde_json::to_string(&c).unwrap();
    let back: TransportConfig = serde_json::from_str(&s).unwrap();
    assert_eq!(c, back);
}

#[test]
fn transport_config_mock_round_trips() {
    let c = TransportConfig::Mock(MockConfig {
        label: "t".to_string(),
        fail_on_drain: None,
    });
    let s = serde_json::to_string(&c).unwrap();
    let back: TransportConfig = serde_json::from_str(&s).unwrap();
    assert_eq!(c, back);
}

#[test]
fn transport_config_name_matches_tag() {
    let cases: Vec<(&'static str, TransportConfig)> = vec![
        ("grpc", TransportConfig::Grpc(GrpcConfig::default())),
        (
            "http",
            TransportConfig::Http(HttpConfig {
                base_url: "https://example.com".to_string(),
                timeout_ms: 5_000,
                tls: None,
                compression: false,
            }),
        ),
        (
            "mock",
            TransportConfig::Mock(MockConfig {
                label: "m".to_string(),
                fail_on_drain: None,
            }),
        ),
    ];

    for (expected_name, config) in &cases {
        assert_eq!(
            config.name(),
            *expected_name,
            "name() mismatch for {:?}",
            config.name()
        );
    }
}

#[test]
fn transport_config_all_variants_round_trip() {
    let cases = vec![
        TransportConfig::Grpc(GrpcConfig::default()),
        TransportConfig::Http(HttpConfig {
            base_url: "https://example.com".to_string(),
            timeout_ms: 30_000,
            tls: None,
            compression: false,
        }),
        TransportConfig::Mock(MockConfig {
            label: "buf".to_string(),
            fail_on_drain: Some(3),
        }),
    ];

    for c in &cases {
        let s = serde_json::to_string(c).unwrap();
        let back: TransportConfig = serde_json::from_str(&s).unwrap();
        assert_eq!(c, &back, "round-trip failed for transport {}", c.name());
    }
}

#[test]
fn transport_config_validate_grpc_empty_endpoint_returns_err() {
    let c = TransportConfig::Grpc(GrpcConfig {
        endpoint: String::new(),
        ..GrpcConfig::default()
    });
    let err = c.validate().unwrap_err();
    assert!(
        matches!(err, WyrdClientError::Config { ref field, .. } if field == "grpc_config.endpoint"),
        "expected Config {{ field: grpc_config.endpoint }}, got: {err:?}"
    );
}

#[test]
fn transport_config_validate_mock_always_ok() {
    let c = TransportConfig::Mock(MockConfig::default());
    assert!(c.validate().is_ok());
}

#[test]
fn transport_config_rejects_deferred_variant_tags() {
    // Queue transports (Kafka, RabbitMQ, Redis) are not part of this phase's
    // TransportConfig. Their tags must round-trip as "unknown variant" errors,
    // regression-pinning the scope reduction.
    for tag in &["kafka", "rabbit_mq", "redis"] {
        let payload = format!(r#"{{"transport":"{tag}","params":{{}}}}"#);
        let r: Result<TransportConfig, _> = serde_json::from_str(&payload);
        assert!(r.is_err(), "expected unknown variant for tag {tag:?}");
    }
}
