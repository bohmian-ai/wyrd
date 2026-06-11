use wyrd_client::transport::{GrpcConfig, HttpConfig, MockConfig, TransportConfig};

#[test]
fn transport_config_default_is_grpc() {
    let config = TransportConfig::default();
    assert!(matches!(config, TransportConfig::Grpc(_)));
}

#[test]
fn transport_config_default_grpc_uses_grpc_default() {
    let config = TransportConfig::default();
    if let TransportConfig::Grpc(grpc) = config {
        assert_eq!(grpc, GrpcConfig::default());
    } else {
        panic!("expected grpc variant");
    }
}

#[test]
fn transport_config_grpc_tag_content_shape() {
    let config = TransportConfig::Grpc(GrpcConfig::default());
    let serialized = serde_json::to_string(&config).expect("transport serializes");
    assert!(serialized.contains(r#""transport":"grpc""#));
    assert!(serialized.contains(r#""params":"#));
}

#[test]
fn transport_config_all_variants_round_trip() {
    let cases = vec![
        TransportConfig::Grpc(GrpcConfig::default()),
        TransportConfig::Http(HttpConfig::default()),
        TransportConfig::Mock(MockConfig {
            label: "buf".to_string(),
            fail_on_flush: Some(3),
        }),
    ];

    for config in cases {
        let serialized = serde_json::to_string(&config).expect("transport serializes");
        let back: TransportConfig =
            serde_json::from_str(&serialized).expect("transport deserializes");
        assert_eq!(config, back, "round trip failed for {}", config.name());
    }
}

#[test]
fn transport_config_name_matches_tag() {
    let cases = vec![
        ("grpc", TransportConfig::Grpc(GrpcConfig::default())),
        ("http", TransportConfig::Http(HttpConfig::default())),
        ("mock", TransportConfig::Mock(MockConfig::default())),
    ];
    for (expected, config) in cases {
        assert_eq!(config.name(), expected);
    }
}

#[test]
fn transport_config_is_enabled_mock_always_true() {
    let config = TransportConfig::Mock(MockConfig::default());
    assert!(config.is_enabled());
    assert!(config.required_feature().is_none());
}

#[cfg(feature = "transport-grpc")]
#[test]
fn transport_config_is_enabled_grpc_with_feature() {
    assert!(TransportConfig::Grpc(GrpcConfig::default()).is_enabled());
}

#[cfg(feature = "transport-http")]
#[test]
fn transport_config_is_enabled_http_with_feature() {
    assert!(TransportConfig::Http(HttpConfig::default()).is_enabled());
}

#[test]
fn transport_config_required_feature_matches_variant() {
    assert_eq!(
        TransportConfig::Grpc(GrpcConfig::default()).required_feature(),
        Some("transport-grpc")
    );
    assert_eq!(
        TransportConfig::Http(HttpConfig::default()).required_feature(),
        Some("transport-http")
    );
}

#[test]
fn transport_config_rejects_deferred_variant_tags() {
    for tag in ["kafka", "rabbit_mq", "redis"] {
        let payload = format!(r#"{{"transport":"{tag}","params":{{}}}}"#);
        let result: Result<TransportConfig, _> = serde_json::from_str(&payload);
        assert!(result.is_err(), "expected unknown variant for {tag}");
    }
}
