//! Interleavings capability binary: deterministic scheduler permutations.
//!
//! Proves the Bifrost concurrency contracts that a forced schedule can decide
//! on its own — the deterministic `Scheduler` permutation replays and their
//! smoke coverage. These assertions are about ordering, not durability.
//!
//! Setup: none. No Postgres, no server, no fixtures beyond the in-process
//! scheduler harness. Nothing here is `#[ignore]`d, so this binary runs
//! unignored in the fast `test:wyrd` and `test:e2e` family lanes.
//!
//! Owning lane: the fast family lanes above run it by default;
//! `mise run test:bifrost:journey` also names it so the Bifrost lane covers
//! every regrouped binary. A test added to a module below runs in both with no
//! `mise.toml` edit — but keep it Postgres-free, or it belongs in `forge`.
//!
//! Out of scope: every database-backed interleaving replay. The Forge
//! compaction and maintenance interleavings need Postgres and live in `forge`.

#[path = "bifrost_interleavings.rs"]
mod bifrost_interleavings;
#[path = "interleaving_smoke.rs"]
mod interleaving_smoke;
