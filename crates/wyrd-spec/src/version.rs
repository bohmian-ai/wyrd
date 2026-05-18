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
        semver::Version::parse(&value).map_err(|source| VersionError::Invalid {
            value: value.clone(),
            source,
        })?;
        Ok(Self(value))
    }

    /// Borrow as a string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
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

/// Version parse failures.
#[derive(Debug, thiserror::Error)]
pub enum VersionError {
    /// Version was not valid semver syntax.
    #[error("invalid semantic version {value}: {source}")]
    Invalid {
        /// Original string.
        value: String,
        /// Parser error.
        source: semver::Error,
    },
}
