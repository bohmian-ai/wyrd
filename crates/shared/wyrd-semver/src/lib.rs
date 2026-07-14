//! Semver primitives shared across Wyrd crates.
//!
//! Owns the version-value newtypes ([`VersionBlock`], [`VersionRange`]),
//! the wire-shape discriminator for the register path ([`VersionSpec`]),
//! the bump enum ([`VersionBump`]), the SQL-pushdown bounds shape
//! ([`VersionBounds`], [`SemverTriple`]), and the shared error type
//! ([`VersionError`]).

mod block;
mod bounds;
mod bump;
mod error;
mod range;
mod spec;

pub use crate::block::VersionBlock;
pub use crate::bounds::{SemverTriple, VersionBounds};
pub use crate::bump::VersionBump;
pub use crate::error::VersionError;
pub use crate::range::VersionRange;
pub use crate::spec::VersionSpec;

/// Initial seed version used when registering a card with no prior version in
/// the (kind, space, name) scope.
#[must_use]
pub fn seed_version() -> VersionBlock {
    VersionBlock::parse("0.1.0").expect("seed version literal is a valid semver triple")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seed_version_is_zero_one_zero() {
        assert_eq!(seed_version().as_str(), "0.1.0");
    }
}
