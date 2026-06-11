use wyrd_client::transport::config::{HTTP_DEFAULT_BASE_URL, HTTP_DEFAULT_TIMEOUT_MS, HttpConfig};
use wyrd_spec::security::{SecretRef, TlsConfig};

#[test]
fn http_default_values() {
    let config = HttpConfig::default();
    assert_eq!(config.base_url, HTTP_DEFAULT_BASE_URL);
    assert_eq!(config.timeout_ms, HTTP_DEFAULT_TIMEOUT_MS);
    assert!(config.tls.is_none());
    assert!(config.auth.is_none());
    assert!(!config.compression);
}

#[test]
fn http_default_validates_clean() {
    assert!(HttpConfig::default().validate().is_ok());
}

#[test]
fn http_default_round_trips() {
    let config = HttpConfig::default();
    let serialized = serde_json::to_string(&config).expect("http serializes");
    let back: HttpConfig = serde_json::from_str(&serialized).expect("http deserializes");
    assert_eq!(config, back);
}

#[test]
fn http_deserialize_uses_serde_defaults_for_missing_fields() {
    let config: HttpConfig = serde_json::from_str("{}").expect("http defaults deserialize");
    assert_eq!(config, HttpConfig::default());
}

#[test]
fn http_config_round_trips() {
    let config = HttpConfig {
        base_url: "https://wyrd-ingest.example.com".to_string(),
        timeout_ms: 10_000,
        tls: None,
        auth: Some(SecretRef::Env {
            name: "WYRD_API_KEY".to_string(),
        }),
        compression: true,
    };
    let serialized = serde_json::to_string(&config).expect("http serializes");
    let back: HttpConfig = serde_json::from_str(&serialized).expect("http deserializes");
    assert_eq!(config, back);
}

#[test]
fn http_validate_rejects_empty_base_url() {
    let config = HttpConfig {
        base_url: String::new(),
        timeout_ms: 5_000,
        tls: None,
        auth: None,
        compression: false,
    };
    assert!(config.validate().is_err());
}

#[test]
fn http_validate_rejects_zero_timeout() {
    let config = HttpConfig {
        timeout_ms: 0,
        ..HttpConfig::default()
    };
    let error = config.validate().expect_err("zero timeout rejected");
    assert!(error.to_string().contains("timeout_ms"));
}

#[test]
fn http_with_full_tls_round_trips() {
    let config = HttpConfig {
        base_url: "https://wyrd.example.com".to_string(),
        timeout_ms: 5_000,
        tls: Some(TlsConfig {
            ca_cert: Some(SecretRef::File {
                path: "/etc/ssl/ca.pem".to_string(),
            }),
            client_cert: Some(SecretRef::Vault {
                key: "wyrd/tls/cert".to_string(),
            }),
            client_key: Some(SecretRef::Vault {
                key: "wyrd/tls/key".to_string(),
            }),
            server_name_override: Some("ingest.internal".to_string()),
            insecure_skip_verify: false,
        }),
        auth: Some(SecretRef::Env {
            name: "WYRD_API_KEY".to_string(),
        }),
        compression: true,
    };
    let serialized = serde_json::to_string(&config).expect("http serializes");
    let back: HttpConfig = serde_json::from_str(&serialized).expect("http deserializes");
    assert_eq!(config, back);
}

#[test]
fn http_rejects_unknown_fields() {
    let result: Result<HttpConfig, _> = serde_json::from_str(
        r#"{"base_url":"https://example.com","timeout_ms":5000,"compression":false,"bad":1}"#,
    );
    assert!(result.is_err());
}
