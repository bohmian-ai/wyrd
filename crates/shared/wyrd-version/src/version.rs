use std::fmt;
use std::str::FromStr;

use schemars::JsonSchema;
use semver::{BuildMetadata, Prerelease, Version};
use serde::{Deserialize, Serialize};

use crate::error::WyrdVersionError;

/// A validated semver version used across Wyrd crates and artifacts.
///
/// Always serializes as the canonical `MAJOR.MINOR.PATCH[-pre][+build]` string.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "String", into = "String")]
pub struct WyrdVersion(#[schemars(with = "String")] Version);

impl WyrdVersion {
    /// Parse a semver string.
    ///
    /// # Errors
    /// Returns [`WyrdVersionError::EmptyVersion`] for empty input and
    /// [`WyrdVersionError::InvalidVersion`] when `semver::Version::parse` rejects the input.
    pub fn parse(s: &str) -> Result<Self, WyrdVersionError> {
        if s.is_empty() {
            return Err(WyrdVersionError::EmptyVersion);
        }
        Version::parse(s)
            .map(Self)
            .map_err(WyrdVersionError::InvalidVersion)
    }

    /// Construct from major/minor/patch components.
    #[must_use]
    pub fn new(major: u64, minor: u64, patch: u64) -> Self {
        Self(Version::new(major, minor, patch))
    }

    /// The version of the crate calling this function at build time.
    /// Used to stamp fitted-baseline artifacts with the Wyrd version that
    /// produced them so downstream code can detect schema drift.
    #[must_use]
    pub fn current() -> Self {
        match Self::parse(env!("CARGO_PKG_VERSION")) {
            Ok(version) => version,
            Err(_) => Self(Version::new(0, 0, 0)),
        }
    }

    #[must_use]
    pub fn major(&self) -> u64 {
        self.0.major
    }

    #[must_use]
    pub fn minor(&self) -> u64 {
        self.0.minor
    }

    #[must_use]
    pub fn patch(&self) -> u64 {
        self.0.patch
    }

    #[must_use]
    pub fn pre(&self) -> &Prerelease {
        &self.0.pre
    }

    #[must_use]
    pub fn build(&self) -> &BuildMetadata {
        &self.0.build
    }

    /// Bump this version by the given strategy. Returns a new value.
    ///
    /// # Errors
    /// Returns [`WyrdVersionError::InvalidPrerelease`] or
    /// [`WyrdVersionError::InvalidBuild`] if the supplied identifiers fail
    /// semver validation.
    pub fn bump(&self, bump: VersionBump<'_>) -> Result<Self, WyrdVersionError> {
        let mut next = self.0.clone();
        match bump {
            VersionBump::Major => {
                next.major += 1;
                next.minor = 0;
                next.patch = 0;
                next.pre = Prerelease::EMPTY;
                next.build = BuildMetadata::EMPTY;
            }
            VersionBump::Minor => {
                next.minor += 1;
                next.patch = 0;
                next.pre = Prerelease::EMPTY;
                next.build = BuildMetadata::EMPTY;
            }
            VersionBump::Patch => {
                next.patch += 1;
                next.pre = Prerelease::EMPTY;
                next.build = BuildMetadata::EMPTY;
            }
            VersionBump::Pre(p) => {
                next.pre = Prerelease::new(p).map_err(WyrdVersionError::InvalidPrerelease)?;
            }
            VersionBump::Build(b) => {
                next.build = BuildMetadata::new(b).map_err(WyrdVersionError::InvalidBuild)?;
            }
        }
        Ok(Self(next))
    }
}

impl fmt::Display for WyrdVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)
    }
}

impl FromStr for WyrdVersion {
    type Err = WyrdVersionError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

impl TryFrom<String> for WyrdVersion {
    type Error = WyrdVersionError;

    fn try_from(s: String) -> Result<Self, Self::Error> {
        Self::parse(&s)
    }
}

impl From<WyrdVersion> for String {
    fn from(v: WyrdVersion) -> Self {
        v.to_string()
    }
}

/// Bump strategy passed to [`WyrdVersion::bump`].
#[derive(Debug, Clone, Copy)]
pub enum VersionBump<'a> {
    Major,
    Minor,
    Patch,
    /// Set the prerelease identifier. Empty string clears it.
    Pre(&'a str),
    /// Set the build identifier. Empty string clears it.
    Build(&'a str),
}

#[cfg(test)]
mod tests {
    use super::{VersionBump, WyrdVersion};
    use crate::error::WyrdVersionError;

    #[test]
    fn parses_basic_semver() {
        let v = WyrdVersion::parse("1.2.3").expect("parse");
        assert_eq!(v.major(), 1);
        assert_eq!(v.minor(), 2);
        assert_eq!(v.patch(), 3);
        assert_eq!(v.to_string(), "1.2.3");
    }

    #[test]
    fn rejects_empty_string() {
        assert!(matches!(
            WyrdVersion::parse(""),
            Err(WyrdVersionError::EmptyVersion)
        ));
    }

    #[test]
    fn rejects_nonsense() {
        assert!(matches!(
            WyrdVersion::parse("not-a-version"),
            Err(WyrdVersionError::InvalidVersion(_))
        ));
    }

    #[test]
    fn bump_major() {
        let v = WyrdVersion::parse("1.2.3").unwrap();
        let n = v.bump(VersionBump::Major).unwrap();
        assert_eq!(n.to_string(), "2.0.0");
    }

    #[test]
    fn bump_minor_clears_patch_and_pre() {
        let v = WyrdVersion::parse("1.2.3-alpha.1").unwrap();
        let n = v.bump(VersionBump::Minor).unwrap();
        assert_eq!(n.to_string(), "1.3.0");
    }

    #[test]
    fn bump_patch() {
        let v = WyrdVersion::parse("1.2.3").unwrap();
        let n = v.bump(VersionBump::Patch).unwrap();
        assert_eq!(n.to_string(), "1.2.4");
    }

    #[test]
    fn set_prerelease() {
        let v = WyrdVersion::parse("1.2.3").unwrap();
        let n = v.bump(VersionBump::Pre("rc.1")).unwrap();
        assert_eq!(n.to_string(), "1.2.3-rc.1");
    }

    #[test]
    fn set_build() {
        let v = WyrdVersion::parse("1.2.3").unwrap();
        let n = v.bump(VersionBump::Build("abc123")).unwrap();
        assert_eq!(n.to_string(), "1.2.3+abc123");
    }

    #[test]
    fn invalid_prerelease() {
        let v = WyrdVersion::parse("1.2.3").unwrap();
        let err = v.bump(VersionBump::Pre("not valid")).unwrap_err();
        assert!(matches!(err, WyrdVersionError::InvalidPrerelease(_)));
    }

    #[test]
    fn round_trip_serde() {
        let v = WyrdVersion::parse("1.2.3-alpha.1+build.42").unwrap();
        let s = serde_json::to_string(&v).unwrap();
        assert_eq!(s, "\"1.2.3-alpha.1+build.42\"");
        let back: WyrdVersion = serde_json::from_str(&s).unwrap();
        assert_eq!(v, back);
    }

    #[test]
    fn current_returns_valid_version() {
        let v = WyrdVersion::current();
        assert!(v.major() > 0 || v.minor() > 0 || v.patch() > 0);
    }
}
