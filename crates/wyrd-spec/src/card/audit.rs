//! Audit Card spec.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::card::common::NonSecretValue;
use crate::reference::CardRef;

/// Audit artifact or evidence bundle metadata.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct AuditSpec {
    /// Audit description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Subject Card references.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub subject_refs: Vec<CardRef>,
    /// Policy references.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub policy_refs: Vec<CardRef>,
    /// Evidence artifact references.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub evidence_refs: Vec<CardRef>,
    /// Free-form details.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub details: BTreeMap<String, NonSecretValue>,
}
