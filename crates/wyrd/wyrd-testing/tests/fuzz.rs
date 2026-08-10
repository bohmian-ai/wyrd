//! Interleaving / replay fuzz binary.
//!
//! Surface-scoped test target carved out of the former monolithic `integration`
//! binary (T54). It isolates the two large Forge interleaving suites
//! (`forge_compaction_interleaving`, `forge_maintenance_interleaving`) plus the
//! Bifrost interleaving replay and the interleaving smoke test — the
//! `test:fuzz:bifrost` lane. Grouping the heavy interleaving files here keeps a
//! single edit from relinking the journey binaries. Modules point at unchanged
//! files via `#[path]`, so every test name and `#[ignore]` gate is retained.

#[path = "bifrost_interleavings.rs"]
mod bifrost_interleavings;
#[path = "forge_compaction_interleaving.rs"]
mod forge_compaction_interleaving;
#[path = "forge_maintenance_interleaving.rs"]
mod forge_maintenance_interleaving;
#[path = "interleaving_smoke.rs"]
mod interleaving_smoke;
