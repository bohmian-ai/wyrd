//! Canonical schema generator for `wyrd-client` public types.

use std::fs;
use std::path::Path;

use schemars::schema_for;
use wyrd_client::transport::{GrpcConfig, HttpConfig, MockConfig, QueueConfig, TransportConfig};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let out = manifest.join("schemas");
    let golden = manifest.join("tests/schemas");
    fs::create_dir_all(&out)?;
    fs::create_dir_all(&golden)?;

    write::<TransportConfig>(&out, &golden, "transport_config_enum")?;
    write::<GrpcConfig>(&out, &golden, "transport_config_grpc")?;
    write::<HttpConfig>(&out, &golden, "transport_config_http")?;
    write::<MockConfig>(&out, &golden, "transport_config_mock")?;
    write::<QueueConfig>(&out, &golden, "transport_queue_config")?;
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
