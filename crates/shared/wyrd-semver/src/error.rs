//! Errors produced by semver parsing, bumping, and bounds derivation.

/// Version parse, bump, and bounds-derivation failures.
#[derive(Debug, thiserror::Error)]
pub enum VersionError {
    /// Version was not valid semver syntax.
    #[error("invalid semantic version: {source}")]
    Invalid {
        /// Parser error.
        source: semver::Error,
    },
    /// Version range was not valid semver requirement syntax.
    #[error("invalid semantic version range: {source}")]
    InvalidRange {
        /// Parser error.
        source: semver::Error,
    },
    /// Version string was empty.
    #[error("empty version string")]
    EmptyVersion,
    /// Pre-release identifier was rejected by the semver crate.
    #[error("invalid pre-release identifier: {source}")]
    InvalidPrerelease {
        /// Parser error.
        source: semver::Error,
    },
    /// Build metadata was rejected by the semver crate.
    #[error("invalid build metadata: {source}")]
    InvalidBuild {
        /// Parser error.
        source: semver::Error,
    },
    /// Computing the upper bound of a version range would overflow a `u64`.
    #[error("version bounds overflow")]
    BoundsOverflow,
    /// Version range cannot be expressed as a single half-open SQL bounds interval.
    #[error("version range is not bound-representable: {reason}")]
    NotRepresentable {
        /// Human-readable reason.
        reason: &'static str,
    },
}
