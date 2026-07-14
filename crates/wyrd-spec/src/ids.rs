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

impl CardUid {
    /// Build a [`CardUid`] from a UUIDv7 value.
    ///
    /// The UUID must be version 7; any other version returns [`IdError::InvalidUuid7`].
    ///
    /// # Errors
    /// Returns [`IdError`] when the UUID is not version 7.
    pub fn from_uuid(uuid: uuid::Uuid) -> Result<Self, IdError> {
        Self::new(uuid.to_string())
    }

    /// Parse the underlying string back to a [`uuid::Uuid`].
    ///
    /// # Panics
    /// Never: the constructor guarantees the string is a valid UUID.
    #[must_use]
    pub fn as_uuid(&self) -> uuid::Uuid {
        uuid::Uuid::parse_str(self.as_str()).expect("CardUid was validated at construction")
    }
}

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
id_type!(
    TenantSlug,
    "Human-visible tenant slug resolved to a DataTenantId at the auth boundary.",
    validate_tenant_slug
);
id_type!(
    PodId,
    "Bifrost pod identifier for multi-pod seal coordination.",
    validate_token
);

/// Immutable tenant isolation key used by tenant-scoped Wyrd and Vala rows.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, schemars::JsonSchema,
)]
#[serde(transparent)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct DataTenantId(uuid::Uuid);

impl DataTenantId {
    /// Nil-UUID sentinel that bypasses the v7 validator and represents Wyrd's
    /// system-owned tables (`SystemShared` OLAP namespace, catalog bootstrap, etc.).
    pub const SYSTEM_OWNER: DataTenantId = DataTenantId(uuid::Uuid::nil());

    /// Generate a UUIDv7-backed tenant isolation key.
    #[must_use]
    pub fn new_v7() -> Self {
        Self(uuid::Uuid::now_v7())
    }

    /// Build a tenant isolation key from a UUIDv7 value.
    ///
    /// # Errors
    /// Returns an error when the UUID is not version 7.
    pub fn new(value: uuid::Uuid) -> Result<Self, IdError> {
        if value.get_version_num() == 7 {
            Ok(Self(value))
        } else {
            Err(IdError::InvalidUuid7)
        }
    }

    /// Borrow the underlying UUID.
    #[must_use]
    pub fn as_uuid(&self) -> uuid::Uuid {
        self.0
    }
}

impl fmt::Display for DataTenantId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)
    }
}

impl FromStr for DataTenantId {
    type Err = IdError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let uuid = uuid::Uuid::parse_str(value).map_err(|_| IdError::InvalidUuid7)?;
        Self::new(uuid)
    }
}

impl TryFrom<uuid::Uuid> for DataTenantId {
    type Error = IdError;

    fn try_from(value: uuid::Uuid) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<DataTenantId> for uuid::Uuid {
    fn from(value: DataTenantId) -> Self {
        value.0
    }
}

impl<'de> Deserialize<'de> for DataTenantId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = uuid::Uuid::deserialize(deserializer)?;
        Self::new(value).map_err(serde::de::Error::custom)
    }
}

/// Generate a UUIDv7 string for Wyrd-owned identifiers.
#[must_use]
pub fn uuid7() -> String {
    uuid::Uuid::new_v7(uuid::Timestamp::now(uuid::NoContext)).to_string()
}

fn validate_token(value: &str) -> Result<(), IdError> {
    validate_token_with_min(value, 3)
}

fn validate_card_token(value: &str) -> Result<(), IdError> {
    validate_token_with_min(value, 1)
}

fn validate_token_with_min(value: &str, min_len: usize) -> Result<(), IdError> {
    if value.len() < min_len || value.len() > 64 {
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

fn validate_tenant_slug(value: &str) -> Result<(), IdError> {
    if value.is_empty() || value.len() > 63 {
        return Err(IdError::InvalidTenantSlug);
    }
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return Err(IdError::InvalidTenantSlug);
    };
    if !first.is_ascii_lowercase() && !first.is_ascii_digit() {
        return Err(IdError::InvalidTenantSlug);
    }
    if chars.all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '_' || ch == '-') {
        Ok(())
    } else {
        Err(IdError::InvalidTenantSlug)
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
    /// Tenant slug did not match `[a-z0-9][a-z0-9_-]{0,62}`.
    #[error("tenant slug must match [a-z0-9][a-z0-9_-]{{0,62}}")]
    InvalidTenantSlug,
    /// Opaque identifier was empty, too long, too short, or contained whitespace.
    #[error("opaque identifier is invalid")]
    InvalidOpaque,
}
