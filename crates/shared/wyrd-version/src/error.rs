use thiserror::Error;

#[derive(Debug, Error)]
pub enum WyrdVersionError {
    #[error("invalid semver string: {0}")]
    InvalidVersion(#[source] semver::Error),

    #[error("invalid prerelease identifier: {0}")]
    InvalidPrerelease(#[source] semver::Error),

    #[error("invalid build identifier: {0}")]
    InvalidBuild(#[source] semver::Error),

    #[error("version string is empty")]
    EmptyVersion,
}
