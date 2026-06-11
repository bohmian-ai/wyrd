use std::collections::BTreeMap;

use vala_core::alert_router::webhook::{
    WEBHOOK_DEFAULT_TIMEOUT_MS, WEBHOOK_SIGNATURE_HEADER_DEFAULT, WEBHOOK_TIMESTAMP_HEADER_DEFAULT,
    WebhookConfig, WebhookMethod,
};
use wyrd_spec::security::SecretRef;

fn base_config() -> WebhookConfig {
    WebhookConfig {
        url: "https://hooks.example.com/wyrd-alerts".to_string(),
        ..WebhookConfig::default()
    }
}

#[test]
fn webhook_default_constants_match_struct_default() {
    let config = WebhookConfig::default();
    assert_eq!(config.method, WebhookMethod::Post);
    assert_eq!(config.signature_header, WEBHOOK_SIGNATURE_HEADER_DEFAULT);
    assert_eq!(config.timestamp_header, WEBHOOK_TIMESTAMP_HEADER_DEFAULT);
    assert_eq!(config.timeout_ms, WEBHOOK_DEFAULT_TIMEOUT_MS);
    assert!(config.secret_ref.is_none());
    assert!(config.tls.is_none());
    assert!(config.headers.is_empty());
    assert!(config.secret_headers.is_empty());
    assert_eq!(config.url, "");
}

#[test]
fn webhook_default_rejects_validation_because_url_is_empty() {
    let error = WebhookConfig::default()
        .validate()
        .expect_err("empty url rejected");
    assert!(error.to_string().contains("url"));
}

#[test]
fn webhook_default_round_trips() {
    let config = WebhookConfig::default();
    let serialized = serde_json::to_string(&config).expect("webhook serializes");
    let back: WebhookConfig = serde_json::from_str(&serialized).expect("webhook deserializes");
    assert_eq!(config, back);
}

#[test]
fn webhook_deserialize_uses_serde_defaults_for_missing_fields() {
    let config: WebhookConfig = serde_json::from_str("{}").expect("webhook defaults deserialize");
    assert_eq!(config, WebhookConfig::default());
}

#[test]
fn webhook_method_round_trips_uppercase() {
    let serialized = serde_json::to_string(&WebhookMethod::Post).expect("method serializes");
    assert_eq!(serialized, "\"POST\"");
    let back: WebhookMethod = serde_json::from_str(&serialized).expect("method deserializes");
    assert_eq!(back, WebhookMethod::Post);

    let serialized = serde_json::to_string(&WebhookMethod::Put).expect("method serializes");
    assert_eq!(serialized, "\"PUT\"");
    let back: WebhookMethod = serde_json::from_str(&serialized).expect("method deserializes");
    assert_eq!(back, WebhookMethod::Put);
}

#[test]
fn webhook_method_lowercase_and_unsupported_verb_rejected() {
    assert!(serde_json::from_str::<WebhookMethod>(r#""post""#).is_err());
    assert!(serde_json::from_str::<WebhookMethod>(r#""GET""#).is_err());
}

#[test]
fn webhook_config_with_secret_ref_round_trips() {
    let config = WebhookConfig {
        secret_ref: Some(SecretRef::Env {
            name: "WEBHOOK_SECRET".to_string(),
        }),
        ..base_config()
    };
    let serialized = serde_json::to_string(&config).expect("webhook serializes");
    let back: WebhookConfig = serde_json::from_str(&serialized).expect("webhook deserializes");
    assert_eq!(config, back);
}

#[test]
fn webhook_validate_rejects_empty_header_fields_and_timeout() {
    let config = WebhookConfig {
        signature_header: String::new(),
        ..base_config()
    };
    assert!(config.validate().is_err());

    let config = WebhookConfig {
        timestamp_header: String::new(),
        ..base_config()
    };
    assert!(config.validate().is_err());

    let config = WebhookConfig {
        timeout_ms: 0,
        ..base_config()
    };
    assert!(config.validate().is_err());
}

#[test]
fn webhook_validate_rejects_same_signature_and_timestamp_header() {
    let config = WebhookConfig {
        timestamp_header: "x-wyrd-signature".to_string(),
        ..base_config()
    };
    let error = config
        .validate()
        .expect_err("signature/timestamp header collision rejected");
    assert!(error.to_string().contains("must differ"));
}

#[test]
fn webhook_headers_denylist_rejects_secret_shaped_names() {
    for name in [
        "Authorization",
        "Cookie",
        "X-Custom-Token",
        "X-Service-Key",
        "X-App-Secret",
        "X-User-Auth",
        "X-Vendor-Credential",
        "X-API-Key",
        "X-Amz-Security-Token",
    ] {
        let mut headers = BTreeMap::new();
        headers.insert(name.to_string(), "value".to_string());
        let config = WebhookConfig {
            headers,
            ..base_config()
        };
        assert!(config.validate().is_err(), "name: {name}");
    }
}

#[test]
fn webhook_headers_denylist_allows_non_sensitive_names() {
    let mut headers = BTreeMap::new();
    headers.insert("Content-Type".to_string(), "application/json".to_string());
    headers.insert("X-App-Name".to_string(), "wyrd-alerts".to_string());
    headers.insert("X-Request-Id".to_string(), "req-1".to_string());
    headers.insert("User-Agent".to_string(), "wyrd/1.0".to_string());
    let config = WebhookConfig {
        headers,
        ..base_config()
    };
    assert!(config.validate().is_ok());
}

#[test]
fn webhook_headers_reject_signing_and_timestamp_collisions() {
    for name in ["X-Wyrd-Signature", "x-wyrd-signature", "X-Wyrd-Timestamp"] {
        let mut headers = BTreeMap::new();
        headers.insert(name.to_string(), "value".to_string());
        let config = WebhookConfig {
            headers,
            ..base_config()
        };
        assert!(config.validate().is_err(), "name: {name}");
    }
}

#[test]
fn webhook_secret_headers_round_trip_and_allow_authorization() {
    let mut secret_headers = BTreeMap::new();
    secret_headers.insert(
        "Authorization".to_string(),
        SecretRef::Env {
            name: "WYRD_WEBHOOK_BEARER".to_string(),
        },
    );
    let config = WebhookConfig {
        secret_headers,
        ..base_config()
    };
    assert!(config.validate().is_ok());
    let serialized = serde_json::to_string(&config).expect("webhook serializes");
    let back: WebhookConfig = serde_json::from_str(&serialized).expect("webhook deserializes");
    assert_eq!(config, back);
}

#[test]
fn webhook_secret_headers_reject_signing_and_timestamp_collisions() {
    for name in ["X-Wyrd-Signature", "X-Wyrd-Timestamp"] {
        let mut secret_headers = BTreeMap::new();
        secret_headers.insert(
            name.to_string(),
            SecretRef::Env {
                name: "WYRD_SECRET".to_string(),
            },
        );
        let config = WebhookConfig {
            secret_headers,
            ..base_config()
        };
        assert!(config.validate().is_err(), "name: {name}");
    }
}

#[test]
fn webhook_rejects_duplicate_key_across_headers_and_secret_headers() {
    let mut headers = BTreeMap::new();
    headers.insert("X-App-Name".to_string(), "wyrd".to_string());
    let mut secret_headers = BTreeMap::new();
    secret_headers.insert(
        "x-app-name".to_string(),
        SecretRef::Env {
            name: "WYRD_APP".to_string(),
        },
    );
    let config = WebhookConfig {
        headers,
        secret_headers,
        ..base_config()
    };
    let error = config.validate().expect_err("duplicate key rejected");
    assert!(error.to_string().contains("appears in both"));
}

#[test]
fn webhook_secret_headers_default_empty_omitted_from_serialized_output() {
    let serialized = serde_json::to_string(&base_config()).expect("webhook serializes");
    assert!(!serialized.contains("secret_headers"));
}
