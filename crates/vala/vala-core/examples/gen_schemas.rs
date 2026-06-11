//! Canonical schema generator for `vala-core` public schema-bearing types.

use std::fs;
use std::path::Path;

use schemars::schema_for;
use vala_core::alert_router::webhook::{WebhookConfig, WebhookMethod};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let out = manifest.join("schemas");
    let golden = manifest.join("tests/schemas");
    fs::create_dir_all(&out)?;
    fs::create_dir_all(&golden)?;

    write::<WebhookConfig>(&out, &golden, "alert_router_webhook_config")?;
    write::<WebhookMethod>(&out, &golden, "alert_router_webhook_method")?;
    Ok(())
}

fn write<T: schemars::JsonSchema>(
    out: &Path,
    golden: &Path,
    name: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut schema = schema_for!(T);
    schema.meta_schema = Some("https://json-schema.org/draft/2020-12/schema".to_string());
    let json = serde_json::to_string_pretty(&schema)?;
    let body = format!("{json}\n");
    fs::write(out.join(format!("{name}.json")), &body)?;
    fs::write(golden.join(format!("{name}.json")), &body)?;
    Ok(())
}
