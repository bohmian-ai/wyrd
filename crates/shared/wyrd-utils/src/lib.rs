//! Small shared helpers with no server or Python dependencies.

#![deny(missing_docs)]

/// Generate a UUIDv7 string.
#[must_use]
pub fn uuid7() -> String {
    uuid::Uuid::new_v7(uuid::Timestamp::now(uuid::NoContext)).to_string()
}

/// Parse and normalize a semantic version.
///
/// # Errors
/// Returns an error when the input is not semver.
pub fn normalize_version(value: &str) -> Result<String, semver::Error> {
    semver::Version::parse(value).map(|version| version.to_string())
}

/// Return a score clamped into the `0.0..=1.0` range.
#[must_use]
pub fn clamp_score(value: f64) -> f64 {
    value.clamp(0.0, 1.0)
}

/// Convert a Python-style identifier into a simple kebab-case token.
#[must_use]
pub fn depythonize(value: &str) -> String {
    value.trim().replace('_', "-").to_ascii_lowercase()
}
