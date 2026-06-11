//! Schema drift guard for `wyrd-spec::security`.

use std::fs;

use schemars::schema_for;

const META_SCHEMA: &str = "https://json-schema.org/draft/2020-12/schema";

fn generate_normalized<T: schemars::JsonSchema>() -> String {
    let mut schema = schema_for!(T);
    schema.meta_schema = Some(META_SCHEMA.to_string());
    let json = serde_json::to_string_pretty(&schema).expect("schema serializes");
    format!("{json}\n")
}

fn load_golden(name: &str) -> String {
    let path = format!("{}/tests/schemas/{name}.json", env!("CARGO_MANIFEST_DIR"));
    fs::read_to_string(&path).unwrap_or_else(|error| {
        panic!(
            "golden missing: {path}: {error}\n\
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
