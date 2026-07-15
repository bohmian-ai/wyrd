//! Registration submission and response contracts.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use wyrd_semver::{VersionBlock, VersionBump, VersionSpec};

use crate::api_version::ApiVersion;
use crate::auth::PrincipalId;
use crate::envelope::{CardKind, Spec};
use crate::ids::{CardName, CardUid, SpaceName};
use crate::metadata::{Annotations, Labels};
use crate::reference::CardRef;
use crate::registry::{
    CardLifecycleStatus, RegisterOutcome, RegistrationOperationId, RelativeArtifactPath,
};
use crate::storage::UploadInitResponse;

/// Metadata accepted on a registration submission.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields, rename_all = "snake_case")]
pub struct SubmissionMetadata {
    /// Card name.
    pub name: CardName,
    /// Requested version or version scope.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<VersionSpec>,
    /// Version bump requested when version is omitted or scoped.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bump: Option<VersionBump>,
    /// Optional workspace.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub space: Option<SpaceName>,
    /// Optional client-provided UID, used only where the server permits it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uid: Option<CardUid>,
    /// Display labels.
    #[serde(default, skip_serializing_if = "Labels::is_empty")]
    pub labels: Labels,
    /// Free-form annotations.
    #[serde(default, skip_serializing_if = "Annotations::is_empty")]
    pub annotations: Annotations,
}

/// Card envelope submitted for registration, without server-managed fields.
#[derive(Debug, Clone, PartialEq, Serialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields, rename_all = "snake_case")]
pub struct CardSubmission {
    /// API version.
    #[serde(rename = "apiVersion")]
    pub api_version: ApiVersion,
    /// Card kind.
    pub kind: CardKind,
    /// Author-supplied metadata.
    pub metadata: SubmissionMetadata,
    /// Kind-specific spec.
    #[schemars(with = "serde_json::Value")]
    pub spec: Spec,
}

impl<'de> Deserialize<'de> for CardSubmission {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields, rename_all = "snake_case")]
        struct RawCardSubmission {
            #[serde(rename = "apiVersion")]
            api_version: ApiVersion,
            kind: CardKind,
            metadata: SubmissionMetadata,
            spec: serde_json::Value,
        }

        let raw = RawCardSubmission::deserialize(deserializer)?;
        let spec =
            Spec::from_kind_and_value(&raw.kind, raw.spec).map_err(serde::de::Error::custom)?;
        Ok(Self {
            api_version: raw.api_version,
            kind: raw.kind,
            metadata: raw.metadata,
            spec,
        })
    }
}

/// One artifact expected by a registration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields, rename_all = "snake_case")]
pub struct ArtifactManifestEntry {
    /// Relative artifact path.
    pub relative_path: RelativeArtifactPath,
    /// Base64-encoded SHA-256 digest.
    pub expected_sha256: String,
    /// Expected byte length.
    pub expected_size_bytes: i64,
    /// Optional MIME type.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_type: Option<String>,
}

/// Create-card request.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields, rename_all = "snake_case")]
pub struct CreateCardRequest {
    /// Submitted card data.
    pub card: CardSubmission,
    /// Artifact manifest, empty for metadata-only cards.
    #[serde(default)]
    pub artifacts: Vec<ArtifactManifestEntry>,
}

/// Server response to card creation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields, rename_all = "snake_case")]
pub struct CreateCardResponse {
    /// Resolved card UID.
    pub card_uid: CardUid,
    /// Resolved kind.
    pub kind: CardKind,
    /// Resolved workspace.
    pub space: SpaceName,
    /// Resolved name.
    pub name: CardName,
    /// Resolved version.
    pub version: VersionBlock,
    /// BLAKE3-hex hash of JCS canonical spec bytes.
    pub spec_hash: String,
    /// BLAKE3-hex hash of the JCS canonical manifest.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifact_hash: Option<String>,
    /// Registration result.
    pub outcome: RegisterOutcome,
    /// Server-managed lifecycle status.
    pub status: CardLifecycleStatus,
    /// Creation timestamp.
    pub created_at: DateTime<Utc>,
    /// Principal attributed to the operation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub principal_id: Option<PrincipalId>,
    /// Pending operation id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub operation_id: Option<RegistrationOperationId>,
    /// Upload plans for artifacts that still need bytes.
    #[serde(default)]
    pub uploads: Vec<UploadInitResponse>,
}

#[cfg(test)]
mod submission_tests {
    use super::{CardSubmission, CreateCardResponse, SubmissionMetadata};
    use crate::api_version::ApiVersion;
    use crate::envelope::CardKind;
    use crate::ids::{CardName, CardUid, SpaceName};
    use crate::registry::{CardLifecycleStatus, RegisterOutcome};
    use chrono::Utc;
    use serde_json::json;
    use wyrd_semver::VersionBlock;

    #[test]
    fn card_submission_deserializes_from_the_flat_wire_shape() {
        let body = json!({
            "apiVersion": "wyrd/v1",
            "kind": "Audit",
            "metadata": {
                "name": "audit-a",
            },
            "spec": {}
        });
        let submission: CardSubmission =
            serde_json::from_value(body).expect("flat wire submission deserializes");
        assert_eq!(submission.api_version.as_str(), ApiVersion::V1);
        assert_eq!(submission.kind, CardKind::Audit);
        assert_eq!(submission.metadata.name.as_str(), "audit-a");
    }

    #[test]
    fn card_submission_rejects_unknown_top_level_fields() {
        let body = json!({
            "apiVersion": "wyrd/v1",
            "kind": "Audit",
            "metadata": { "name": "audit-a" },
            "spec": {},
            "status": "active",
        });
        let error = serde_json::from_value::<CardSubmission>(body)
            .expect_err("unknown top-level fields must be rejected");
        assert!(
            error.to_string().contains("status"),
            "error message must name the rejected field, got {error}",
        );
    }

    #[test]
    fn submission_metadata_rejects_server_managed_fields() {
        let body = json!({
            "name": "greeter",
            "card_uid": "01890f28-7c4a-7cc3-98e7-4f4a3c2d1b00",
        });
        serde_json::from_value::<SubmissionMetadata>(body)
            .expect_err("card_uid is server-derived and must not appear in submission metadata");
    }

    #[test]
    fn create_card_response_carries_server_managed_fields() {
        let response = CreateCardResponse {
            card_uid: CardUid::new("01890f28-7c4a-7cc3-98e7-4f4a3c2d1b00").unwrap(),
            kind: CardKind::Prompt,
            space: SpaceName::new("default").unwrap(),
            name: CardName::new("greeter").unwrap(),
            version: VersionBlock::parse("1.0.0").unwrap(),
            spec_hash: "abc".to_owned(),
            artifact_hash: None,
            outcome: RegisterOutcome::Created,
            status: CardLifecycleStatus::Active,
            created_at: Utc::now(),
            principal_id: None,
            operation_id: None,
            uploads: Vec::new(),
        };
        let encoded = serde_json::to_value(&response).unwrap();
        assert!(encoded.get("card_uid").is_some());
        assert!(encoded.get("spec_hash").is_some());
        assert_eq!(encoded["outcome"], "created");
        assert_eq!(encoded["status"], "active");
    }
}

/// Server-resolved registration receipt.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields, rename_all = "snake_case")]
pub struct RegistrationReceipt {
    /// Resolved card reference.
    pub card_ref: CardRef,
    /// Registration result.
    pub outcome: RegisterOutcome,
    /// Lifecycle status.
    pub status: CardLifecycleStatus,
    /// Creation timestamp.
    pub created_at: DateTime<Utc>,
    /// Principal attributed to the operation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub principal_id: Option<PrincipalId>,
}
