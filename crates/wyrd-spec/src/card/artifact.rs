//! Artifact Card spec.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::card::common::NonSecretValue;
use crate::error::WyrdError;
use crate::reference::Ref;

/// Versioned artifact descriptor.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct ArtifactSpec {
    /// Artifact kind.
    pub artifact_kind: String,
    /// Artifact URIs.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub artifact_uris: Vec<String>,
    /// MIME content type.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_type: Option<String>,
    /// Size in bytes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size_bytes: Option<u64>,
    /// Integrity digest.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub integrity: Option<String>,
    /// Schema Card reference.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schema_ref: Option<Ref>,
    /// Framework adapter metadata for runtime loaders.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub framework_adapter: Option<FrameworkAdapterRef>,
    /// External URI.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub external_uri: Option<String>,
    /// Artifact metadata.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub metadata: BTreeMap<String, NonSecretValue>,
}

impl ArtifactSpec {
    /// Validates the declared descriptor: `artifact_kind` is an open
    /// vocabulary but must name something.
    ///
    /// # Errors
    /// Returns [`WyrdError::Validation`] with rule `artifact_kind` when
    /// `artifact_kind` is empty or whitespace-only.
    pub fn validate(&self) -> Result<(), WyrdError> {
        if self.artifact_kind.trim().is_empty() {
            return Err(WyrdError::Validation {
                message: "spec.artifact_kind: must not be empty".to_owned(),
                details: serde_json::json!({ "rule": "artifact_kind", "path": "spec.artifact_kind" }),
            });
        }
        Ok(())
    }
}

/// Framework adapter reference for loading or invoking artifacts.
// Status: Locked (2026-05-18)
// source: execution/PLAN_DELTA_LEDGER.md#plan-delta-aah-6
// source: execution/PLAN_DELTA_LEDGER.md#plan-delta-aah-7
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct FrameworkAdapterRef {
    /// Adapter name.
    pub name: String,
    /// Adapter version or version range.
    pub version: String,
    /// Non-secret adapter configuration.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub config: BTreeMap<String, NonSecretValue>,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A blank `artifact_kind` is refused; any named kind is accepted.
    ///
    /// # Panics
    /// Panics when a blank kind validates or a named kind is refused.
    #[test]
    fn artifact_kind_must_name_something() {
        for blank in ["", "  \t"] {
            let spec = ArtifactSpec {
                artifact_kind: blank.to_owned(),
                ..ArtifactSpec::default()
            };
            assert!(spec.validate().is_err(), "{blank:?}");
        }
        let spec = ArtifactSpec {
            artifact_kind: "policy-bundle".to_owned(),
            ..ArtifactSpec::default()
        };
        assert!(spec.validate().is_ok());
    }
}
