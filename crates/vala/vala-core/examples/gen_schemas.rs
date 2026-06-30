//! Canonical schema generator for `vala-core` public-ish config types.
//!
//! Writes BOTH `crates/vala/vala-core/schemas/<name>.json` (published
//! snapshot) and `crates/vala/vala-core/tests/schemas/<name>.json` (test
//! golden) per type. Invoked by `mise codegen:regen`.

use std::path::Path;

fn write<T: schemars::JsonSchema>(out: &Path, golden: &Path, name: &str) -> std::io::Result<()> {
    let mut schema = schemars::schema_for!(T);
    schema.meta_schema = Some("https://json-schema.org/draft/2020-12/schema".to_string());
    let json = serde_json::to_string_pretty(&schema)
        .expect("schema serialization should be infallible for generated contracts");
    let body = format!("{json}\n");
    std::fs::write(out.join(format!("{name}.json")), &body)?;
    std::fs::write(golden.join(format!("{name}.json")), &body)?;
    Ok(())
}

fn main() -> std::io::Result<()> {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let out = manifest.join("schemas");
    let golden = manifest.join("tests/schemas");
    std::fs::create_dir_all(&out)?;
    std::fs::create_dir_all(&golden)?;

    use vala_core::alert_router::webhook::{WebhookConfig, WebhookMethod};

    write::<WebhookConfig>(&out, &golden, "alert_router_webhook_config")?;
    write::<WebhookMethod>(&out, &golden, "alert_router_webhook_method")?;
    Ok(())
}
