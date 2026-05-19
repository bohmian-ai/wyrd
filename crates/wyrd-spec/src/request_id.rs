//! Request correlation identifiers.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

/// Request correlation ID accepted as a ULID or UUID.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct RequestId(String);

impl RequestId {
    /// Parse a `RequestId` from a ULID or UUID string.
    ///
    /// # Errors
    /// Returns an error when the input is neither a valid ULID nor UUID.
    pub fn parse(value: &str) -> Result<Self, RequestIdError> {
        if is_ulid(value) || is_uuid7(value) {
            return Ok(Self(value.to_string()));
        }
        Err(RequestIdError::Invalid)
    }

    /// Borrow the inner identifier string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

fn is_ulid(value: &str) -> bool {
    value.len() == 26
        && value
            .chars()
            .all(|ch| matches!(ch, '0'..='9' | 'A'..='H' | 'J'..='K' | 'M'..='N' | 'P'..='T' | 'V'..='Z'))
}

fn is_uuid7(value: &str) -> bool {
    uuid::Uuid::parse_str(value)
        .map(|uuid| uuid.get_version_num() == 7)
        .unwrap_or(false)
}

impl fmt::Display for RequestId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl FromStr for RequestId {
    type Err = RequestIdError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::parse(value)
    }
}

/// Request ID validation errors.
#[derive(Debug, thiserror::Error)]
pub enum RequestIdError {
    /// Input did not parse as a ULID or UUID.
    #[error("invalid request id")]
    Invalid,
}
