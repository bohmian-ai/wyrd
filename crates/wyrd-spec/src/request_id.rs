//! Request correlation identifiers.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

/// Request correlation ID; required to be a UUID v7.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct RequestId(String);

impl RequestId {
    /// Generate a fresh `RequestId` from a newly minted UUID v7.
    ///
    /// Infallible: the value is generated as a valid v7 UUID, so callers that
    /// need a correlator for a server-internal operation avoid the parse dance.
    #[must_use]
    pub fn now_v7() -> Self {
        Self(uuid::Uuid::now_v7().to_string())
    }

    /// Parse a `RequestId` from a UUID v7 string.
    ///
    /// # Errors
    /// Returns an error when the input is not a valid UUID v7.
    pub fn parse(value: &str) -> Result<Self, RequestIdError> {
        if is_uuid7(value) {
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
    /// Input did not parse as a UUID v7.
    #[error("invalid request id")]
    Invalid,
}
