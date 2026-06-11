//! Schema drift guard for `wyrd_client::transport`.

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
             Run: cargo run --locked -p wyrd-client --example gen_schemas"
        )
    })
}

fn assert_schema_matches<T: schemars::JsonSchema>(golden_name: &str) {
    let generated = generate_normalized::<T>();
    let golden = load_golden(golden_name);
    assert_eq!(generated, golden, "schema drift: {golden_name}");
}

#[test]
fn transport_config_enum_schema_matches_golden() {
    use wyrd_client::transport::TransportConfig;

    assert_schema_matches::<TransportConfig>("transport_config_enum");
}

#[test]
fn grpc_config_schema_matches_golden() {
    use wyrd_client::transport::GrpcConfig;

    assert_schema_matches::<GrpcConfig>("transport_config_grpc");
}

#[test]
fn http_config_schema_matches_golden() {
    use wyrd_client::transport::HttpConfig;

    assert_schema_matches::<HttpConfig>("transport_config_http");
}

#[test]
fn mock_config_schema_matches_golden() {
    use wyrd_client::transport::MockConfig;

    assert_schema_matches::<MockConfig>("transport_config_mock");
}

#[test]
fn queue_config_schema_matches_golden() {
    use wyrd_client::transport::QueueConfig;

    assert_schema_matches::<QueueConfig>("transport_queue_config");
}
