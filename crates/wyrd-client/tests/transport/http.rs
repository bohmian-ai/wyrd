use wyrd_client::error::WyrdClientError;
use wyrd_client::transport::config::{HTTP_DEFAULT_BASE_URL, HTTP_DEFAULT_TIMEOUT_MS, HttpConfig};
use wyrd_spec::security::{SecretRef, TlsConfig};

#[test]
fn http_default_values() {
    let h = HttpConfig::default();
    assert_eq!(h.base_url, HTTP_DEFAULT_BASE_URL);
    assert_eq!(h.timeout_ms, HTTP_DEFAULT_TIMEOUT_MS);
    assert!(h.tls.is_none());
    assert!(h.auth.is_none());
    assert!(!h.compression);
}

#[test]
fn http_default_validates_clean() {
    assert!(HttpConfig::default().validate().is_ok());
}

#[test]
fn http_default_round_trips() {
    let h = HttpConfig::default();
    let s = serde_json::to_string(&h).unwrap();
    let back: HttpConfig = serde_json::from_str(&s).unwrap();
    assert_eq!(h, back);
}

#[test]
fn http_deserialize_uses_serde_defaults_for_missing_fields() {
    // `serde(default = "...")` fills `base_url`/`timeout_ms`; `compression`
    // defaults via `#[serde(default)]`. Unknown fields are still rejected by
    // `deny_unknown_fields`.
    let h: HttpConfig = serde_json::from_str("{}").unwrap();
    assert_eq!(h, HttpConfig::default());
}

#[test]
fn http_config_round_trips() {
    let h = HttpConfig {
        base_url: "https://wyrd-ingest.example.com".to_string(),
        timeout_ms: 10_000,
        tls: None,
        auth: Some(SecretRef::Env {
            name: "WYRD_API_KEY".to_string(),
        }),
        compression: true,
    };
    let s = serde_json::to_string(&h).unwrap();
    let back: HttpConfig = serde_json::from_str(&s).unwrap();
    assert_eq!(h, back);
}

#[test]
fn http_compression_false_round_trips() {
    let h = HttpConfig {
        base_url: "https://wyrd.example.com".to_string(),
        timeout_ms: 30_000,
        tls: None,
        auth: None,
        compression: false,
    };
    let s = serde_json::to_string(&h).unwrap();
    let back: HttpConfig = serde_json::from_str(&s).unwrap();
    assert_eq!(h, back);
}

#[test]
fn http_validate_rejects_empty_base_url() {
    let h = HttpConfig {
        base_url: String::new(),
        timeout_ms: 5_000,
        tls: None,
        auth: None,
        compression: false,
    };
    let err = h.validate().unwrap_err();
    assert_config_error(err, "http_config.base_url", "must not be empty");
}

#[test]
fn http_validate_accepts_non_empty_base_url() {
    let h = HttpConfig {
        base_url: "https://example.com".to_string(),
        timeout_ms: 5_000,
        tls: None,
        auth: None,
        compression: false,
    };
    assert!(h.validate().is_ok());
}

#[test]
fn http_validate_rejects_zero_timeout() {
    let h = HttpConfig {
        base_url: "https://example.com".to_string(),
        timeout_ms: 0,
        tls: None,
        auth: None,
        compression: false,
    };
    let err = h.validate().unwrap_err();
    assert_config_error(err, "http_config.timeout_ms", "must be at least 1");
}

#[test]
fn http_with_full_tls_round_trips() {
    let h = HttpConfig {
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
    let s = serde_json::to_string(&h).unwrap();
    let back: HttpConfig = serde_json::from_str(&s).unwrap();
    assert_eq!(h, back);
}

#[test]
fn http_rejects_unknown_fields() {
    let r: Result<HttpConfig, _> = serde_json::from_str(
        r#"{"base_url":"https://example.com","timeout_ms":5000,"compression":false,"bad":1}"#,
    );
    assert!(r.is_err());
}

fn assert_config_error(err: WyrdClientError, expected_field: &str, expected_reason: &str) {
    let WyrdClientError::Config { field, reason } = err;
    assert_eq!(field, expected_field);
    assert_eq!(reason, expected_reason);
}
