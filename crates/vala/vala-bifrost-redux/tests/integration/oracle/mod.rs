//! Tier-2 `oracle` integration modules of the `integration` binary.
//!
//! See `../main.rs` for the tier this binary occupies and
//! `../../README.md` for the rule that decides whether a test belongs
//! here or in `wyrd-testing`.

mod audit;
mod decoder;
mod distributed;
mod readiness;
mod resource_release;
mod semantics;
mod spill;
mod support;
