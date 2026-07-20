//! Registration identity and path types.

use std::fmt;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A validated relative path inside a card's artifact manifest.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(transparent)]
pub struct RelativeArtifactPath(String);

impl<'de> Deserialize<'de> for RelativeArtifactPath {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::new(&value).map_err(serde::de::Error::custom)
    }
}

impl RelativeArtifactPath {
    /// Validate and construct a relative artifact path.
    ///
    /// The grammar mirrors the server-side tenant-path validator
    /// (`crates/wyrd/wyrd-storage/src/tenant_path.rs`): each segment must
    /// match `[A-Za-z0-9._-]+`, and the segments `""`, `.`, and `..` are
    /// forbidden. Client and server therefore reject exactly the same set of
    /// paths, so a locally validated path either round-trips or the caller
    /// gets an immediate, offline `PathValidationError` instead of a network
    /// round-trip that fails at the server.
    pub fn new(value: &str) -> Result<Self, PathValidationError> {
        if value.is_empty() {
            return Err(PathValidationError::Empty);
        }
        if value.starts_with('/') {
            return Err(PathValidationError::Absolute);
        }
        if value.contains('\\') {
            return Err(PathValidationError::Backslash);
        }
        for segment in value.split('/') {
            if segment.is_empty() {
                return Err(PathValidationError::EmptySegment);
            }
            if segment == ".." {
                return Err(PathValidationError::ParentTraversal);
            }
            if segment == "." {
                return Err(PathValidationError::CurrentDir);
            }
            if let Some(bad) = segment
                .chars()
                .find(|c| !(c.is_ascii_alphanumeric() || *c == '.' || *c == '_' || *c == '-'))
            {
                return Err(PathValidationError::InvalidCharacter(bad));
            }
        }
        Ok(Self(value.to_owned()))
    }

    /// Borrow the validated path.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for RelativeArtifactPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Failure while validating a relative artifact path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum PathValidationError {
    /// The path was empty.
    #[error("artifact path must not be empty")]
    Empty,
    /// The path began at a filesystem root.
    #[error("artifact path must be relative")]
    Absolute,
    /// Backslashes are not valid wire separators.
    #[error("artifact path must use forward slashes")]
    Backslash,
    /// The path contained an empty segment.
    #[error("artifact path must not contain empty segments")]
    EmptySegment,
    /// The path attempted parent traversal.
    #[error("artifact path must not contain parent traversal")]
    ParentTraversal,
    /// A segment was `.` (current directory).
    #[error("artifact path must not contain current-dir segments")]
    CurrentDir,
    /// A segment contained a character outside `[A-Za-z0-9._-]`.
    #[error("artifact path segment contains invalid character: {0:?}")]
    InvalidCharacter(char),
}

impl From<PathValidationError> for crate::error::WyrdError {
    fn from(error: PathValidationError) -> Self {
        Self::RegistryUnresolvedPathRef {
            message: error.to_string(),
            details: serde_json::json!({}),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{PathValidationError, RelativeArtifactPath};
    use crate::error::WyrdError;

    #[test]
    fn relative_path_rejects_traversal_and_platform_separators() {
        assert_eq!(
            RelativeArtifactPath::new("a\\b"),
            Err(PathValidationError::Backslash)
        );
        assert_eq!(
            RelativeArtifactPath::new("a/../b"),
            Err(PathValidationError::ParentTraversal)
        );
        assert_eq!(
            RelativeArtifactPath::new("/a"),
            Err(PathValidationError::Absolute)
        );
        assert_eq!(
            RelativeArtifactPath::new("a//b"),
            Err(PathValidationError::EmptySegment)
        );
    }

    #[test]
    fn relative_path_matches_server_tenant_grammar() {
        // Mirror `crates/wyrd/wyrd-storage/src/tenant_path.rs` so the client's
        // typed newtype rejects exactly what the server rejects — no
        // successful-locally-then-rejected-remotely surprises. Any locally
        // valid path must round-trip through the server.
        assert_eq!(
            RelativeArtifactPath::new("a/./b"),
            Err(PathValidationError::CurrentDir),
            "current-dir segments are forbidden by the server; reject them here too"
        );
        assert_eq!(
            RelativeArtifactPath::new("weights/model file.bin"),
            Err(PathValidationError::InvalidCharacter(' ')),
            "the server grammar excludes spaces; reject them here too"
        );
        assert_eq!(
            RelativeArtifactPath::new("weights/mödel.bin"),
            Err(PathValidationError::InvalidCharacter('ö')),
            "the server grammar is ASCII-only; reject non-ASCII here too"
        );
        assert_eq!(
            RelativeArtifactPath::new("weights/model+v2.bin"),
            Err(PathValidationError::InvalidCharacter('+'))
        );

        // These shapes are accepted by both validators.
        for accepted in [
            "model.bin",
            "weights/model.bin",
            "layer_0/attn.qkv-proj.bin",
            "v1.2.3/file",
        ] {
            RelativeArtifactPath::new(accepted)
                .unwrap_or_else(|err| panic!("{accepted:?} must be accepted, got {err:?}"));
        }
    }

    #[test]
    fn path_error_maps_to_registry_error_catalog() {
        let error: WyrdError = PathValidationError::Absolute.into();
        assert_eq!(error.code(), "WYRD_REGISTRY_400_UNRESOLVED_PATH_REF");
        assert_eq!(error.status(), 400);
    }
}

/// Opaque server handle for an in-flight registration operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(transparent)]
pub struct RegistrationOperationId(Uuid);

impl RegistrationOperationId {
    /// Construct an operation id from a UUID.
    #[must_use]
    pub const fn new(value: Uuid) -> Self {
        Self(value)
    }

    /// Borrow the underlying UUID.
    #[must_use]
    pub const fn as_uuid(self) -> Uuid {
        self.0
    }
}
