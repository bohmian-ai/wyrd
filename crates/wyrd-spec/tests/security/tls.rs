use wyrd_spec::security::{SecretRef, TlsConfig};

#[test]
fn tls_config_default_shape_serializes_options_as_absent() {
    let tls = TlsConfig {
        ca_cert: None,
        client_cert: None,
        client_key: None,
        server_name_override: None,
        insecure_skip_verify: false,
    };
    let serialized = serde_json::to_string(&tls).expect("tls serializes");
    assert_eq!(serialized, r#"{"insecure_skip_verify":false}"#);
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
    let serialized = serde_json::to_string(&tls).expect("tls serializes");
    let back: TlsConfig = serde_json::from_str(&serialized).expect("tls deserializes");
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
    let serialized = serde_json::to_string(&tls).expect("tls serializes");
    let back: TlsConfig = serde_json::from_str(&serialized).expect("tls deserializes");
    assert_eq!(tls, back);
}
