//! Newtype identifiers used across Wyrd specs.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

macro_rules! id_type {
    ($name:ident, $doc:literal, $validator:ident) => {
        #[doc = $doc]
        #[derive(
            Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, schemars::JsonSchema,
        )]
        #[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
        pub struct $name(String);

        impl $name {
            /// Build a validated identifier.
            ///
            /// # Errors
            /// Returns an error when the identifier is not canonical for this type.
            pub fn new(value: impl Into<String>) -> Result<Self, IdError> {
                let value = value.into();
                $validator(&value)?;
                Ok(Self(value))
            }

            /// Borrow the identifier as a string.
            #[must_use]
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }

        impl FromStr for $name {
            type Err = IdError;

            fn from_str(value: &str) -> Result<Self, Self::Err> {
                Self::new(value)
            }
        }

        impl From<$name> for String {
            fn from(value: $name) -> Self {
                value.0
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                let value = String::deserialize(deserializer)?;
                Self::new(value).map_err(serde::de::Error::custom)
            }
        }
    };
}

id_type!(
    SpaceName,
    "Human-visible namespace for Cards and Runs.",
    validate_token
);
id_type!(CardName, "Human-visible Card name.", validate_token);
id_type!(CardUid, "Resolved immutable Card UID.", validate_uuid7);
id_type!(
    ProfileName,
    "Named execution or configuration profile.",
    validate_token
);
id_type!(
    ExperimentUid,
    "Resolved immutable Experiment UID.",
    validate_uuid7
);
id_type!(ApiToken, "Opaque API token identifier.", validate_opaque);
id_type!(
    IdempotencyKey,
    "Idempotency key for write operations.",
    validate_opaque
);
id_type!(ArtifactKey, "Artifact storage key.", validate_token);
id_type!(RoleName, "RBAC role identifier.", validate_token);
id_type!(ColumnName, "DataCard column name.", validate_card_token);
id_type!(
    FeatureName,
    "Feature column name referenced by a DriftCard signal.",
    validate_card_token
);
id_type!(SplitName, "DataCard split label.", validate_card_token);
id_type!(QueryName, "DataCard SQL query key.", validate_card_token);

fn validate_token(value: &str) -> Result<(), IdError> {
    if value.len() < 3 || value.len() > 64 {
        return Err(IdError::InvalidToken);
    }
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return Err(IdError::InvalidToken);
    };
    if !first.is_ascii_lowercase() {
        return Err(IdError::InvalidToken);
    }
    if chars.all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '_' || ch == '-') {
        Ok(())
    } else {
        Err(IdError::InvalidToken)
    }
}

fn validate_card_token(value: &str) -> Result<(), IdError> {
    if value.is_empty() || value.len() > 64 {
        return Err(IdError::InvalidToken);
    }
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return Err(IdError::InvalidToken);
    };
    if !first.is_ascii_lowercase() {
        return Err(IdError::InvalidToken);
    }
    if chars.all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '_' || ch == '-') {
        Ok(())
    } else {
        Err(IdError::InvalidToken)
    }
}

fn validate_uuid7(value: &str) -> Result<(), IdError> {
    let uuid = uuid::Uuid::parse_str(value).map_err(|_| IdError::InvalidUuid7)?;
    if uuid.get_version_num() == 7 {
        Ok(())
    } else {
        Err(IdError::InvalidUuid7)
    }
}

fn validate_opaque(value: &str) -> Result<(), IdError> {
    if value.len() < 8 || value.len() > 256 || value.chars().any(char::is_whitespace) {
        return Err(IdError::InvalidOpaque);
    }
    Ok(())
}

/// Identifier validation failures.
#[derive(Debug, thiserror::Error)]
pub enum IdError {
    /// Human-readable token did not match `[a-z][a-z0-9_-]{2,63}`.
    #[error("identifier must match [a-z][a-z0-9_-]{{2,63}}")]
    InvalidToken,
    /// UID was not a UUIDv7.
    #[error("identifier must be a UUIDv7")]
    InvalidUuid7,
    /// Opaque identifier was empty, too long, too short, or contained whitespace.
    #[error("opaque identifier is invalid")]
    InvalidOpaque,
}
