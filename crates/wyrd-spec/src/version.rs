//! API and artifact version values.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

/// Canonical Wyrd API version.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct ApiVersion(String);

impl ApiVersion {
    /// The v1 API version string.
    pub const V1: &'static str = "wyrd/v1";

    /// Construct the canonical v1 API version.
    #[must_use]
    pub fn v1() -> Self {
        Self(Self::V1.to_string())
    }

    /// Borrow as a string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for ApiVersion {
    fn default() -> Self {
        Self::v1()
    }
}

impl fmt::Display for ApiVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Semver-compatible version string.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct VersionBlock(String);

impl VersionBlock {
    /// Parse a semver version.
    ///
    /// # Errors
    /// Returns an error when `value` is not valid semantic version syntax.
    pub fn parse(value: impl Into<String>) -> Result<Self, VersionError> {
        let value = value.into();
        semver::Version::parse(&value).map_err(|source| VersionError::Invalid { source })?;
        Ok(Self(value))
    }

    /// Parse into a semantic version value.
    ///
    /// # Errors
    /// Returns an error when this stored version is not valid semantic version syntax.
    pub fn semver(&self) -> Result<semver::Version, VersionError> {
        semver::Version::parse(&self.0).map_err(|source| VersionError::Invalid { source })
    }

    /// Borrow as a string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Return a bumped semantic version.
    ///
    /// # Errors
    /// Returns an error when this stored version is not valid semantic version syntax.
    pub fn bump(&self, bump: VersionBump) -> Result<Self, VersionError> {
        let mut version = self.semver()?;
        match bump {
            VersionBump::Patch => {
                version.patch += 1;
            }
            VersionBump::Minor => {
                version.minor += 1;
                version.patch = 0;
            }
            VersionBump::Major => {
                version.major += 1;
                version.minor = 0;
                version.patch = 0;
            }
        }
        version.pre = semver::Prerelease::EMPTY;
        version.build = semver::BuildMetadata::EMPTY;
        Ok(Self(version.to_string()))
    }

    /// Sort versions by semantic version precedence.
    ///
    /// # Errors
    /// Returns an error when any version is not valid semantic version syntax.
    pub fn sort_versions(versions: &mut [Self]) -> Result<(), VersionError> {
        versions.sort_by(|left, right| {
            let left = semver::Version::parse(left.as_str());
            let right = semver::Version::parse(right.as_str());
            match (left, right) {
                (Ok(left), Ok(right)) => left.cmp(&right),
                _ => std::cmp::Ordering::Equal,
            }
        });
        for version in versions {
            version.semver()?;
        }
        Ok(())
    }
}

impl Default for VersionBlock {
    fn default() -> Self {
        Self("0.0.1".to_string())
    }
}

impl fmt::Display for VersionBlock {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl FromStr for VersionBlock {
    type Err = VersionError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::parse(value)
    }
}

/// Version range expression.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct VersionRange(String);

impl VersionRange {
    /// Parse a semantic version requirement.
    ///
    /// # Errors
    /// Returns an error when `value` is not a valid semantic version requirement.
    pub fn parse(value: impl Into<String>) -> Result<Self, VersionError> {
        let value = value.into();
        semver::VersionReq::parse(&value)
            .map_err(|source| VersionError::InvalidRange { source })?;
        Ok(Self(value))
    }

    /// Borrow as a string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Check whether a concrete version matches this range.
    ///
    /// # Errors
    /// Returns an error when the stored range or version is invalid semver syntax.
    pub fn matches(&self, version: &VersionBlock) -> Result<bool, VersionError> {
        let range = semver::VersionReq::parse(&self.0)
            .map_err(|source| VersionError::InvalidRange { source })?;
        Ok(range.matches(&version.semver()?))
    }
}

impl Default for VersionRange {
    fn default() -> Self {
        Self("*".to_string())
    }
}

impl fmt::Display for VersionRange {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl FromStr for VersionRange {
    type Err = VersionError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::parse(value)
    }
}

/// Semantic version bump level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum VersionBump {
    /// Increment major and reset lower components.
    Major,
    /// Increment minor and reset patch.
    Minor,
    /// Increment patch.
    Patch,
}

/// Version parse failures.
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
}
