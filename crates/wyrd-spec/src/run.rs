//! Run identifiers and run-kind taxonomy.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::ids::SpaceName;
use crate::metadata::Labels;

/// Execution kind recorded as a Run.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[non_exhaustive]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum RunKind {
    /// Training execution.
    Training,
    /// Inference execution.
    Inference,
    /// Offline or batch evaluation execution.
    OfflineEval,
    /// Drift check execution.
    DriftCheck,
    /// Import execution.
    Import,
    /// Workflow execution.
    Workflow,
    /// Autonomous remediation execution.
    Remediation,
    /// Externally defined execution kind.
    External(String),
}

/// Reference to a Run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct RunRef {
    /// Run UID. Promoted to `RunUid` in the Runs phase.
    pub uid: String,
    /// Run kind.
    pub kind: RunKind,
    /// Optional space.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub space: Option<SpaceName>,
    /// Query/display labels.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub labels: Labels,
}
