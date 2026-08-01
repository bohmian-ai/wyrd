//! Compile the `wyrd.v1` protobuf surface. The `FileDescriptorSet` is emitted to
//! `OUT_DIR` on normal builds; the `check:proto-drift` gate sets
//! `WYRD_PROTO_SNAPSHOT=1` to refresh the checked-in `proto/wyrd.v1.bin` snapshot.
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

    // Default: emit the descriptor into OUT_DIR. Writing it into the source tree
    // (proto/) on every build bumps the crate's mtime, so cargo refingerprints
    // wyrd-tonic and recompiles it plus every dependent (wyrd-server, wyrd-client,
    // vala-ingest, ...) on each invocation. The checked-in snapshot at
    // proto/wyrd.v1.bin is only consumed by the check:proto-drift gate, which sets
    // WYRD_PROTO_SNAPSHOT=1 to refresh it for its git-diff comparison.
    let descriptor = if std::env::var_os("WYRD_PROTO_SNAPSHOT").is_some() {
        proto_dir.join("wyrd.v1.bin")
    } else {
        PathBuf::from(std::env::var("OUT_DIR")?).join("wyrd.v1.bin")
    };

    let build_server = std::env::var_os("CARGO_FEATURE_SERVER").is_some();
    let build_client = std::env::var_os("CARGO_FEATURE_CLIENT").is_some();

    tonic_prost_build::configure()
        .build_server(build_server)
        .build_client(build_client)
        .bytes(".wyrd.v1.InsertBatchRequest.arrow_ipc")
        .bytes(".wyrd.v1.InsertBatchRequest.wyrd_batch_id")
        .bytes(".wyrd.v1.InsertBatchResponse.wyrd_batch_id")
        .file_descriptor_set_path(&descriptor)
        .compile_protos(&[proto], &[proto_dir])?;

    println!("cargo:rerun-if-changed=proto/wyrd.v1.proto");
    println!("cargo:rerun-if-env-changed=WYRD_PROTO_SNAPSHOT");
    Ok(())
}
