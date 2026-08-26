//! Tier-2 integration surface for `vala-bifrost-redux`.
//!
//! Every test here links this crate directly and drives one subsystem against
//! its real dependency — Postgres, the object store, an in-process dispatcher —
//! without booting a Wyrd server. That is the tier's defining constraint and
//! also its reason to exist: `vala-bifrost-redux` is a dependency of
//! `wyrd-server`, so nothing here *can* stand up a server without a dependency
//! cycle, and much of what these tests assert (physical-plan shape, participant
//! cut selection, dispatcher attempt accounting, decoder partial retention) has
//! no wire representation to observe through a server API.
//!
//! A test that needs a booted server and the real SDK is a tier-1 user journey
//! and belongs in `wyrd-testing/tests`, not here. See
//! `crates/wyrd/wyrd-testing/tests/README.md` for that topology and
//! `tests/README.md` beside this file for the rule that governs this one.
//!
//! Owning lane: `mise run test:bifrost`, which runs this binary — and the
//! crate's lib and doc tests — whole at the canonical test feature union.
//! Adding a module below therefore needs no `mise.toml` edit.

#[path = "control_row_fixture.rs"]
mod control_row_fixture;
#[path = "forge_incremental_compaction.rs"]
mod forge_incremental_compaction;
#[path = "immutable_tier_retention.rs"]
mod immutable_tier_retention;
#[path = "mitari_rewrite_api.rs"]
mod mitari_rewrite_api;
#[path = "oracle_core.rs"]
mod oracle_core;
#[path = "pg_file_list_tenant_table.rs"]
mod pg_file_list_tenant_table;
#[path = "pg_scribe_governor_gauges.rs"]
mod pg_scribe_governor_gauges;
#[path = "pg_scribe_idempotent.rs"]
mod pg_scribe_idempotent;
#[path = "pg_scribe_persistence.rs"]
mod pg_scribe_persistence;
#[path = "pg_scribe_registry.rs"]
mod pg_scribe_registry;
#[path = "pg_scribe_seal.rs"]
mod pg_scribe_seal;
#[path = "tail_rpc.rs"]
mod tail_rpc;
