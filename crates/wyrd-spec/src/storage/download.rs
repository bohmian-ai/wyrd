//! Download wire contracts.

use crate::ids::CardUid;
use serde::{Deserialize, Serialize};

/// Client request to initialize a download.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields, rename_all = "snake_case")]
pub struct DownloadInitRequest {
    /// Card that owns the stored object.
    pub card_uid: CardUid,
    /// Object path under the card.
    pub relative_path: String,
    /// Optional presign TTL override in seconds.
    #[serde(default)]
    pub ttl_secs: Option<u32>,
}

/// Server response to a download-init request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields, rename_all = "snake_case")]
pub struct DownloadInitResponse {
    /// Backend-specific download plan.
    pub plan: DownloadPlan,
    /// Expected object size in bytes.
    pub size_bytes: u64,
    /// Expected base64-encoded SHA-256 digest.
    pub sha256: String,
}

/// Presigned GET plan for any backend.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields, rename_all = "snake_case")]
pub struct DownloadPlan {
    /// Presigned GET URL or local server route URL.
    pub get_url: String,
    /// URL time-to-live in seconds.
    pub ttl_secs: u32,
}
