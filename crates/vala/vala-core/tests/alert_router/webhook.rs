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
    let d = WebhookConfig::default();
    assert_eq!(d.method, WebhookMethod::Post);
    assert_eq!(d.signature_header, WEBHOOK_SIGNATURE_HEADER_DEFAULT);
    assert_eq!(d.timestamp_header, WEBHOOK_TIMESTAMP_HEADER_DEFAULT);
    assert_eq!(d.timeout_ms, WEBHOOK_DEFAULT_TIMEOUT_MS);
    assert!(d.secret_ref.is_none());
    assert!(d.tls.is_none());
    assert!(d.headers.is_empty());
    assert!(d.url.is_empty());
}

#[test]
fn webhook_default_rejects_validation_because_url_is_empty() {
    let d = WebhookConfig::default();
    let err = d.validate().unwrap_err().to_string();
    assert_eq!(
        err,
        "alert config invalid: webhook_config.url: must not be empty"
    );
}

#[test]
fn webhook_default_round_trips() {
    let d = WebhookConfig::default();
    let s = serde_json::to_string(&d).unwrap();
    let back: WebhookConfig = serde_json::from_str(&s).unwrap();
    assert_eq!(d, back);
}

#[test]
fn webhook_deserialize_uses_serde_defaults_for_missing_fields() {
    let h: WebhookConfig = serde_json::from_str("{}").unwrap();
    assert_eq!(h, WebhookConfig::default());
}

#[test]
fn webhook_method_post_round_trips() {
    let s = serde_json::to_string(&WebhookMethod::Post).unwrap();
    assert_eq!(s, "\"POST\"");
    let back: WebhookMethod = serde_json::from_str(&s).unwrap();
    assert_eq!(back, WebhookMethod::Post);
}

#[test]
fn webhook_method_put_round_trips() {
    let s = serde_json::to_string(&WebhookMethod::Put).unwrap();
    assert_eq!(s, "\"PUT\"");
    let back: WebhookMethod = serde_json::from_str(&s).unwrap();
    assert_eq!(back, WebhookMethod::Put);
}

#[test]
fn webhook_config_round_trips() {
    let c = base_config();
    let s = serde_json::to_string(&c).unwrap();
    let back: WebhookConfig = serde_json::from_str(&s).unwrap();
    assert_eq!(c, back);
}

#[test]
fn webhook_config_with_secret_ref_round_trips() {
    let c = WebhookConfig {
        secret_ref: Some(SecretRef::Env {
            name: "WEBHOOK_SECRET".to_string(),
        }),
        ..base_config()
    };
    let s = serde_json::to_string(&c).unwrap();
    let back: WebhookConfig = serde_json::from_str(&s).unwrap();
    assert_eq!(c, back);
}

#[test]
fn webhook_config_headers_map_round_trips() {
    let mut headers = BTreeMap::new();
    headers.insert("X-App-Name".to_string(), "wyrd-alerts".to_string());
    headers.insert("X-Version".to_string(), "1".to_string());
    let c = WebhookConfig {
        headers,
        ..base_config()
    };
    let s = serde_json::to_string(&c).unwrap();
    let back: WebhookConfig = serde_json::from_str(&s).unwrap();
    assert_eq!(c, back);
    assert!(s.contains("X-App-Name"));
}

#[test]
fn webhook_config_default_signature_header() {
    let c = base_config();
    assert_eq!(c.signature_header, "X-Wyrd-Signature");
    assert_eq!(c.timestamp_header, "X-Wyrd-Timestamp");
}

#[test]
fn webhook_config_validate_rejects_empty_url() {
    let c = WebhookConfig {
        url: String::new(),
        ..base_config()
    };
    assert!(c.validate().is_err());
}

#[test]
fn webhook_config_validate_accepts_non_empty_url() {
    let c = base_config();
    assert!(c.validate().is_ok());
}

#[test]
fn webhook_method_lowercase_rejected() {
    let r: Result<WebhookMethod, _> = serde_json::from_str(r#""post""#);
    assert!(r.is_err());
    let r: Result<WebhookMethod, _> = serde_json::from_str(r#""put""#);
    assert!(r.is_err());
}

#[test]
fn webhook_method_unsupported_verb_rejected() {
    let r: Result<WebhookMethod, _> = serde_json::from_str(r#""GET""#);
    assert!(r.is_err());
}

#[test]
fn webhook_config_validate_rejects_empty_signature_header() {
    let c = WebhookConfig {
        signature_header: String::new(),
        ..base_config()
    };
    assert_eq!(
        c.validate().unwrap_err().to_string(),
        "alert config invalid: webhook_config.signature_header: must not be empty"
    );
}

#[test]
fn webhook_config_validate_rejects_empty_timestamp_header() {
    let c = WebhookConfig {
        timestamp_header: String::new(),
        ..base_config()
    };
    assert_eq!(
        c.validate().unwrap_err().to_string(),
        "alert config invalid: webhook_config.timestamp_header: must not be empty"
    );
}

#[test]
fn webhook_config_validate_rejects_zero_timeout() {
    let c = WebhookConfig {
        timeout_ms: 0,
        ..base_config()
    };
    assert_eq!(
        c.validate().unwrap_err().to_string(),
        "alert config invalid: webhook_config.timeout_ms: must be at least 1"
    );
}

#[test]
fn webhook_config_secret_ref_none_means_signing_disabled() {
    let c = WebhookConfig {
        secret_ref: None,
        ..base_config()
    };
    assert!(c.validate().is_ok());
}

#[test]
fn webhook_config_secret_ref_none_omits_field() {
    let c = base_config();
    let s = serde_json::to_string(&c).unwrap();
    assert!(!s.contains("secret_ref"));
}

#[test]
fn webhook_config_rejects_unknown_fields() {
    let r: Result<WebhookConfig, _> = serde_json::from_str(
        r#"{"url":"https://h.example.com","method":"POST","signature_header":"X-Sig","timestamp_header":"X-Ts","timeout_ms":1000,"unknown":true}"#,
    );
    assert!(r.is_err());
}

#[test]
fn webhook_headers_denylist_rejects_authorization() {
    for name in ["Authorization", "authorization", "AUTHORIZATION"] {
        let mut h = BTreeMap::new();
        h.insert(name.to_string(), "Bearer xyz".to_string());
        let c = WebhookConfig {
            headers: h,
            ..base_config()
        };
        let err = c.validate().unwrap_err().to_string();
        assert!(err.contains("secret-shaped header name"), "got: {err}");
    }
}

#[test]
fn webhook_headers_denylist_rejects_cookie() {
    let mut h = BTreeMap::new();
    h.insert("Cookie".to_string(), "session=abc".to_string());
    let c = WebhookConfig {
        headers: h,
        ..base_config()
    };
    assert!(c.validate().is_err());
}

#[test]
fn webhook_headers_denylist_rejects_suffix_patterns() {
    for name in [
        "X-Custom-Token",
        "X-Service-Key",
        "X-App-Secret",
        "X-User-Auth",
        "X-Vendor-Credential",
    ] {
        let mut h = BTreeMap::new();
        h.insert(name.to_string(), "value".to_string());
        let c = WebhookConfig {
            headers: h,
            ..base_config()
        };
        assert!(c.validate().is_err(), "name: {name}");
    }
}

#[test]
fn webhook_headers_denylist_rejects_x_api_key_explicit() {
    let mut h = BTreeMap::new();
    h.insert("X-API-Key".to_string(), "abcd1234".to_string());
    let c = WebhookConfig {
        headers: h,
        ..base_config()
    };
    assert!(c.validate().is_err());
}

#[test]
fn webhook_headers_denylist_allows_non_sensitive_names() {
    let mut h = BTreeMap::new();
    h.insert("Content-Type".to_string(), "application/json".to_string());
    h.insert("X-App-Name".to_string(), "wyrd-alerts".to_string());
    h.insert("X-Request-Id".to_string(), "req-1".to_string());
    h.insert("User-Agent".to_string(), "wyrd/1.0".to_string());
    let c = WebhookConfig {
        headers: h,
        ..base_config()
    };
    assert!(c.validate().is_ok());
}

#[test]
fn webhook_headers_rejects_signature_header_collision() {
    let mut h = BTreeMap::new();
    h.insert("X-Wyrd-Signature".to_string(), "v1:abc".to_string());
    let c = WebhookConfig {
        headers: h,
        ..base_config()
    };
    assert!(
        c.validate()
            .unwrap_err()
            .to_string()
            .contains("collides with the signing or timestamp header")
    );
}

#[test]
fn webhook_headers_rejects_signature_header_collision_case_insensitive() {
    let mut h = BTreeMap::new();
    h.insert("x-wyrd-signature".to_string(), "v1:abc".to_string());
    let c = WebhookConfig {
        headers: h,
        ..base_config()
    };
    assert!(c.validate().is_err());
}

#[test]
fn webhook_headers_rejects_timestamp_header_collision() {
    let mut h = BTreeMap::new();
    h.insert("X-Wyrd-Timestamp".to_string(), "1700000000".to_string());
    let c = WebhookConfig {
        headers: h,
        ..base_config()
    };
    assert!(c.validate().is_err());
}

#[test]
fn webhook_headers_collisions_use_configured_header_names() {
    let mut h = BTreeMap::new();
    h.insert("X-Custom-Sig".to_string(), "v1:abc".to_string());
    let c = WebhookConfig {
        signature_header: "X-Custom-Sig".to_string(),
        headers: h,
        ..base_config()
    };
    assert!(c.validate().is_err());
}

#[test]
fn webhook_secret_headers_round_trips() {
    let mut sh = BTreeMap::new();
    sh.insert(
        "Authorization".to_string(),
        SecretRef::Env {
            name: "WYRD_WEBHOOK_BEARER".to_string(),
        },
    );
    let c = WebhookConfig {
        secret_headers: sh,
        ..base_config()
    };
    let s = serde_json::to_string(&c).unwrap();
    let back: WebhookConfig = serde_json::from_str(&s).unwrap();
    assert_eq!(c, back);
}

#[test]
fn webhook_secret_headers_default_empty_omitted_from_serialized_output() {
    let c = base_config();
    let s = serde_json::to_string(&c).unwrap();
    assert!(!s.contains("secret_headers"));
}

#[test]
fn webhook_secret_headers_allows_authorization() {
    let mut sh = BTreeMap::new();
    sh.insert(
        "Authorization".to_string(),
        SecretRef::Env {
            name: "WYRD_WEBHOOK_BEARER".to_string(),
        },
    );
    let c = WebhookConfig {
        secret_headers: sh,
        ..base_config()
    };
    assert!(c.validate().is_ok());
}

#[test]
fn webhook_secret_headers_rejects_signing_header_collision() {
    let mut sh = BTreeMap::new();
    sh.insert(
        "X-Wyrd-Signature".to_string(),
        SecretRef::Env {
            name: "WYRD_SIG".to_string(),
        },
    );
    let c = WebhookConfig {
        secret_headers: sh,
        ..base_config()
    };
    assert!(
        c.validate()
            .unwrap_err()
            .to_string()
            .contains("collides with the signing or timestamp header")
    );
}

#[test]
fn webhook_secret_headers_rejects_timestamp_header_collision() {
    let mut sh = BTreeMap::new();
    sh.insert(
        "X-Wyrd-Timestamp".to_string(),
        SecretRef::Env {
            name: "WYRD_TS".to_string(),
        },
    );
    let c = WebhookConfig {
        secret_headers: sh,
        ..base_config()
    };
    assert!(c.validate().is_err());
}

#[test]
fn webhook_rejects_duplicate_key_across_headers_and_secret_headers() {
    let mut h = BTreeMap::new();
    h.insert("X-App-Name".to_string(), "wyrd".to_string());
    let mut sh = BTreeMap::new();
    sh.insert(
        "x-app-name".to_string(),
        SecretRef::Env {
            name: "WYRD_APP".to_string(),
        },
    );
    let c = WebhookConfig {
        headers: h,
        secret_headers: sh,
        ..base_config()
    };
    assert!(
        c.validate()
            .unwrap_err()
            .to_string()
            .contains("appears in both")
    );
}

#[test]
fn webhook_secret_headers_with_secret_ref_round_trips_alongside_static_headers() {
    let mut h = BTreeMap::new();
    h.insert("Content-Type".to_string(), "application/json".to_string());
    let mut sh = BTreeMap::new();
    sh.insert(
        "Authorization".to_string(),
        SecretRef::Vault {
            key: "secret/data/wyrd/webhook".to_string(),
        },
    );
    let c = WebhookConfig {
        headers: h,
        secret_headers: sh,
        ..base_config()
    };
    assert!(c.validate().is_ok());
    let s = serde_json::to_string(&c).unwrap();
    assert!(s.contains("secret_headers"));
    assert!(s.contains("Authorization"));
    let back: WebhookConfig = serde_json::from_str(&s).unwrap();
    assert_eq!(c, back);
}
