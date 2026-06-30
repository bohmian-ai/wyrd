use wyrd_spec::security::SecretRef;

#[test]
fn secret_ref_env_round_trip() {
    let r = SecretRef::Env {
        name: "WYRD_API_KEY".to_string(),
    };
    let s = serde_json::to_string(&r).unwrap();
    assert_eq!(s, r#"{"source":"env","name":"WYRD_API_KEY"}"#);
    let back: SecretRef = serde_json::from_str(&s).unwrap();
    assert_eq!(r, back);
}

#[test]
fn secret_ref_file_round_trip() {
    let r = SecretRef::File {
        path: "/var/run/secrets/key".to_string(),
    };
    let s = serde_json::to_string(&r).unwrap();
    let back: SecretRef = serde_json::from_str(&s).unwrap();
    assert_eq!(r, back);
}

#[test]
fn secret_ref_vault_round_trip() {
    let r = SecretRef::Vault {
        key: "secret/data/wyrd/api".to_string(),
    };
    let s = serde_json::to_string(&r).unwrap();
    let back: SecretRef = serde_json::from_str(&s).unwrap();
    assert_eq!(r, back);
}

#[test]
fn secret_ref_rejects_unknown_source() {
    let r: Result<SecretRef, _> = serde_json::from_str(r#"{"source":"s3","bucket":"foo"}"#);
    assert!(r.is_err());
}

// IMPORTANT: integration-test files build with `cfg(test)` ALWAYS on (even when
// `test-utils` is off). The inline-related items are gated on
// `feature = "test-utils"` in the library, so these tests MUST be gated on
// `feature = "test-utils"` ONLY — not `any(test, feature = "test-utils")` —
// or the no-`test-utils` build (`cargo test -p wyrd-spec --no-default-features
// --features server --tests security`, used by `check:security-inline-gate`)
// will try to compile references to `InlineSecret` / `SecretRef::Inline` that
// the library does not export.

#[cfg(feature = "test-utils")]
#[test]
fn secret_ref_inline_round_trip() {
    use wyrd_spec::security::InlineSecret;
    let r = SecretRef::Inline {
        value: InlineSecret::new("supersecret"),
    };
    let s = serde_json::to_string(&r).unwrap();
    // InlineSecret serializes the inner string transparently — the wire
    // payload reads `{"source":"inline","value":"supersecret"}`.
    assert_eq!(s, r#"{"source":"inline","value":"supersecret"}"#);
    let back: SecretRef = serde_json::from_str(&s).unwrap();
    // PartialEq is hand-implemented (compares via redacted Debug); the
    // round-trip is equality-preserving.
    assert_eq!(r, back);
}

#[cfg(feature = "test-utils")]
#[test]
fn inline_secret_debug_is_redacted() {
    use wyrd_spec::security::InlineSecret;
    let s = InlineSecret::new("supersecret");
    let dbg = format!("{:?}", s);
    assert!(
        dbg.contains("<redacted>"),
        "Debug must redact plaintext, got {dbg}"
    );
    assert!(
        !dbg.contains("supersecret"),
        "plaintext leaked to Debug output: {dbg}"
    );
}
