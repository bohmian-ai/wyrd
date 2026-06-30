//! Semver bump levels.

use serde::{Deserialize, Serialize};

/// Semantic version bump level.
///
/// `Major`, `Minor`, and `Patch` perform a conventional semver bump and clear
/// any pre-release / build metadata. `Pre`, `Build`, and `PreBuild` are
/// additive: they apply the requested pre-release or build metadata to the
/// existing triple without resetting it.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum VersionBump {
    /// Increment major and reset minor, patch, pre-release, and build metadata.
    Major,
    /// Increment minor and reset patch, pre-release, and build metadata.
    Minor,
    /// Increment patch and reset pre-release and build metadata.
    Patch,
    /// Set the pre-release identifier on the current triple (e.g. "rc.1").
    Pre {
        /// Pre-release identifier as defined by SemVer 2.0 §9.
        identifier: String,
    },
    /// Set the build metadata on the current triple (e.g. "001", "sha-abc123").
    Build {
        /// Build metadata as defined by SemVer 2.0 §10.
        metadata: String,
    },
    /// Set both pre-release and build metadata on the current triple.
    PreBuild {
        /// Pre-release identifier.
        pre: String,
        /// Build metadata.
        build: String,
    },
}
