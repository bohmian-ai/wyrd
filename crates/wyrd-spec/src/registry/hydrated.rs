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

/// Renders hydration modes using their stable manifest values.
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

/// Wire-compatibility fixtures for hydrated bundle projections.
#[cfg(test)]
mod tests {
    use crate::{
        envelope::CardKind,
        ids::{CardName, SpaceName},
        reference::CardRef,
    };
    use wyrd_semver::VersionBlock;

    use super::{
        HydratedArtifactManifest, HydratedBundleManifest, HydratedCardManifest, HydrationMode,
    };

    /// Build the exact Service identity shared by the manifest root and Card projection.
    ///
    /// # Panics
    ///
    /// Panics only if repository-owned static identity literals stop satisfying
    /// the corresponding domain invariants.
    fn service_ref() -> CardRef {
        CardRef {
            kind: CardKind::Service,
            name: CardName::new("service").expect("static Card name is valid"),
            version: VersionBlock::parse("1.0.0").expect("static Card version is valid"),
            space: Some(SpaceName::new("default").expect("static Card space is valid")),
            uid: None,
        }
    }

    /// Hydrated bundle DTO ownership preserves every field and its canonical JSON bytes.
    #[test]
    fn hydrated_bundle_manifest_preserves_wire_bytes() {
        let card_ref = service_ref();
        let manifest = HydratedBundleManifest {
            api_version: "wyrd/hydrated-bundle/v1".to_owned(),
            hydration: HydrationMode::Complete,
            root: card_ref.clone(),
            cards: vec![HydratedCardManifest {
                aliases: vec!["root".to_owned()],
                card_ref,
                card_path: "cards/service/card.yaml".to_owned(),
                relationships_path: "cards/service/relationships.yaml".to_owned(),
                artifact_inventory_path: "cards/service/artifacts.yaml".to_owned(),
                artifacts: vec![HydratedArtifactManifest {
                    relative_path: "model.joblib".to_owned(),
                    sha256: "YWJj".to_owned(),
                    size_bytes: 3,
                    content_type: Some("application/octet-stream".to_owned()),
                    local_path: Some("cards/service/artifacts/model.joblib".to_owned()),
                }],
            }],
            card_count: 1,
            artifact_count: 1,
            downloaded_artifact_count: 1,
        };

        let bytes = serde_json::to_vec(&manifest).expect("hydrated manifest serializes");
        assert_eq!(
            bytes,
            br#"{"apiVersion":"wyrd/hydrated-bundle/v1","hydration":"complete","root":{"kind":"Service","name":"service","version":"1.0.0","space":"default"},"cards":[{"aliases":["root"],"card_ref":{"kind":"Service","name":"service","version":"1.0.0","space":"default"},"card_path":"cards/service/card.yaml","relationships_path":"cards/service/relationships.yaml","artifact_inventory_path":"cards/service/artifacts.yaml","artifacts":[{"relative_path":"model.joblib","sha256":"YWJj","size_bytes":3,"content_type":"application/octet-stream","local_path":"cards/service/artifacts/model.joblib"}]}],"card_count":1,"artifact_count":1,"downloaded_artifact_count":1}"#
        );
        let round_trip: HydratedBundleManifest =
            serde_json::from_slice(&bytes).expect("hydrated manifest deserializes");
        assert_eq!(round_trip, manifest);
    }
}
