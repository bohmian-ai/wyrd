#!/usr/bin/env bash
# Fail if the checked-in wyrd.v1 FileDescriptorSet drifts from the .proto.
set -e
# Rebuilding wyrd-tonic reruns build.rs, which regenerates the descriptor
# snapshot at crates/wyrd/wyrd-tonic/proto/wyrd.v1.bin from the .proto. Force a
# fresh compile of the build script so a stale target cache cannot mask drift.
# WYRD_PROTO_SNAPSHOT=1 makes build.rs write the snapshot into the source tree
# (normal builds emit it to OUT_DIR, so they never dirty the crate fingerprint).
touch crates/wyrd/wyrd-tonic/build.rs
WYRD_PROTO_SNAPSHOT=1 cargo build --locked -p wyrd-tonic --all-features
git diff --exit-code -- crates/wyrd/wyrd-tonic/proto/wyrd.v1.bin \
  || { echo 'wyrd.v1 descriptor drift detected - rebuild wyrd-tonic and commit crates/wyrd/wyrd-tonic/proto/wyrd.v1.bin'; exit 1; }
