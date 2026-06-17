//! SQL-pushdown bounds for semver range queries.

use std::fmt;

/// A lexicographically-comparable (major, minor, patch) triple.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SemverTriple {
    /// Major version component.
    pub major: u64,
    /// Minor version component.
    pub minor: u64,
    /// Patch version component.
    pub patch: u64,
}

impl SemverTriple {
    /// The (0, 0, 0) origin, used as the lower bound for an unbounded range.
    pub const ZERO: Self = Self {
        major: 0,
        minor: 0,
        patch: 0,
    };

    /// Construct a triple.
    #[must_use]
    pub const fn new(major: u64, minor: u64, patch: u64) -> Self {
        Self {
            major,
            minor,
            patch,
        }
    }
}

impl fmt::Display for SemverTriple {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

impl PartialOrd for SemverTriple {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for SemverTriple {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.major
            .cmp(&other.major)
            .then_with(|| self.minor.cmp(&other.minor))
            .then_with(|| self.patch.cmp(&other.patch))
    }
}

/// Inclusive-lower, optionally-bounded-upper semver range.
///
/// Maps a [`crate::VersionRange`] to a shape suitable for SQL predicates of
/// the form `(major, minor, patch) >= lower AND (major, minor, patch) < upper`
/// (or `<=` when `upper_inclusive` is true).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct VersionBounds {
    /// Inclusive lower bound.
    pub(crate) lower: SemverTriple,
    /// Upper bound; `None` means unbounded above.
    pub(crate) upper: Option<SemverTriple>,
    /// When true, `upper` is inclusive (used for exact pins).
    pub(crate) upper_inclusive: bool,
}

impl VersionBounds {
    /// Construct a bounds value.
    ///
    /// `upper_inclusive` must be false when `upper` is `None`.
    #[must_use]
    pub fn new(lower: SemverTriple, upper: Option<SemverTriple>, upper_inclusive: bool) -> Self {
        debug_assert!(
            upper.is_some() || !upper_inclusive,
            "upper_inclusive must be false when upper is None"
        );
        Self {
            lower,
            upper,
            upper_inclusive,
        }
    }

    /// Inclusive lower bound.
    #[must_use]
    pub fn lower(&self) -> SemverTriple {
        self.lower
    }

    /// Upper bound; `None` means unbounded above.
    #[must_use]
    pub fn upper(&self) -> Option<SemverTriple> {
        self.upper
    }

    /// When true, the upper bound is inclusive (used for exact pins).
    #[must_use]
    pub fn upper_inclusive(&self) -> bool {
        self.upper_inclusive
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn triple_ord_lexicographic() {
        assert!(SemverTriple::new(1, 0, 0) < SemverTriple::new(1, 0, 1));
        assert!(SemverTriple::new(1, 0, 1) < SemverTriple::new(1, 1, 0));
        assert!(SemverTriple::new(1, 1, 0) < SemverTriple::new(2, 0, 0));
    }

    #[test]
    fn triple_zero_constant() {
        assert_eq!(SemverTriple::ZERO, SemverTriple::new(0, 0, 0));
    }

    #[test]
    fn triple_display_canonical_dotted() {
        assert_eq!(SemverTriple::new(1, 4, 2).to_string(), "1.4.2");
        assert_eq!(SemverTriple::ZERO.to_string(), "0.0.0");
    }
}
