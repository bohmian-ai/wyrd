//! Schema drift guard for `vala_core::alert_router::webhook`.

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
             Run: cargo run --locked -p vala-core --example gen_schemas"
        )
    })
}

fn assert_schema_matches<T: schemars::JsonSchema>(golden_name: &str) {
    let generated = generate_normalized::<T>();
    let golden = load_golden(golden_name);
    assert_eq!(generated, golden, "schema drift: {golden_name}");
}

#[test]
fn webhook_config_schema_matches_golden() {
    use vala_core::alert_router::webhook::WebhookConfig;

    assert_schema_matches::<WebhookConfig>("alert_router_webhook_config");
}

#[test]
fn webhook_method_schema_matches_golden() {
    use vala_core::alert_router::webhook::WebhookMethod;

    assert_schema_matches::<WebhookMethod>("alert_router_webhook_method");
}
