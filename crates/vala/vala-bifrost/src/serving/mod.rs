//! Serving layer for the Bifrost OLAP warehouse.
//!
//! This layer owns query serving and projection matching/rewriting.
//!
//! - `projections` — projection matching + read-time rewrite (slice 04)
//! - `session` — query session / context builder (slice 04)

pub mod projections;
pub mod session;
