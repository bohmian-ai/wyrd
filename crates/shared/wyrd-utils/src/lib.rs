//! Small shared helpers with no server dependencies.
//!
//! Python boundary helpers live behind the optional `python` feature.

#![deny(missing_docs)]

pub mod codec;
pub mod fs;
pub mod json;
#[cfg(feature = "python")]
pub mod py;

use std::path::PathBuf;

/// Shared utility result type.
pub type WyrdUtilsResult<T> = Result<T, WyrdUtilsError>;

/// Errors returned by shared Wyrd utility helpers.
#[derive(Debug, thiserror::Error)]
pub enum WyrdUtilsError {
    /// Filesystem IO failed.
    #[error(transparent)]
    Io(#[from] std::io::Error),
    /// JSON serialization failed.
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    /// Directory walking failed.
    #[error(transparent)]
    Walk(#[from] walkdir::Error),
    /// A path could not be made relative to its expected root.
    #[error(transparent)]
    StripPrefix(#[from] std::path::StripPrefixError),
    /// The path does not exist or is not a local filesystem path.
    #[error("local path does not exist: {0}")]
    LocalPathMissing(PathBuf),
    /// The path exists but is not a file.
    #[error("local path is not a file: {0}")]
    LocalFileMissing(PathBuf),
    /// The path exists but is neither a regular file nor a directory.
    #[error("local path is not a file or directory: {0}")]
    UnsupportedPath(PathBuf),
}

pub use wyrd_spec::uuid7;

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
