//! Serving layer for the Bifrost OLAP warehouse.
//!
//! This layer owns the maintenance workers (`repair`), query serving,
//! projection matching/rewriting, admission control, result caching, and
//! async job management. Sub-modules are introduced slice-by-slice:
//!
//! - `repair` — maintenance sweep workers (slice 01+)
//! - `projections` — projection matching + read-time rewrite (slice 04)
//! - `admission` — query admission control (slice 06)
//! - `async_jobs` — async query worker (slice 07)
//! - `derivations` — cross-table derivation worker (slice 05b)

pub mod repair;
