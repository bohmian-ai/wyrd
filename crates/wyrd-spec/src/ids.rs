//! Newtype identifiers used across Wyrd specs.

use std::fmt::{self, Display, Formatter, Result as FmtResult};
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize};
use uuid::Uuid;

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
    "Feature column name referenced by a drift Verifier signal.",
    validate_card_token
);
id_type!(
    MediaBindingId,
    "Name of a `${media:id}` binding slot in a resolved judge Prompt.",
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
id_type!(
    ProviderId,
    "Tenant-visible gateway provider identifier using the canonical Wyrd name grammar.",
    validate_token
);
id_type!(
    ModelId,
    "Provider-native gateway model identifier: 1..=255 UTF-8 bytes without control characters.",
    validate_model_id
);
id_type!(
    ProviderCredentialName,
    "Tenant-unique gateway provider credential name.",
    validate_token
);
id_type!(
    ProviderDeploymentName,
    "Tenant-unique gateway provider deployment name.",
    validate_token
);
id_type!(
    CredentialBindingName,
    "Operator-configured gateway credential binding name.",
    validate_token
);
id_type!(
    SecretBackendName,
    "Operator-configured external secret backend identity.",
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
    /// Fixed `UUIDv7` of the platform system tenant (`wyrd-system`).
    ///
    /// It owns system-attributed data such as audit decisions that cannot be
    /// attributed to a caller's tenant. It is an ordinary valid tenant id, so
    /// every decoder accepts it without a special case.
    pub const SYSTEM_OWNER: DataTenantId = DataTenantId(uuid::Uuid::from_u128(
        0x0000_0000_0000_7000_8000_0000_0000_0000,
    ));

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

/// Define a server-minted `UUIDv7` identity newtype.
///
/// Every generated type serializes as the canonical hyphenated UUID, mints
/// fresh values with [`Uuid::now_v7`], and refuses any non-v7 UUID on
/// construction, parsing, and deserialization, so a stored or submitted
/// identity of another version never becomes a typed value.
macro_rules! uuid7_id_type {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(
            Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize,
            schemars::JsonSchema,
        )]
        #[serde(transparent)]
        #[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
        pub struct $name(Uuid);

        impl $name {
            /// Mint a fresh identity.
            #[must_use]
            pub fn new_v7() -> Self {
                Self(uuid::Uuid::now_v7())
            }

            /// Build an identity from a stored or decoded UUID.
            ///
            /// # Errors
            /// Returns [`IdError::InvalidUuid7`] when the UUID is not version 7.
            pub fn new(value: Uuid) -> Result<Self, IdError> {
                if value.get_version_num() == 7 {
                    Ok(Self(value))
                } else {
                    Err(IdError::InvalidUuid7)
                }
            }

            /// Return the underlying UUID.
            #[must_use]
            pub fn as_uuid(&self) -> Uuid {
                self.0
            }
        }

        impl Display for $name {
            /// Render the canonical hyphenated UUID form used on the wire.
            fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
                Display::fmt(&self.0, f)
            }
        }

        impl FromStr for $name {
            /// The identifier refusal a malformed or non-v7 identity produces.
            type Err = IdError;

            /// Parse a path or query value, refusing a malformed or non-v7 UUID.
            ///
            /// # Errors
            /// Returns [`IdError::InvalidUuid7`] when `value` is not a UUID or
            /// is a UUID of any version other than 7.
            fn from_str(value: &str) -> Result<Self, Self::Err> {
                let uuid = uuid::Uuid::parse_str(value).map_err(|_| IdError::InvalidUuid7)?;
                Self::new(uuid)
            }
        }

        impl<'de> Deserialize<'de> for $name {
            /// Decode a UUID and refuse any version other than 7.
            ///
            /// # Errors
            /// Returns the deserializer's error when the value is not a UUID,
            /// or a custom error carrying [`IdError::InvalidUuid7`] for a
            /// non-v7 UUID.
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: Deserializer<'de>,
            {
                let value = uuid::Uuid::deserialize(deserializer)?;
                Self::new(value).map_err(serde::de::Error::custom)
            }
        }
    };
}

uuid7_id_type!(
    /// Durable identity of one projected verification binding.
    ///
    /// A binding is an inline `verified_by` entry on a Service, one of its
    /// component occurrences, or a standalone Agent; it is never authored with
    /// its own name or identifier. Registration mints this `UUIDv7` on the
    /// first projection of the binding's natural key — tenant, exact owner
    /// Card UID, subject occurrence, and exact Verifier UID — and every later
    /// projection of that same key keeps it, so status, runs, and results
    /// address the binding stably.
    BindingId
);

uuid7_id_type!(
    /// Durable identity of one Verifier run in `wyrd.verifier_runs`.
    ///
    /// The server mints it when scheduled, manual, or observation work is
    /// enqueued. It is the managed `run_id` of every result and detail row the
    /// run publishes and addresses the run's status and Operator dispatches.
    /// Retries and lease reclaims keep it.
    VerificationRunId
);

uuid7_id_type!(
    /// Durable identity of one canonical Verification Result.
    ///
    /// The runner mints it for a completed run's result batches: the summary
    /// row in `vala.verification.results` and every detail row join on
    /// (`data_tenant_id`, `result_id`), and the settled run points at it.
    VerificationResultId
);

uuid7_id_type!(
    /// Durable identity of one Operator dispatch in `wyrd.operator_dispatches`.
    ///
    /// Settlement of a failed binding-created run mints one per distinct
    /// configured Operator. Every delivery retry keeps it, so it is also the
    /// idempotency key an external destination may honor.
    OperatorDispatchId
);

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

/// Accepts a provider-native model identifier.
///
/// Provider catalogs are open-ended, so the grammar only bounds the value to
/// 1..=255 UTF-8 bytes and rejects control characters; slashes are allowed.
///
/// # Errors
///
/// Returns [`IdError::InvalidModelId`] for an empty, over-long, or
/// control-character-bearing value.
fn validate_model_id(value: &str) -> Result<(), IdError> {
    if value.is_empty() || value.len() > 255 || value.chars().any(char::is_control) {
        return Err(IdError::InvalidModelId);
    }
    Ok(())
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
    /// Model identifier was empty, longer than 255 bytes, or held a control character.
    #[error("model identifier must be 1..=255 UTF-8 bytes without control characters")]
    InvalidModelId,
}
