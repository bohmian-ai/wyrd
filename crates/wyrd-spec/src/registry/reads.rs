//! Card read and list contracts.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use wyrd_semver::VersionBlock;

use crate::envelope::{Card, CardKind};
use crate::ids::{CardName, CardUid, SpaceName};
use crate::query::MetadataQuery;
use crate::reference::CardRef;
use crate::registry::{CardLifecycleStatus, RelativeArtifactPath};

/// Full card response.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields, rename_all = "snake_case")]
pub struct GetCardResponse {
    /// Card envelope.
    pub card: Card,
    /// Creation time.
    pub created_at: DateTime<Utc>,
    /// Last update time.
    pub updated_at: DateTime<Utc>,
}

/// Soft-delete response.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields, rename_all = "snake_case")]
pub struct DeleteCardResponse {
    /// Card UID.
    pub card_uid: CardUid,
    /// True only on the first delete.
    pub deleted: bool,
}

/// Metadata-only card list item.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields, rename_all = "snake_case")]
pub struct CardSummary {
    /// Card UID.
    pub card_uid: CardUid,
    /// Card kind.
    pub kind: CardKind,
    /// Workspace.
    pub space: SpaceName,
    /// Card name.
    pub name: CardName,
    /// Resolved version.
    pub version: VersionBlock,
    /// BLAKE3-hex spec hash.
    pub spec_hash: String,
    /// Optional BLAKE3-hex artifact hash.
    pub artifact_hash: Option<String>,
    /// Lifecycle status.
    pub status: CardLifecycleStatus,
    /// Creation time.
    pub created_at: DateTime<Utc>,
    /// Last update time.
    pub updated_at: DateTime<Utc>,
}

/// Query parameters for card listing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields, rename_all = "snake_case")]
pub struct ListCardsRequest {
    /// Optional kind filter.
    pub kind: Option<CardKind>,
    /// Optional workspace filter.
    pub space: Option<SpaceName>,
    /// Optional name filter.
    pub name: Option<CardName>,
    /// Optional semver range filter.
    pub version_range: Option<String>,
    /// Optional lifecycle filter.
    pub status: Option<CardLifecycleStatus>,
    /// Optional metadata query encoded as its canonical query-language string.
    #[serde(
        serialize_with = "metadata_query_option::serialize",
        deserialize_with = "metadata_query_option::deserialize"
    )]
    pub filter: Option<MetadataQuery>,
    /// Include prerelease versions.
    #[serde(default)]
    pub include_prerelease: bool,
    /// Maximum result count.
    pub limit: Option<i32>,
    /// Opaque continuation cursor.
    pub cursor: Option<String>,
}

mod metadata_query_option {
    use super::MetadataQuery;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<S>(value: &Option<MetadataQuery>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        value
            .as_ref()
            .map(ToString::to_string)
            .serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<MetadataQuery>, D::Error>
    where
        D: Deserializer<'de>,
    {
        Option::<String>::deserialize(deserializer)?
            .map(|value| MetadataQuery::parse(&value).map_err(serde::de::Error::custom))
            .transpose()
    }
}

/// A page of card summaries.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields, rename_all = "snake_case")]
pub struct ListCardsResponse {
    /// Page items.
    pub items: Vec<CardSummary>,
    /// Opaque continuation cursor.
    pub next_cursor: Option<String>,
}

/// Versions available for one card identity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields, rename_all = "snake_case")]
pub struct ListVersionsResponse {
    /// Resolved versions.
    pub versions: Vec<VersionBlock>,
}

/// Exact card lookup discriminator.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum CardLocator {
    /// Lookup by UID.
    Uid(CardUid),
    /// Lookup by exact reference.
    Ref(CardRef),
}

/// Stored artifact inventory response.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields, rename_all = "snake_case")]
pub struct ArtifactInventoryResponse {
    /// Stored artifacts.
    pub artifacts: Vec<StoredArtifactEntry>,
}

/// One stored artifact.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields, rename_all = "snake_case")]
pub struct StoredArtifactEntry {
    /// Relative artifact path.
    pub relative_path: RelativeArtifactPath,
    /// Base64-encoded SHA-256.
    pub sha256: String,
    /// Stored byte length.
    pub size_bytes: i64,
    /// Optional MIME type.
    pub content_type: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::{ArtifactInventoryResponse, CardLocator, ListCardsRequest, StoredArtifactEntry};
    use crate::ids::CardUid;
    use crate::query::{FieldRef, MetadataQuery, Operator, Predicate, Value};
    use crate::registry::RelativeArtifactPath;
    use serde_json::json;

    #[test]
    fn card_locator_uid_variant_uses_snake_case_tag() {
        let uid = CardUid::new("01890f28-7c4a-7cc3-98e7-4f4a3c2d1b00").unwrap();
        let encoded = serde_json::to_value(CardLocator::Uid(uid.clone())).unwrap();
        assert_eq!(encoded["uid"], "01890f28-7c4a-7cc3-98e7-4f4a3c2d1b00");
        let decoded: CardLocator =
            serde_json::from_value(encoded).expect("locator round-trips through JSON");
        assert!(matches!(decoded, CardLocator::Uid(back) if back == uid));
    }

    #[test]
    fn artifact_inventory_response_round_trips() {
        let payload = json!({
            "artifacts": [
                {
                    "relative_path": "weights/model.bin",
                    "sha256": "abc",
                    "size_bytes": 42,
                    "content_type": "application/octet-stream",
                }
            ]
        });
        let decoded: ArtifactInventoryResponse =
            serde_json::from_value(payload).expect("inventory deserializes");
        assert_eq!(decoded.artifacts.len(), 1);
        assert_eq!(
            decoded.artifacts[0].relative_path.as_str(),
            "weights/model.bin"
        );
        let re_encoded = serde_json::to_value(&decoded).unwrap();
        assert_eq!(re_encoded["artifacts"][0]["size_bytes"], 42);
    }

    #[test]
    fn stored_artifact_entry_denies_unknown_fields() {
        let payload = json!({
            "relative_path": "weights/model.bin",
            "sha256": "abc",
            "size_bytes": 42,
            "content_type": null,
            "server_id": "1"
        });
        serde_json::from_value::<StoredArtifactEntry>(payload)
            .expect_err("unknown stored-artifact fields must be rejected");
    }

    #[test]
    fn relative_artifact_path_via_stored_entry_rejects_absolute_paths() {
        let bad = json!({
            "relative_path": "/etc/passwd",
            "sha256": "abc",
            "size_bytes": 42,
            "content_type": null,
        });
        serde_json::from_value::<StoredArtifactEntry>(bad)
            .expect_err("absolute paths must be rejected in wire values");

        let path = RelativeArtifactPath::new("weights/model.bin")
            .expect("well-formed relative path is accepted");
        assert_eq!(path.as_str(), "weights/model.bin");
    }

    #[test]
    fn list_filter_serializes_as_canonical_query_string() {
        let request = ListCardsRequest {
            kind: None,
            space: None,
            name: None,
            version_range: None,
            status: None,
            filter: Some(MetadataQuery::Predicate(Predicate {
                field: FieldRef::Label("stage".to_owned()),
                op: Operator::Eq(Value::String("prod".to_owned())),
            })),
            include_prerelease: false,
            limit: Some(20),
            cursor: None,
        };
        let encoded = serde_urlencoded::to_string(&request).unwrap();
        assert!(encoded.contains("filter=labels.stage+%3D+%22prod%22"));
        assert_eq!(
            serde_urlencoded::from_str::<ListCardsRequest>(&encoded).unwrap(),
            request
        );
    }
}
