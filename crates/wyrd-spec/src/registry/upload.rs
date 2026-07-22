//! Server-minted upload response contracts for card artifacts.

use serde::{Deserialize, Serialize};

use crate::registry::RelativeArtifactPath;
use crate::storage::{UploadId, UploadPlan};

/// One tenant-scoped, server-minted upload for a card artifact.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields, rename_all = "snake_case")]
pub struct CardUploadEntry {
    /// Manifest path this upload accepts.
    pub relative_path: RelativeArtifactPath,
    /// Durable storage upload identifier used for part URLs and completion.
    pub upload_id: UploadId,
    /// Backend-specific transfer protocol minted by the storage service.
    pub plan: UploadPlan,
}
