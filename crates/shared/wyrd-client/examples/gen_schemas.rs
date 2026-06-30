//! Canonical schema generator for `wyrd-client` public types.
//!
//! Writes BOTH `crates/shared/wyrd-client/schemas/<name>.json` (published
//! snapshot) and `crates/shared/wyrd-client/tests/schemas/<name>.json` (test
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

    use wyrd_client::transport::{GrpcConfig, HttpConfig, MockConfig, TransportConfig};

    write::<TransportConfig>(&out, &golden, "transport_config_enum")?;
    write::<GrpcConfig>(&out, &golden, "transport_config_grpc")?;
    write::<HttpConfig>(&out, &golden, "transport_config_http")?;
    write::<MockConfig>(&out, &golden, "transport_config_mock")?;
    Ok(())
}
