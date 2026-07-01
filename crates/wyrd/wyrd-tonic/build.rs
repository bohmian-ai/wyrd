//! Compile the `wyrd.v1` protobuf surface and emit a checked-in
//! `FileDescriptorSet` snapshot for the `check:proto-drift` gate.
//!
//! Server vs client codegen is gated on this crate's own cargo features
//! (`CARGO_FEATURE_SERVER` / `CARGO_FEATURE_CLIENT`), so a consumer that enables
//! neither (e.g. `wyrd-client`) compiles only the message structs and never
//! pulls the tonic server stack. The descriptor snapshot is independent of those
//! features — it is the parsed proto — so the drift gate is stable across
//! feature sets.

use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR")?);
    let proto_dir = manifest.join("proto");
    let proto = proto_dir.join("wyrd.v1.proto");
    let descriptor = proto_dir.join("wyrd.v1.bin");

    let build_server = std::env::var_os("CARGO_FEATURE_SERVER").is_some();
    let build_client = std::env::var_os("CARGO_FEATURE_CLIENT").is_some();

    tonic_build::configure()
        .build_server(build_server)
        .build_client(build_client)
        .file_descriptor_set_path(&descriptor)
        .compile_protos(&[proto], &[proto_dir])?;

    println!("cargo:rerun-if-changed=proto/wyrd.v1.proto");
    Ok(())
}
