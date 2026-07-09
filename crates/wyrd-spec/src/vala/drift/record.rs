//! `DriftRecordObservation` — the raw per-feature measurement a subject emits via
//! `run.observe.drift(features)`. The client emits native values; the server owns
//! the fitted baseline and all binning/sampling/scoring.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::ids::FeatureName;
use crate::reference::CardRef;
use crate::vala::ids::{RecordId, RunId, SessionId};

/// One measured feature value. Untagged so the wire scalar's type is the tag —
/// `82000.0 → Float`, `5 → Int`, `"premium" → Cat`, `true → Bool`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(untagged)]
pub enum FeatureValue {
    /// Boolean feature (`true` / `false`).
    Bool(bool),
    /// Integer feature (64-bit signed).
    Int(i64),
    /// Floating-point feature (64-bit).
    Float(f64),
    /// Categorical feature (string label).
    Cat(String),
}

/// The raw drift measurement a subject emits for one event.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct DriftRecordObservation {
    /// Client-generated UUIDv7 record identity; server deduplicates on it.
    pub record_id: RecordId,
    /// Run identifier of the invocation that emitted the record.
    pub run_id: RunId,
    /// Optional session identifier supplied explicitly at emit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<SessionId>,
    /// Pin one Drift card. `None` → fan to every Drift card whose `subject_ref`
    /// is the run's Target.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub drift_ref: Option<CardRef>,
    /// Native per-feature values for one inference event.
    pub features: BTreeMap<FeatureName, FeatureValue>,
    /// Wall-clock emission time.
    pub created_at: DateTime<Utc>,
}
