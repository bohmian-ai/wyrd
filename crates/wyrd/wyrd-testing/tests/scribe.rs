//! Scribe capability binary: the durable write path.
//!
//! Proves public write and acknowledgement, shard rotation and seal, WAL
//! replay, exactly-once behavior across restart, the event-time admission
//! window, and the OTLP source-boundary durability slice that publishes source
//! Parquet and `vala.file_list` bookkeeping without an Iceberg snapshot.
//!
//! Setup: Postgres, and a booted `WyrdTestServer` for the journeys that drive
//! the public ingest surface.
//!
//! Owning lane: `mise run test:bifrost:journey`, which runs this binary whole
//! with `--include-ignored --test-threads=1`. A test added to a module below
//! runs in that lane with no `mise.toml` edit.
//!
//! Out of scope: compaction and maintenance (`forge`), Oracle reads
//! (`oracle`), the OTLP export protocol surface itself (`otlp`), multi-pod
//! topology (`cluster`), and server boot and mount (`server`).

#[path = "pg_event_time_window_journey.rs"]
mod pg_event_time_window_journey;
#[path = "public_write_journeys.rs"]
mod public_write_journeys;
#[path = "scribe_source_boundary_journey.rs"]
mod scribe_source_boundary_journey;
#[path = "scribe_source_boundary_recovery.rs"]
mod scribe_source_boundary_recovery;
