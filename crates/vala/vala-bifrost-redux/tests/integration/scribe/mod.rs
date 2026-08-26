//! Tier-2 `scribe` integration modules of the `integration` binary.
//!
//! See `../main.rs` for the tier this binary occupies and
//! `../../README.md` for the rule that decides whether a test belongs
//! here or in `wyrd-testing`.

mod atomicity;
mod bounds;
mod commit_hints;
mod governor_gauges;
mod idempotent;
mod ordering;
mod persistence_support;
mod registry;
mod replay;
mod seal;
mod tail_rpc;
