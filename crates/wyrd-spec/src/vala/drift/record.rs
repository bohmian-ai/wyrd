//! `DriftRecordObservation` — the raw per-feature measurement a subject emits via
//! `run.observe.drift(features)`. The client emits native values; the server owns
//! the fitted baseline and all binning/sampling/scoring.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::ids::FeatureName;
use crate::vala::ids::{RecordId, SessionId};

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
///
/// The record names neither the invocation nor a Verifier. The emitting run
/// and the observed subject Card travel beside it as Bifrost row correlation,
/// which is what the server authorizes and stamps; the server then selects
/// every matching active `verified_by` binding from that authorized subject
/// identity rather than from anything the client wrote into the record.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DriftRecordObservation {
    /// Client-generated UUIDv7 record identity; server deduplicates on it.
    pub record_id: RecordId,
    /// Optional session identifier supplied explicitly at emit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<SessionId>,
    /// Native per-feature values for one inference event.
    pub features: BTreeMap<FeatureName, FeatureValue>,
    /// Wall-clock emission time.
    pub created_at: DateTime<Utc>,
}
