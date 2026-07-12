#!/usr/bin/env bash
# WHY THIS FILE EXISTS: the checked-in wyrd.v1.bin FileDescriptorSet is used
# for gRPC server reflection (grpcurl, Postman, evans) and MCP tool schema
# generation. If the .proto changes without regenerating the snapshot, the
# reflection API and the actual service diverge: clients see stale field names
# and types, tooling generates wrong schemas, and the mismatch is invisible
# until a client call fails at runtime. Catching drift at CI time costs one
# build; catching it in production costs an incident.
#
# WHAT IT CHECKS: forces a fresh build.rs run with WYRD_PROTO_SNAPSHOT=1,
# which writes the generated descriptor into the source tree, then fails if
# git reports a diff against the committed snapshot.
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
