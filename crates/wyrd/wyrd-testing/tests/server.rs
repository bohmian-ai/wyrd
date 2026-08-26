//! Server capability binary: boot, mount, and Bifrost owner lifecycle.
//!
//! Proves that a `WyrdTestServer` boots and shuts down cleanly, that the
//! Bifrost gRPC services mount on the public surface, and that the production
//! recorder exposes the expected Bifrost owner families once owners are
//! running.
//!
//! Setup: Postgres plus a booted server.
//!
//! Owning lane: `mise run test:bifrost:journey`, which runs this binary whole
//! with `--include-ignored --test-threads=1`. A test added to a module below
//! runs in that lane with no `mise.toml` edit.
//!
//! Out of scope: what the mounted services then do — the write path
//! (`scribe`), reads (`oracle`), compaction (`forge`), OTLP ingest (`otlp`),
//! and multi-pod topology (`cluster`).

#[path = "bifrost_owner_inspection.rs"]
mod bifrost_owner_inspection;
#[path = "pg_grpc_mount.rs"]
mod pg_grpc_mount;
#[path = "test_server_smoke.rs"]
mod test_server_smoke;
