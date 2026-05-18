//! Newtype identifiers used across Wyrd specs.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

macro_rules! string_id {
    ($name:ident, $doc:literal) => {
        #[doc = $doc]
        #[derive(
            Debug,
            Clone,
            PartialEq,
            Eq,
            PartialOrd,
            Ord,
            Hash,
            Serialize,
            Deserialize,
            schemars::JsonSchema,
        )]
        #[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
        pub struct $name(String);

        impl $name {
            /// Build a validated identifier.
            ///
            /// # Errors
            /// Returns an error when the identifier is empty or contains whitespace.
            pub fn new(value: impl Into<String>) -> Result<Self, IdError> {
                let value = value.into();
                validate(&value)?;
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
    };
}

string_id!(SpaceName, "Human-visible namespace for Cards and Runs.");
string_id!(CardName, "Human-visible Card name.");
string_id!(CardUid, "Resolved immutable Card UID.");
string_id!(ProfileName, "Named execution or configuration profile.");
string_id!(ExperimentUid, "Resolved immutable Experiment UID.");
string_id!(ApiToken, "Opaque API token identifier.");
string_id!(IdempotencyKey, "Idempotency key for write operations.");
string_id!(ArtifactKey, "Artifact storage key.");

fn validate(value: &str) -> Result<(), IdError> {
    if value.trim().is_empty() {
        return Err(IdError::Empty);
    }
    if value.chars().any(char::is_whitespace) {
        return Err(IdError::Whitespace(value.to_string()));
    }
    Ok(())
}

/// Identifier validation failures.
#[derive(Debug, thiserror::Error)]
pub enum IdError {
    /// Identifier was empty.
    #[error("identifier must not be empty")]
    Empty,
    /// Identifier contained whitespace.
    #[error("identifier must not contain whitespace: {0}")]
    Whitespace(String),
}
