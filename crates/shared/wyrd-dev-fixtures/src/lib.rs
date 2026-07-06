//! Development fixture crate.
//!
//! SQL fixtures are feature-gated so non-Postgres fixture users do not pull in
//! embedded database dependencies.

#[cfg(feature = "pg")]
pub mod pg;

#[cfg(feature = "pg")]
pub mod cards;
