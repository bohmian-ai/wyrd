//! Storage backend identifiers.

use serde::{Deserialize, Serialize};

/// Relational metadata storage backend.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum MetadataBackend {
    /// SQLite metadata store.
    Sqlite,
    /// PostgreSQL metadata store.
    Postgres,
    /// MySQL metadata store.
    Mysql,
}

/// Artifact/blob storage backend.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum ArtifactBackend {
    /// Local filesystem artifacts.
    Local,
    /// Amazon S3-compatible object storage.
    S3,
    /// Google Cloud Storage.
    Gcs,
    /// Azure Blob Storage.
    Azure,
}
