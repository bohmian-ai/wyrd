use wyrd_client::error::WyrdClientError;
use wyrd_client::transport::config::GrpcConfig;
use wyrd_spec::security::{SecretRef, TlsConfig};

#[test]
fn grpc_default_values() {
    let g = GrpcConfig::default();
    assert_eq!(g.endpoint, "http://localhost:50051");
    assert_eq!(g.timeout_ms, 30_000);
    assert_eq!(g.connect_retries, 3);
    assert!(g.tls.is_none());
    assert!(g.auth.is_none());
}

#[test]
fn grpc_default_round_trips() {
    let g = GrpcConfig::default();
    let s = serde_json::to_string(&g).unwrap();
    let back: GrpcConfig = serde_json::from_str(&s).unwrap();
    assert_eq!(g, back);
}

#[test]
fn grpc_with_tls_round_trips() {
    let g = GrpcConfig {
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
    let s = serde_json::to_string(&g).unwrap();
    let back: GrpcConfig = serde_json::from_str(&s).unwrap();
    assert_eq!(g, back);
}

#[test]
fn grpc_with_auth_round_trips() {
    let g = GrpcConfig {
        auth: Some(SecretRef::Env {
            name: "WYRD_API_KEY".to_string(),
        }),
        ..GrpcConfig::default()
    };
    let s = serde_json::to_string(&g).unwrap();
    let back: GrpcConfig = serde_json::from_str(&s).unwrap();
    assert_eq!(g, back);
}

#[test]
fn grpc_validate_rejects_empty_endpoint() {
    let g = GrpcConfig {
        endpoint: String::new(),
        ..GrpcConfig::default()
    };
    let err = g.validate().unwrap_err();
    assert_config_error(err, "grpc_config.endpoint", "must not be empty");
}

#[test]
fn grpc_validate_accepts_non_empty_endpoint() {
    let g = GrpcConfig::default();
    assert!(g.validate().is_ok());
}

#[test]
fn grpc_validate_rejects_zero_timeout() {
    let g = GrpcConfig {
        timeout_ms: 0,
        ..GrpcConfig::default()
    };
    let err = g.validate().unwrap_err();
    assert_config_error(err, "grpc_config.timeout_ms", "must be at least 1");
}

#[test]
fn grpc_validate_accepts_zero_connect_retries() {
    let g = GrpcConfig {
        connect_retries: 0,
        ..GrpcConfig::default()
    };
    assert!(g.validate().is_ok());
}

#[test]
fn grpc_rejects_unknown_fields() {
    let r: Result<GrpcConfig, _> = serde_json::from_str(
        r#"{"endpoint":"http://localhost:50051","timeout_ms":5000,"connect_retries":1,"unknown_field":true}"#,
    );
    assert!(r.is_err());
}

fn assert_config_error(err: WyrdClientError, expected_field: &str, expected_reason: &str) {
    let WyrdClientError::Config { field, reason } = err else {
        panic!("expected WyrdClientError::Config");
    };
    assert_eq!(field, expected_field);
    assert_eq!(reason, expected_reason);
}
