use wyrd_spec::security::{SecretRef, TlsConfig};

#[test]
fn tls_config_default_all_none() {
    let tls = TlsConfig {
        ca_cert: None,
        client_cert: None,
        client_key: None,
        server_name_override: None,
        insecure_skip_verify: false,
    };
    let s = serde_json::to_string(&tls).unwrap();
    // All Option::None fields skip serialization; insecure_skip_verify defaults false.
    assert_eq!(s, r#"{"insecure_skip_verify":false}"#);
}

#[test]
fn tls_config_with_ca_cert_round_trips() {
    let tls = TlsConfig {
        ca_cert: Some(SecretRef::File {
            path: "/etc/ssl/ca.pem".to_string(),
        }),
        client_cert: None,
        client_key: None,
        server_name_override: Some("wyrd-server".to_string()),
        insecure_skip_verify: false,
    };
    let s = serde_json::to_string(&tls).unwrap();
    let back: TlsConfig = serde_json::from_str(&s).unwrap();
    assert_eq!(tls, back);
}

#[test]
fn tls_config_insecure_skip_verify_round_trips() {
    let tls = TlsConfig {
        ca_cert: None,
        client_cert: None,
        client_key: None,
        server_name_override: None,
        insecure_skip_verify: true,
    };
    let s = serde_json::to_string(&tls).unwrap();
    let back: TlsConfig = serde_json::from_str(&s).unwrap();
    assert_eq!(tls, back);
}
