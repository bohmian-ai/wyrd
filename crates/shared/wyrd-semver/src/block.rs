//! Concrete semver version values.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::bounds::SemverTriple;
use crate::bump::VersionBump;
use crate::error::VersionError;

/// Semver-compatible version string.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct VersionBlock(String);

impl VersionBlock {
    /// Construct from a `(major, minor, patch)` triple.
    ///
    /// Always produces a valid `VersionBlock` because the components are typed
    /// `u64`s that format to a valid semver triple. Used to seed a
    /// `VersionBlock` from a `VersionBounds::lower()` value on the register
    /// path.
    #[must_use]
    pub fn from_triple(triple: SemverTriple) -> Self {
        Self::parse(format!(
            "{}.{}.{}",
            triple.major, triple.minor, triple.patch
        ))
        .expect("SemverTriple components always form a valid semver triple")
    }

    /// Parse a semver version.
    ///
    /// # Errors
    /// Returns an error when `value` is not valid semantic version syntax.
    pub fn parse(value: impl Into<String>) -> Result<Self, VersionError> {
        let value = value.into();
        if value.is_empty() {
            return Err(VersionError::EmptyVersion);
        }
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

    /// Returns true when this value is an exact semver triple (with optional
    /// pre-release and build metadata), not a range expression.
    ///
    /// Always true for any successfully-parsed `VersionBlock`. Provided as an
    /// explicit method so guards that require a pinned version (e.g. Service
    /// or Agent card registration) can read at the call site, and so the
    /// definition of "pin" has a single source of truth if the type ever
    /// loosens.
    #[must_use]
    pub fn is_pin(&self) -> bool {
        true
    }

    /// Return a bumped semantic version.
    ///
    /// `Major`/`Minor`/`Patch` reset pre-release and build metadata to empty,
    /// matching the conventional semver bump. `Pre`/`Build`/`PreBuild` are
    /// additive: they set the requested fields without resetting the triple
    /// or the unaffected metadata.
    ///
    /// # Errors
    /// Returns an error when this stored version is not valid semantic version
    /// syntax, or when a supplied pre-release/build identifier is rejected by
    /// the semver crate.
    pub fn bump(&self, bump: VersionBump) -> Result<Self, VersionError> {
        let mut version = self.semver()?;
        match bump {
            VersionBump::Patch => {
                version.patch += 1;
                version.pre = semver::Prerelease::EMPTY;
                version.build = semver::BuildMetadata::EMPTY;
            }
            VersionBump::Minor => {
                version.minor += 1;
                version.patch = 0;
                version.pre = semver::Prerelease::EMPTY;
                version.build = semver::BuildMetadata::EMPTY;
            }
            VersionBump::Major => {
                version.major += 1;
                version.minor = 0;
                version.patch = 0;
                version.pre = semver::Prerelease::EMPTY;
                version.build = semver::BuildMetadata::EMPTY;
            }
            VersionBump::Pre { identifier } => {
                version.pre = semver::Prerelease::new(&identifier)
                    .map_err(|source| VersionError::InvalidPrerelease { source })?;
            }
            VersionBump::Build { metadata } => {
                version.build = semver::BuildMetadata::new(&metadata)
                    .map_err(|source| VersionError::InvalidBuild { source })?;
            }
            VersionBump::PreBuild { pre, build } => {
                version.pre = semver::Prerelease::new(&pre)
                    .map_err(|source| VersionError::InvalidPrerelease { source })?;
                version.build = semver::BuildMetadata::new(&build)
                    .map_err(|source| VersionError::InvalidBuild { source })?;
            }
        }
        Ok(Self(version.to_string()))
    }

    /// Sort versions by semantic version precedence.
    pub fn sort_versions(versions: &mut [Self]) {
        versions.sort_by(|left, right| {
            left.semver()
                .expect("VersionBlock invariant")
                .cmp(&right.semver().expect("VersionBlock invariant"))
        });
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_accepts_full_triple() {
        let block = VersionBlock::parse("1.2.3").unwrap();
        assert_eq!(block.as_str(), "1.2.3");
    }

    #[test]
    fn parse_rejects_partial() {
        let err = VersionBlock::parse("1.2").unwrap_err();
        assert!(matches!(err, VersionError::Invalid { .. }));
    }

    #[test]
    fn parse_rejects_empty() {
        let err = VersionBlock::parse("").unwrap_err();
        assert!(matches!(err, VersionError::EmptyVersion));
    }

    #[test]
    fn from_str_parses() {
        let block: VersionBlock = "1.2.3".parse().unwrap();
        assert_eq!(block.as_str(), "1.2.3");
    }

    #[test]
    fn display_round_trip() {
        let block = VersionBlock::parse("1.2.3-rc.1+meta").unwrap();
        assert_eq!(block.to_string(), "1.2.3-rc.1+meta");
    }

    #[test]
    fn default_is_zero_zero_one() {
        assert_eq!(VersionBlock::default().as_str(), "0.0.1");
    }

    #[test]
    fn bump_patch() {
        let next = VersionBlock::parse("1.2.3")
            .unwrap()
            .bump(VersionBump::Patch)
            .unwrap();
        assert_eq!(next.as_str(), "1.2.4");
    }

    #[test]
    fn bump_minor_resets_patch() {
        let next = VersionBlock::parse("1.2.3")
            .unwrap()
            .bump(VersionBump::Minor)
            .unwrap();
        assert_eq!(next.as_str(), "1.3.0");
    }

    #[test]
    fn bump_major_resets_minor_patch() {
        let next = VersionBlock::parse("1.2.3")
            .unwrap()
            .bump(VersionBump::Major)
            .unwrap();
        assert_eq!(next.as_str(), "2.0.0");
    }

    #[test]
    fn bump_pre_adds_prerelease() {
        let next = VersionBlock::parse("1.2.3")
            .unwrap()
            .bump(VersionBump::Pre {
                identifier: "rc.1".to_string(),
            })
            .unwrap();
        assert_eq!(next.as_str(), "1.2.3-rc.1");
    }

    #[test]
    fn bump_build_adds_metadata() {
        let next = VersionBlock::parse("1.2.3")
            .unwrap()
            .bump(VersionBump::Build {
                metadata: "001".to_string(),
            })
            .unwrap();
        assert_eq!(next.as_str(), "1.2.3+001");
    }

    #[test]
    fn bump_pre_build_sets_both() {
        let next = VersionBlock::parse("1.2.3")
            .unwrap()
            .bump(VersionBump::PreBuild {
                pre: "rc.1".to_string(),
                build: "sha-abc".to_string(),
            })
            .unwrap();
        assert_eq!(next.as_str(), "1.2.3-rc.1+sha-abc");
    }

    #[test]
    fn bump_pre_does_not_reset_build() {
        let next = VersionBlock::parse("1.2.3+meta")
            .unwrap()
            .bump(VersionBump::Pre {
                identifier: "rc.1".to_string(),
            })
            .unwrap();
        assert_eq!(next.as_str(), "1.2.3-rc.1+meta");
    }

    #[test]
    fn bump_major_clears_pre_and_build() {
        let next = VersionBlock::parse("1.2.3-rc.1+meta")
            .unwrap()
            .bump(VersionBump::Major)
            .unwrap();
        assert_eq!(next.as_str(), "2.0.0");
    }

    #[test]
    fn bump_pre_rejects_invalid_identifier() {
        let err = VersionBlock::parse("1.2.3")
            .unwrap()
            .bump(VersionBump::Pre {
                identifier: "not valid!".to_string(),
            })
            .unwrap_err();
        assert!(matches!(err, VersionError::InvalidPrerelease { .. }));
    }

    #[test]
    fn bump_build_rejects_invalid_metadata() {
        let err = VersionBlock::parse("1.2.3")
            .unwrap()
            .bump(VersionBump::Build {
                metadata: "not valid!".to_string(),
            })
            .unwrap_err();
        assert!(matches!(err, VersionError::InvalidBuild { .. }));
    }

    #[test]
    fn is_pin_is_true_for_parsed_block() {
        assert!(VersionBlock::parse("1.2.3").unwrap().is_pin());
        assert!(VersionBlock::parse("1.2.3-rc.1+meta").unwrap().is_pin());
    }

    #[test]
    fn sort_versions_orders_lexicographically() {
        let mut versions = vec![
            VersionBlock::parse("1.2.10").unwrap(),
            VersionBlock::parse("1.2.2").unwrap(),
            VersionBlock::parse("0.9.0").unwrap(),
        ];
        VersionBlock::sort_versions(&mut versions);
        let strs: Vec<_> = versions.iter().map(VersionBlock::as_str).collect();
        assert_eq!(strs, vec!["0.9.0", "1.2.2", "1.2.10"]);
    }

    #[test]
    fn sort_versions_respects_prerelease_precedence() {
        let mut versions = vec![
            VersionBlock::parse("1.2.3").unwrap(),
            VersionBlock::parse("1.2.3-rc.1").unwrap(),
        ];
        VersionBlock::sort_versions(&mut versions);
        let strs: Vec<_> = versions.iter().map(VersionBlock::as_str).collect();
        assert_eq!(strs, vec!["1.2.3-rc.1", "1.2.3"]);
    }

    #[test]
    fn bump_minor_clears_pre_and_build() {
        let next = VersionBlock::parse("1.2.3-rc.1+meta")
            .unwrap()
            .bump(VersionBump::Minor)
            .unwrap();
        assert_eq!(next.as_str(), "1.3.0");
    }

    #[test]
    fn bump_patch_clears_pre_and_build() {
        let next = VersionBlock::parse("1.2.3-rc.1+meta")
            .unwrap()
            .bump(VersionBump::Patch)
            .unwrap();
        assert_eq!(next.as_str(), "1.2.4");
    }

    #[test]
    fn from_triple_round_trips_to_block() {
        let block = VersionBlock::from_triple(SemverTriple::new(1, 4, 2));
        assert_eq!(block.as_str(), "1.4.2");
    }

    #[test]
    fn from_triple_zero() {
        let block = VersionBlock::from_triple(SemverTriple::ZERO);
        assert_eq!(block.as_str(), "0.0.0");
    }
}
