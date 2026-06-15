//! Storage backend identifiers used in wire payloads and audit logging.

use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

/// Relational metadata storage backend.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum MetadataBackend {
    /// PostgreSQL metadata store.
    Postgres,
}

/// Object-storage backend configured for artifact bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum StorageBackendKind {
    /// Local filesystem storage.
    Local,
    /// AWS S3 or compatible object storage.
    S3,
    /// Google Cloud Storage.
    Gcs,
    /// Azure Blob Storage.
    Azure,
}

impl fmt::Display for StorageBackendKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Local => "local",
            Self::S3 => "s3",
            Self::Gcs => "gcs",
            Self::Azure => "azure",
        })
    }
}

impl FromStr for StorageBackendKind {
    type Err = StorageBackendKindParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "local" => Ok(Self::Local),
            "s3" => Ok(Self::S3),
            "gcs" => Ok(Self::Gcs),
            "azure" => Ok(Self::Azure),
            other => Err(StorageBackendKindParseError(other.to_owned())),
        }
    }
}

/// Error returned when parsing a storage backend kind fails.
#[derive(Debug, thiserror::Error)]
#[error("invalid storage backend kind: {0}")]
pub struct StorageBackendKindParseError(String);
