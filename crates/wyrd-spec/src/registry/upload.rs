//! Presigned upload response contracts for card artifacts.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::registry::RelativeArtifactPath;

/// HTTP method permitted for a presigned upload.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "UPPERCASE")]
pub enum HttpMethod {
    /// Upload bytes with a single PUT request.
    Put,
}

/// One tenant-scoped, single-artifact presigned upload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields, rename_all = "snake_case")]
pub struct PresignedUpload {
    /// Manifest path this URL accepts.
    pub relative_path: RelativeArtifactPath,
    /// Presigned PUT URL.
    #[schemars(with = "String")]
    #[cfg_attr(feature = "server", schema(value_type = String))]
    pub url: url::Url,
    /// Only `PUT` is legal for v1 artifact uploads.
    pub method: HttpMethod,
    /// URL expiration instant.
    pub expires_at: DateTime<Utc>,
    /// Headers required by the storage provider.
    #[serde(default)]
    pub required_headers: Vec<(String, String)>,
}
