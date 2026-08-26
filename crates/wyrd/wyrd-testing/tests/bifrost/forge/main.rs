//! Forge capability binary: compaction, maintenance, and worker lifecycle.
//!
//! Proves the Forge side of Bifrost — compaction planning and rewrite,
//! maintenance and lease reclaim, publication recovery, worker lifecycle, and
//! concurrent compaction ordering — including the deterministic interleaving
//! replays that drive those paths through forced schedules.
//!
//! Setup: Postgres. Most tests here are `#[ignore]`d so they stay out of the
//! fast lane and run serialized against a live database.
//!
//! Owning lane: `mise run test:bifrost:journey`, which runs this binary whole
//! with `--include-ignored --test-threads=1`. A test added to a module below
//! runs in that lane with no `mise.toml` edit.
//!
//! Out of scope: the Scribe write path (`scribe`), Oracle reads (`oracle`),
//! OTLP ingest (`otlp`), multi-pod topology (`cluster`), server boot and mount
//! (`server`), and the Postgres-free scheduler permutations (`interleavings`).

mod compaction_interleaving;
mod journeys;
mod maintenance_interleaving;
