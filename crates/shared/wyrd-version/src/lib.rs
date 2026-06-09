//! Shared version primitives for Wyrd crates.
//!
//! `WyrdVersion` is a thin newtype around `semver::Version` with serde,
//! schemars, and validation helpers. Cross-cutting concerns (version
//! bumping, prerelease/build identifier handling) live here so primitives
//! never carry their own ad-hoc semver code.

pub mod error;
pub mod version;

pub use error::WyrdVersionError;
pub use version::{VersionBump, WyrdVersion};
