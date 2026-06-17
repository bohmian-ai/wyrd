//! Semver primitives shared across Wyrd crates.
//!
//! Owns the version-value newtypes ([`VersionBlock`], [`VersionRange`]),
//! the bump enum ([`VersionBump`]), the SQL-pushdown bounds shape
//! ([`VersionBounds`], [`SemverTriple`]), and the shared error type
//! ([`VersionError`]).
//!
//! [`crate::block::VersionBlock`]: VersionBlock

mod block;
mod bounds;
mod bump;
mod error;
mod range;

pub use crate::block::VersionBlock;
pub use crate::bounds::{SemverTriple, VersionBounds};
pub use crate::bump::VersionBump;
pub use crate::error::VersionError;
pub use crate::range::VersionRange;
