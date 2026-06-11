use wyrd_client::transport::config::GrpcConfig;
use wyrd_spec::security::{SecretRef, TlsConfig};

#[test]
fn grpc_default_values() {
    let config = GrpcConfig::default();
    assert_eq!(config.endpoint, "http://localhost:50051");
    assert_eq!(config.timeout_ms, 30_000);
    assert_eq!(config.connect_retries, 3);
    assert!(config.tls.is_none());
    assert!(config.auth.is_none());
}

#[test]
fn grpc_default_round_trips() {
    let config = GrpcConfig::default();
    let serialized = serde_json::to_string(&config).expect("grpc serializes");
    let back: GrpcConfig = serde_json::from_str(&serialized).expect("grpc deserializes");
    assert_eq!(config, back);
}

#[test]
fn grpc_with_tls_round_trips() {
    let config = GrpcConfig {
        tls: Some(TlsConfig {
            ca_cert: Some(SecretRef::File {
                path: "/etc/ssl/ca.pem".to_string(),
            }),
            client_cert: None,
            client_key: None,
            server_name_override: None,
            insecure_skip_verify: false,
        }),
        ..GrpcConfig::default()
    };
    let serialized = serde_json::to_string(&config).expect("grpc serializes");
    let back: GrpcConfig = serde_json::from_str(&serialized).expect("grpc deserializes");
    assert_eq!(config, back);
}

#[test]
fn grpc_with_auth_round_trips() {
    let config = GrpcConfig {
        auth: Some(SecretRef::Env {
            name: "WYRD_API_KEY".to_string(),
        }),
        ..GrpcConfig::default()
    };
    let serialized = serde_json::to_string(&config).expect("grpc serializes");
    let back: GrpcConfig = serde_json::from_str(&serialized).expect("grpc deserializes");
    assert_eq!(config, back);
}

#[test]
fn grpc_validate_rejects_empty_endpoint() {
    let config = GrpcConfig {
        endpoint: String::new(),
        ..GrpcConfig::default()
    };
    assert!(config.validate().is_err());
}

#[test]
fn grpc_validate_accepts_non_empty_endpoint() {
    assert!(GrpcConfig::default().validate().is_ok());
}

#[test]
fn grpc_validate_rejects_zero_timeout() {
    let config = GrpcConfig {
        timeout_ms: 0,
        ..GrpcConfig::default()
    };
    let error = config.validate().expect_err("zero timeout rejected");
    assert!(error.to_string().contains("timeout_ms"));
}

#[test]
fn grpc_validate_accepts_zero_connect_retries() {
    let config = GrpcConfig {
        connect_retries: 0,
        ..GrpcConfig::default()
    };
    assert!(config.validate().is_ok());
}

#[test]
fn grpc_rejects_unknown_fields() {
    let result: Result<GrpcConfig, _> = serde_json::from_str(
        r#"{"endpoint":"http://localhost:50051","timeout_ms":5000,"connect_retries":1,"unknown_field":true}"#,
    );
    assert!(result.is_err());
}
