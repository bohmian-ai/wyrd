//! Storage-scoped identifier newtypes.

use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;
use ulid::Ulid;

/// Typed upload identifier displayed as `wyu_{ulid}`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(transparent)]
pub struct UploadId(String);

impl UploadId {
    /// Generate a new upload identifier.
    #[must_use]
    pub fn new() -> Self {
        Self::from_ulid(Ulid::new())
    }

    /// Build an upload identifier from an existing ULID.
    #[must_use]
    pub fn from_ulid(value: Ulid) -> Self {
        Self(format!("wyu_{value}"))
    }

    /// Borrow the upload identifier as a string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Parse and return the ULID body.
    ///
    /// # Errors
    /// Returns an error if the stored identifier has somehow lost its valid
    /// `wyu_` ULID shape.
    pub fn as_ulid(&self) -> Result<Ulid, UploadIdParseError> {
        parse_ulid_body(&self.0)
    }
}

impl Default for UploadId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for UploadId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl FromStr for UploadId {
    type Err = UploadIdParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        parse_ulid_body(value)?;
        Ok(Self(value.to_owned()))
    }
}

impl<'de> Deserialize<'de> for UploadId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::from_str(&value).map_err(serde::de::Error::custom)
    }
}

fn parse_ulid_body(value: &str) -> Result<Ulid, UploadIdParseError> {
    let Some(body) = value.strip_prefix("wyu_") else {
        return Err(UploadIdParseError::MissingPrefix);
    };
    Ulid::from_string(body).map_err(|_| UploadIdParseError::InvalidUlid)
}

/// Error returned when parsing an [`UploadId`] fails.
#[derive(Debug, thiserror::Error)]
pub enum UploadIdParseError {
    /// The identifier did not start with `wyu_`.
    #[error("upload id must start with wyu_")]
    MissingPrefix,
    /// The identifier body was not a valid ULID.
    #[error("upload id body must be a valid ULID")]
    InvalidUlid,
}
