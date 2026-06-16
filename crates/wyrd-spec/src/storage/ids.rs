//! Storage-scoped identifier newtypes.

use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;
use uuid::Uuid;

/// Typed upload identifier displayed as `wyu_{uuid_v7}`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(transparent)]
pub struct UploadId(String);

impl UploadId {
    /// Generate a new upload identifier.
    #[must_use]
    pub fn new() -> Self {
        Self::from_uuid(Uuid::now_v7())
    }

    /// Build an upload identifier from an existing UUID.
    #[must_use]
    pub fn from_uuid(value: Uuid) -> Self {
        Self(format!("wyu_{value}"))
    }

    /// Borrow the upload identifier as a string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Parse and return the UUID body.
    ///
    /// # Errors
    /// Returns an error if the stored identifier has somehow lost its valid
    /// `wyu_` UUID shape.
    pub fn as_uuid(&self) -> Result<Uuid, UploadIdParseError> {
        parse_uuid_body(&self.0)
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
        parse_uuid_body(value)?;
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

fn parse_uuid_body(value: &str) -> Result<Uuid, UploadIdParseError> {
    let Some(body) = value.strip_prefix("wyu_") else {
        return Err(UploadIdParseError::MissingPrefix);
    };
    let uuid = Uuid::parse_str(body).map_err(|_| UploadIdParseError::InvalidUuid)?;
    if uuid.get_version_num() != 7 {
        return Err(UploadIdParseError::InvalidUuid);
    }
    Ok(uuid)
}

/// Error returned when parsing an [`UploadId`] fails.
#[derive(Debug, thiserror::Error)]
pub enum UploadIdParseError {
    /// The identifier did not start with `wyu_`.
    #[error("upload id must start with wyu_")]
    MissingPrefix,
    /// The identifier body was not a valid UUID v7.
    #[error("upload id body must be a valid UUIDv7")]
    InvalidUuid,
}
