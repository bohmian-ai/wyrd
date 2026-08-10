//! Public write / scribe / forge user-journey binary.
//!
//! Surface-scoped test target carved out of the former monolithic `integration`
//! binary (T54). It aggregates the public ingest, compaction, gRPC-mount,
//! event-time, GenAI-derivation, owner-inspection, and server-smoke journeys so
//! that editing any one of them relinks only this binary instead of every
//! Bifrost test in the crate. Each module below points at an existing test file
//! via `#[path]`; the files are unchanged, so every test keeps its name and
//! `#[ignore]` gating.

#[path = "bifrost_owner_inspection.rs"]
mod bifrost_owner_inspection;
#[path = "forge_journeys.rs"]
mod forge_journeys;
#[path = "genai_derivation_journey.rs"]
mod genai_derivation_journey;
#[path = "genai_derivation_recovery.rs"]
mod genai_derivation_recovery;
#[path = "pg_event_time_window_journey.rs"]
mod pg_event_time_window_journey;
#[path = "pg_grpc_mount.rs"]
mod pg_grpc_mount;
#[path = "public_write_journeys.rs"]
mod public_write_journeys;
#[path = "test_server_smoke.rs"]
mod test_server_smoke;
