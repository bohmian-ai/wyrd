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
    use wyrd_spec::security::SecretRef;
    assert_schema_matches::<SecretRef>("security_secret_ref");
}

#[test]
fn tls_config_schema_matches_golden() {
    use wyrd_spec::security::TlsConfig;
    assert_schema_matches::<TlsConfig>("security_tls_config");
}
