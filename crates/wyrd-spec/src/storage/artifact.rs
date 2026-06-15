//! Wire-only descriptor for bytes held by the storage subsystem.
//!
//! This is not a durable artifact identity. Durable artifact identity remains
//! an `Artifact` card referenced with `CardRef`.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Verified stored object descriptor returned after upload completion.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields, rename_all = "snake_case")]
pub struct StoredObjectRef {
    /// Full tenant-scoped storage path.
    pub storage_path: String,
    /// Verified byte length.
    pub size_bytes: u64,
    /// Verified base64-encoded SHA-256 digest.
    pub sha256: String,
    /// Object content type when known.
    #[serde(default)]
    pub content_type: Option<String>,
    /// Backend-specific server-side-encryption marker when known.
    #[serde(default)]
    pub sse_marker: Option<String>,
    /// Server completion timestamp.
    pub created_at: DateTime<Utc>,
}
