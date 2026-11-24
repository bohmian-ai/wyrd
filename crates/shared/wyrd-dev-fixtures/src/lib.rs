//! Development fixture crate.
//!
//! SQL fixtures are feature-gated so non-Postgres fixture users do not pull in
//! embedded database dependencies.

pub mod google;

#[cfg(feature = "pg")]
pub mod pg;

#[cfg(feature = "pg")]
pub mod cards;
