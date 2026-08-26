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
//!
//! # Module layout
//!
//! Sources are grouped by the subsystem they exercise —
//! `catalog/`, `forge/`, `oracle/`, `scribe/` — each an ordinary
//! directory module of this binary. `support` holds fixtures shared across
//! more than one of those groups and contains no tests.

mod support;

mod catalog;
mod forge;
mod oracle;
mod scribe;
