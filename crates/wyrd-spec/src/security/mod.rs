//! Durable shared security primitives.
//!
//! `TlsConfig` and `SecretRef` are consumed by both the external client
//! (`wyrd-client::transport`) and the server-internal alert router
//! (`vala-core::alert_router`). Putting them in `wyrd-spec::security` gives
//! both consumers one durable, schema-bearing definition without a
//! cross-crate dependency cycle.

pub mod secret_ref;
pub mod tls;

#[cfg(any(test, feature = "test-utils"))]
pub use secret_ref::InlineSecret;
pub use secret_ref::{SecretRef, SecretRefError};
pub use tls::TlsConfig;

#[cfg(test)]
#[cfg(not(feature = "test-utils"))]
mod schema_drift_tests {
    //! Schema drift guard for `wyrd-spec::security`.
    //!
    //! Regenerates each type's JSON schema and compares against the golden files
    //! committed under `crates/wyrd-spec/tests/schemas/`. A drift means a public
    //! wire contract changed; updating the golden is a deliberate sign-off.
    //!
    //! To regenerate goldens (writes both `schemas/` and `tests/schemas/`):
    //!   cargo run --locked -p wyrd-spec --example gen_schemas --features server
    //!
    //! `test-utils` MUST be off during schema generation. This module is
    //! `cfg(not(feature = "test-utils"))`-gated at the `mod`-mount site in
    //! `tests/security.rs`, so it only ever compiles in the no-`test-utils`
    //! build. The no-`test-utils` gate test
    //! (`inline_rejected_without_test_utils.rs`) proves the variant is absent at
    //! the type level when the feature is off.

    use schemars::schema_for;
    use std::fs;

    const META_SCHEMA: &str = "https://json-schema.org/draft/2020-12/schema";

    fn generate_normalized<T: schemars::JsonSchema>() -> String {
        let mut schema = schema_for!(T);
        schema.meta_schema = Some(META_SCHEMA.to_string());
        let json = serde_json::to_string_pretty(&schema).unwrap();
        format!("{json}\n")
    }

    fn load_golden(name: &str) -> String {
        let path = format!("{}/tests/schemas/{}.json", env!("CARGO_MANIFEST_DIR"), name);
        fs::read_to_string(&path).unwrap_or_else(|_| {
            panic!(
                "golden missing: {path}\n\
                 Run: cargo run --locked -p wyrd-spec --example gen_schemas --features server"
            )
        })
    }

    fn assert_schema_matches<T: schemars::JsonSchema>(golden_name: &str) {
        let generated = generate_normalized::<T>();
        let golden = load_golden(golden_name);
        assert_eq!(generated, golden, "schema drift: {golden_name}");
    }

    #[test]
    fn secret_ref_schema_matches_golden() {
        use crate::security::SecretRef;
        assert_schema_matches::<SecretRef>("security_secret_ref");
    }

    #[test]
    fn tls_config_schema_matches_golden() {
        use crate::security::TlsConfig;
        assert_schema_matches::<TlsConfig>("security_tls_config");
    }
}

#[cfg(test)]
mod secret_ref_tests {
    use crate::security::SecretRef;

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
        use crate::security::InlineSecret;
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
        use crate::security::InlineSecret;
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
}

#[cfg(test)]
mod tls_tests {
    use crate::security::{SecretRef, TlsConfig};

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
}
