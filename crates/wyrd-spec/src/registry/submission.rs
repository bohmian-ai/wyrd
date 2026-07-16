//! Composite card registration request and response contracts.

use serde::{Deserialize, Serialize};
use url::Url;

use crate::api_version::ApiVersion;
use crate::envelope::{CardKind, Metadata};
use crate::reference::CardRef;
use crate::registry::{CardLifecycleStatus, PresignedUpload, RegistrationOutcomeKind};

/// One card submitted for registration.
///
/// The server derives relationships, lifecycle status, resolved version, and
/// UID. A submission is therefore intentionally smaller than a stored Card
/// envelope and cannot carry those server-managed fields.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields, rename_all = "snake_case")]
pub struct CardSubmission {
    /// API version.
    #[serde(rename = "apiVersion")]
    pub api_version: ApiVersion,
    /// Card kind.
    pub kind: CardKind,
    /// Author-supplied card metadata.
    pub metadata: Metadata,
    /// Kind-specific spec body.
    #[schemars(with = "serde_json::Value")]
    #[cfg_attr(feature = "server", schema(value_type = serde_json::Value))]
    pub spec: serde_json::Value,
    /// Heavy artifact manifest for this submission.
    #[serde(default)]
    pub artifacts: Vec<ArtifactManifestEntry>,
}

/// One heavy artifact declared by a submission.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields, rename_all = "snake_case")]
pub struct ArtifactManifestEntry {
    /// Validated path below the registered card's artifact prefix.
    pub relative_path: crate::registry::RelativeArtifactPath,
    /// Base64-encoded SHA-256 digest of the artifact bytes.
    pub sha256: String,
    /// Declared artifact size in bytes.
    pub size_bytes: u64,
    /// Optional MIME type for the stored object.
    pub content_type: Option<String>,
}

/// Composite registration request.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields, rename_all = "snake_case")]
pub struct CreateCardRequest {
    /// Flat list of cards. The server topo-sorts the graph and derives root.
    pub submissions: Vec<CardSubmission>,
}

/// Outcome for one submission in a composite registration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields, rename_all = "snake_case")]
pub struct CardRegistrationOutcome {
    /// Server-resolved card identity.
    pub card_ref: CardRef,
    /// BLAKE3 hex hash of the JCS-canonical resolved spec.
    pub spec_hash: String,
    /// BLAKE3 hex hash of the sorted artifact manifest, when present.
    pub artifact_hash: Option<String>,
    /// Server-managed lifecycle state.
    pub status: CardLifecycleStatus,
    /// Registration result for this submission.
    pub outcome: RegistrationOutcomeKind,
    /// URI of the durable card blob once written.
    #[schemars(with = "Option<String>")]
    #[cfg_attr(feature = "server", schema(value_type = Option<String>))]
    pub card_blob_uri: Option<Url>,
}

/// Upload plan group keyed by the resolved card reference.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields, rename_all = "snake_case")]
pub struct CardUploadPlan {
    /// Card whose submission declared the artifact manifest.
    pub card_ref: CardRef,
    /// One presigned upload per manifest entry.
    pub entries: Vec<PresignedUpload>,
}

/// Composite registration response.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields, rename_all = "snake_case")]
pub struct CreateCardResponse {
    /// Server-derived graph root.
    pub root: CardRef,
    /// Per-submission outcomes in server topo order.
    pub outcomes: Vec<CardRegistrationOutcome>,
    /// Upload plans for artifact-bearing submissions.
    #[serde(default)]
    pub upload_plans: Vec<CardUploadPlan>,
}

/// SDK-facing projection of a composite registration response.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields, rename_all = "snake_case")]
pub struct RegistrationReceipt {
    /// Server-derived graph root.
    pub root: CardRef,
    /// Per-submission registration outcomes.
    pub outcomes: Vec<CardRegistrationOutcome>,
}

#[cfg(test)]
mod tests {
    use super::{CardSubmission, CreateCardRequest, CreateCardResponse};
    use crate::api_version::ApiVersion;
    use crate::envelope::{CardKind, Metadata};
    use crate::registry::{CardLifecycleStatus, RegistrationOutcomeKind};
    use serde_json::json;

    fn submission() -> CardSubmission {
        CardSubmission {
            api_version: ApiVersion::v1(),
            kind: CardKind::Audit,
            metadata: Metadata {
                name: "audit-a".parse().expect("test name is valid"),
                version: None,
                bump: None,
                space: Some("default".parse().expect("test space is valid")),
                uid: None,
                labels: Default::default(),
                annotations: Default::default(),
                spec_hash: None,
                artifact_hash: None,
                origin: None,
            },
            spec: json!({}),
            artifacts: Vec::new(),
        }
    }

    #[test]
    fn create_card_request_is_flat_submissions_list() {
        let request: CreateCardRequest = serde_json::from_value(json!({
            "submissions": [{
                "apiVersion": "wyrd/v1",
                "kind": "Audit",
                "metadata": {"name": "audit-a", "space": "default"},
                "spec": {}
            }]
        }))
        .expect("flat request deserializes");
        assert_eq!(request.submissions.len(), 1);
        assert!(
            serde_json::from_value::<CreateCardRequest>(json!({
                "card": {}, "artifacts": []
            }))
            .is_err()
        );
    }

    #[test]
    fn card_submission_artifacts_default_is_empty() {
        let encoded = serde_json::to_value(submission()).expect("submission serializes");
        assert!(encoded["artifacts"].as_array().is_some());
        assert!(encoded["artifacts"].as_array().expect("array").is_empty());
    }

    #[test]
    fn card_submission_server_managed_fields_are_not_top_level_fields() {
        let mut encoded = serde_json::to_value(submission()).expect("submission serializes");
        encoded["relationships"] = json!([]);
        encoded["status"] = json!("active");
        assert!(serde_json::from_value::<CardSubmission>(encoded).is_err());
    }

    #[test]
    fn create_card_response_carries_root_outcomes_and_upload_plans() {
        let response = CreateCardResponse {
            root: serde_json::from_value(json!({
                "kind": "Audit", "name": "audit-a", "space": "default", "version": "1.0.0"
            }))
            .expect("card ref deserializes"),
            outcomes: Vec::new(),
            upload_plans: Vec::new(),
        };
        assert!(response.outcomes.is_empty());
        assert!(response.upload_plans.is_empty());
        let _ = (
            CardLifecycleStatus::Pending,
            RegistrationOutcomeKind::Registered,
        );
    }
}
