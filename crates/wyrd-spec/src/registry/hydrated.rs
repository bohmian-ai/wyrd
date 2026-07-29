//! Pure wire projections for an offline hydrated Card bundle.

use serde::{Deserialize, Serialize};

use crate::reference::CardRef;

/// Hydration depth recorded in a published local Card bundle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HydrationMode {
    /// Persist Card metadata and artifact inventories without payload bytes.
    #[serde(rename = "metadata")]
    MetadataOnly,
    /// Persist Card metadata and verified artifact payload bytes.
    #[serde(rename = "complete")]
    Complete,
}

impl std::fmt::Display for HydrationMode {
    /// Format the stable hydration mode stored in the bundle manifest.
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::MetadataOnly => "metadata",
            Self::Complete => "complete",
        })
    }
}

/// Stable metadata written to `metadata.yaml` at a hydrated bundle root.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HydratedBundleManifest {
    /// Bundle format version.
    #[serde(rename = "apiVersion")]
    pub api_version: String,
    /// Hydration depth used to create the bundle.
    pub hydration: HydrationMode,
    /// Exact resolved root reference.
    pub root: CardRef,
    /// Per-Card local projections.
    pub cards: Vec<HydratedCardManifest>,
    /// Number of unique Cards in the bundle.
    pub card_count: usize,
    /// Number of artifact inventory entries.
    pub artifact_count: usize,
    /// Number of verified artifact payloads materialized locally.
    pub downloaded_artifact_count: usize,
}

/// One Card's stable hydrated-bundle projection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HydratedCardManifest {
    /// Every user-facing alias resolving to this exact Card.
    pub aliases: Vec<String>,
    /// Exact resolved Card reference.
    pub card_ref: CardRef,
    /// Relative path to the Card envelope.
    pub card_path: String,
    /// Relative path to the relationship projection.
    pub relationships_path: String,
    /// Relative path to the artifact inventory.
    pub artifact_inventory_path: String,
    /// Artifact inventory and local payload projections.
    pub artifacts: Vec<HydratedArtifactManifest>,
}

/// One artifact inventory entry in a hydrated bundle.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HydratedArtifactManifest {
    /// Relative server-owned artifact path.
    pub relative_path: String,
    /// Base64-encoded SHA-256 supplied by the server.
    pub sha256: String,
    /// Server-recorded byte length.
    pub size_bytes: i64,
    /// Optional MIME type.
    pub content_type: Option<String>,
    /// Relative local payload path when complete hydration downloaded the file.
    pub local_path: Option<String>,
}
