use wyrd_spec::security::SecretRef;

#[test]
fn secret_ref_env_round_trip() {
    let secret_ref = SecretRef::Env {
        name: "WYRD_API_KEY".to_string(),
    };
    let serialized = serde_json::to_string(&secret_ref).expect("secret ref serializes");
    assert_eq!(serialized, r#"{"source":"env","name":"WYRD_API_KEY"}"#);
    let back: SecretRef = serde_json::from_str(&serialized).expect("secret ref deserializes");
    assert_eq!(secret_ref, back);
}

#[test]
fn secret_ref_file_round_trip() {
    let secret_ref = SecretRef::File {
        path: "/var/run/secrets/key".to_string(),
    };
    let serialized = serde_json::to_string(&secret_ref).expect("secret ref serializes");
    let back: SecretRef = serde_json::from_str(&serialized).expect("secret ref deserializes");
    assert_eq!(secret_ref, back);
}

#[test]
fn secret_ref_vault_round_trip() {
    let secret_ref = SecretRef::Vault {
        key: "secret/data/wyrd/api".to_string(),
    };
    let serialized = serde_json::to_string(&secret_ref).expect("secret ref serializes");
    let back: SecretRef = serde_json::from_str(&serialized).expect("secret ref deserializes");
    assert_eq!(secret_ref, back);
}

#[test]
fn secret_ref_rejects_unknown_source() {
    let result: Result<SecretRef, _> = serde_json::from_str(r#"{"source":"s3","bucket":"foo"}"#);
    assert!(result.is_err());
}

#[cfg(feature = "test-utils")]
#[test]
fn secret_ref_inline_round_trip() {
    use wyrd_spec::security::InlineSecret;

    let secret_ref = SecretRef::Inline {
        value: InlineSecret::new("supersecret"),
    };
    let serialized = serde_json::to_string(&secret_ref).expect("secret ref serializes");
    assert_eq!(serialized, r#"{"source":"inline","value":"supersecret"}"#);
    let back: SecretRef = serde_json::from_str(&serialized).expect("secret ref deserializes");
    assert_eq!(secret_ref, back);
}

#[cfg(feature = "test-utils")]
#[test]
fn inline_secret_debug_is_redacted() {
    use wyrd_spec::security::InlineSecret;

    let secret = InlineSecret::new("supersecret");
    let debug = format!("{secret:?}");
    assert!(debug.contains("<redacted>"));
    assert!(!debug.contains("supersecret"));
}
