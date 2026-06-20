//! Canonical Wyrd API-version value.
//!
//! Semver primitives (`VersionBlock`, `VersionRange`, `VersionBump`,
//! `VersionBounds`, `SemverTriple`, `VersionError`, `VersionSpec`) live in
//! the [`wyrd_semver`] crate. Import them from there directly — this module
//! intentionally does not re-export them.

use std::fmt;

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
