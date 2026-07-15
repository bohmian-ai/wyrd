//! Card code-origin provenance — the commit a card was declared from.
//!
//! `Origin` is the platform's **code axis**: it ties a registered Card back to
//! the exact repository commit (and path) it was authored at. Runtime
//! observations that carry the same commit (see
//! [`crate::vala::correlation::CorrelationContext`]) can then be joined to the
//! card across the full develop → review → deploy → observe lifecycle, even
//! though identity (the service principal) is only minted at registration.
//!
//! `Origin` is distinct from a Source card, which is a read-side reference to an
//! external data system. Origin describes where a card's *own* code came from.

use serde::{Deserialize, Deserializer, Serialize};

use crate::error::WyrdError;

/// Git commit object name a card or observation was produced from.
///
/// Wire format is a 7- to 40-character lowercase hex string (an abbreviated or
/// full git SHA). Deserialization validates, so wire payloads cannot carry a
/// malformed commit.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "server", schema(value_type = String))]
#[serde(transparent)]
pub struct CommitSha(String);

impl CommitSha {
    /// Constructs a validated commit sha.
    ///
    /// # Errors
    /// Returns [`OriginValidationError::InvalidCommit`] when the value is not a
    /// 7- to 40-character lowercase hex string.
    pub fn new(value: impl Into<String>) -> Result<Self, OriginValidationError> {
        let value = value.into();
        if is_valid_commit_sha(&value) {
            Ok(Self(value))
        } else {
            Err(OriginValidationError::InvalidCommit { value })
        }
    }

    /// Borrows the validated commit sha as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for CommitSha {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::str::FromStr for CommitSha {
    type Err = OriginValidationError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::new(value)
    }
}

impl<'de> Deserialize<'de> for CommitSha {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        Self::new(value).map_err(serde::de::Error::custom)
    }
}

impl schemars::JsonSchema for CommitSha {
    fn schema_name() -> String {
        "CommitSha".to_string()
    }

    fn json_schema(_generator: &mut schemars::r#gen::SchemaGenerator) -> schemars::schema::Schema {
        use schemars::schema::{InstanceType, SchemaObject, SingleOrVec, StringValidation};

        SchemaObject {
            instance_type: Some(SingleOrVec::Single(Box::new(InstanceType::String))),
            string: Some(Box::new(StringValidation {
                max_length: Some(40),
                min_length: Some(7),
                pattern: Some(r"^[0-9a-f]{7,40}$".to_string()),
            })),
            ..Default::default()
        }
        .into()
    }
}

/// Code-origin provenance for a card: the repository commit it was declared
/// from, plus the in-repo path for monorepo disambiguation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct Origin {
    /// Normalized repository identifier, e.g. `github.com/org/repo`.
    pub repo: String,
    /// Commit the card was declared from. When `dirty`, this is the base commit
    /// of the uncommitted working tree.
    pub commit: CommitSha,
    /// Declaration path within the repository (monorepo disambiguation).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// Whether the card was declared from an uncommitted working tree.
    #[serde(default, skip_serializing_if = "is_false")]
    pub dirty: bool,
}

impl Origin {
    /// Validates non-empty coordinates. The commit is validated on
    /// construction/deserialization.
    ///
    /// # Errors
    /// Returns [`OriginValidationError`] when `repo` is empty or `path` is
    /// present but empty.
    pub fn validate(&self) -> Result<(), OriginValidationError> {
        if self.repo.trim().is_empty() {
            return Err(OriginValidationError::EmptyRepo);
        }
        if let Some(path) = &self.path
            && path.trim().is_empty()
        {
            return Err(OriginValidationError::EmptyPath);
        }
        Ok(())
    }
}

/// Origin provenance validation failures.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum OriginValidationError {
    /// The commit was not a 7- to 40-character lowercase hex string.
    #[error("origin commit `{value}` must be 7-40 lowercase hex characters")]
    InvalidCommit {
        /// The offending value.
        value: String,
    },
    /// The repository identifier was empty or whitespace-only.
    #[error("origin field `repo` must not be empty")]
    EmptyRepo,
    /// The path was present but empty or whitespace-only.
    #[error("origin field `path` must not be empty when present")]
    EmptyPath,
}

impl From<OriginValidationError> for WyrdError {
    fn from(e: OriginValidationError) -> Self {
        Self::OriginValidation {
            message: e.to_string(),
            details: serde_json::Value::Null,
        }
    }
}

fn is_valid_commit_sha(value: &str) -> bool {
    (7..=40).contains(&value.len())
        && value
            .bytes()
            .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

// justification: serde skip_serializing_if predicate signature requires fn(&T) -> bool
#[allow(clippy::trivially_copy_pass_by_ref)]
fn is_false(value: &bool) -> bool {
    !*value
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commit_sha_accepts_short_and_full_hex() {
        assert!(CommitSha::new("abc1234").is_ok());
        assert!(CommitSha::new("0123456789abcdef0123456789abcdef01234567").is_ok());
    }

    #[test]
    fn commit_sha_rejects_uppercase_nonhex_and_bad_length() {
        assert!(CommitSha::new("ABC1234").is_err());
        assert!(CommitSha::new("ghijklm").is_err());
        assert!(CommitSha::new("abc123").is_err()); // 6 chars
        assert!(CommitSha::new("0123456789abcdef0123456789abcdef012345678").is_err()); // 41 chars
    }

    #[test]
    fn origin_round_trips_and_omits_defaults() {
        let origin = Origin {
            repo: "github.com/org/repo".to_string(),
            commit: CommitSha::new("deadbeef").expect("valid commit"),
            path: None,
            dirty: false,
        };
        let json = serde_json::to_value(&origin).expect("serialize");
        assert_eq!(json.get("path"), None, "None path is omitted");
        assert_eq!(json.get("dirty"), None, "false dirty is omitted");
        let back: Origin = serde_json::from_value(json).expect("deserialize");
        assert_eq!(origin, back);
    }

    #[test]
    fn origin_deserialize_rejects_bad_commit() {
        let json = serde_json::json!({ "repo": "r", "commit": "NOTHEX!" });
        assert!(serde_json::from_value::<Origin>(json).is_err());
    }

    #[test]
    fn origin_validate_rejects_empty_repo_and_path() {
        let mut origin = Origin {
            repo: "  ".to_string(),
            commit: CommitSha::new("deadbeef").expect("valid commit"),
            path: None,
            dirty: false,
        };
        assert_eq!(origin.validate(), Err(OriginValidationError::EmptyRepo));
        origin.repo = "github.com/org/repo".to_string();
        origin.path = Some("  ".to_string());
        assert_eq!(origin.validate(), Err(OriginValidationError::EmptyPath));
    }

    #[test]
    fn origin_error_maps_to_wyrd_error() {
        let err: WyrdError = OriginValidationError::EmptyRepo.into();
        match err {
            WyrdError::OriginValidation { .. } => {}
            other => panic!("expected OriginValidation, got {other:?}"),
        }
    }
}
