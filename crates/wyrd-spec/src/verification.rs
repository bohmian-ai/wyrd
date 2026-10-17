//! Wire contracts of the Verification control-plane API.
//!
//! `GET /v1/verification/bindings/{binding_id}` reports binding activity and
//! readiness; [`execution`] holds the evidence, execute, enqueue, and
//! execution-status contracts. HTTP, MCP, and the Rust, Python, and TypeScript
//! clients share these typed shapes. Durable control state — binding
//! readiness, execution status, and each Operator dispatch's delivery status —
//! is owned by Postgres; results stay in Bifrost. [`ids`], [`operator`] and
//! [`plan`] hold the step and requirement identifiers, comparison operators
//! and step-graph planning that Verifier specs and the engine share.

pub mod execution;
pub mod ids;
pub mod operator;
pub mod plan;

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::card::drift::DriftMethod;
use crate::card::trigger::TriggerDuration;
use crate::card::verifier::SourceText;
use crate::ids::{BindingId, CardUid, ExecutionId, FeatureName, OperatorDispatchId, SlotName};

pub use ids::{JsonPath, TaskId};
pub use operator::{ComparisonOperator, DivergenceMetric, JsonValueType};
pub use plan::{DagError, ExecutionPlan, Stage, plan_stages};

/// How a Verifier stores each requirement result's captured `actual` value.
///
/// Verdict correctness is unaffected by this setting; only the stored
/// snapshot of the extracted value changes. Set on the Verifier spec via
/// `context_capture`; absent means `Full`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ContextCapture {
    /// Store the raw extracted value.
    Full,
    /// Store a SHA-256 hex digest of the extracted value. Allows determinism
    /// checks without persisting PII.
    Hash,
    /// Do not store the value. Use when evidence may contain PII.
    Redact,
}

/// A frozen reference to one effective Trigger or Operator.
///
/// A referenced Card is frozen by its exact UID; an inline body, which has no
/// Card, by the canonical digest of its typed spec. Binding projection, runs,
/// and dispatches store this identity so later Card changes never retarget
/// admitted work.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum FrozenTarget {
    /// A registered Card version.
    Uid(CardUid),
    /// The canonical spec digest of an inline body.
    Digest(String),
}

/// A half-open UTC Drift comparison window `[start, end)`.
///
/// Scheduled executions derive it from the due cron occurrence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct DriftWindow {
    /// Inclusive window start.
    pub start: DateTime<Utc>,
    /// Exclusive window end.
    pub end: DateTime<Utc>,
}

/// Whether a Verifier may produce a judgment now.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    schemars::JsonSchema,
    strum::EnumString,
    strum::IntoStaticStr,
)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum VerifierReadiness {
    /// The Verifier can run.
    Ready,
    /// A PSI or SPC Drift Verifier's fitted baseline is not ready yet.
    BaselineNotReady,
    /// The Verifier Card is missing, not a Verifier, or no longer active.
    VerifierUnavailable,
}

/// Execution state of one Verifier execution.
///
/// Independent of the verdict: only `completed` has one, and it lives in the
/// result. `cancelled`, `timed_out`, and `errored` never carry a verdict or
/// dispatch an Operator.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    schemars::JsonSchema,
    strum::EnumString,
    strum::IntoStaticStr,
)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum VerificationExecutionStatus {
    /// Enqueued and waiting for its first claim.
    Pending,
    /// Claimed by a runner under an unexpired lease.
    Running,
    /// A retryable failure is waiting for its next attempt.
    Retrying,
    /// Executed and produced an acknowledged result.
    Completed,
    /// Stopped before producing a result.
    Cancelled,
    /// Exceeded its execution deadline.
    TimedOut,
    /// Failed terminally or exhausted its attempts.
    Errored,
}

/// Common Verifier verdict of a completed execution.
///
/// Carried by the result and used by settlement to decide Operator dispatch.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    schemars::JsonSchema,
    strum::EnumString,
    strum::IntoStaticStr,
)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum VerificationVerdict {
    /// Expectations held.
    Passed,
    /// Expectations were violated; a background binding execution dispatches
    /// Operators.
    Failed,
    /// No judgment could be made; never a pass.
    Inconclusive,
}

/// Delivery state of one Operator dispatch.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    schemars::JsonSchema,
    strum::EnumString,
    strum::IntoStaticStr,
)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum OperatorDispatchStatus {
    /// Created by settlement and waiting for its first claim.
    Pending,
    /// Claimed by the Operator worker under an unexpired lease.
    Running,
    /// A transient delivery failure is waiting for its next attempt.
    Retrying,
    /// The provider accepted the action.
    Delivered,
    /// Failed terminally or exhausted its attempts.
    Failed,
}

/// Structured, secret-free execution or delivery error.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct VerificationError {
    /// Stable machine-readable error code.
    pub code: String,
    /// Bounded human-readable description.
    pub message: String,
}

/// `GET /v1/verification/bindings/{binding_id}` response.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct VerificationBindingStatus {
    /// The binding.
    pub binding_id: BindingId,
    /// Exact owner Service or standalone Agent Card version.
    pub owner_card_uid: CardUid,
    /// Exact Card the binding verifies.
    pub card_uid: CardUid,
    /// Exact bound Verifier Card version.
    pub verifier_card_uid: CardUid,
    /// Whether the owner's Card-bound principal is runtime-active, so
    /// scheduled work may be admitted now.
    pub active: bool,
    /// Whether the Verifier can run, and why not when it cannot.
    pub readiness: VerifierReadiness,
    /// Next scheduled occurrence; null before activation and for a binding
    /// without a schedule.
    pub next_run_at: Option<DateTime<Utc>>,
    /// The owner principal's last qualifying machine exchange.
    pub last_activated_at: Option<DateTime<Utc>>,
    /// Latest execution admitted for this binding by queue admission order;
    /// retries do not create a new admission. Read its status, result, and
    /// dispatches through execution status.
    pub last_execution_id: Option<ExecutionId>,
    /// Pinned end timing and source bindings of a `group_end` binding; null
    /// for any other binding.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group_end: Option<GroupEndSettings>,
}

/// The settings a `group_end` binding was registered with, frozen on the
/// binding so later Trigger or Verifier registrations never change them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct GroupEndSettings {
    /// The group ends once its newest source row is older than this.
    pub idle: TriggerDuration,
    /// The group ends once its first source row is older than this.
    pub max_age: TriggerDuration,
    /// Rows timed this long after the group's first source row no longer
    /// reopen or expand it.
    pub finality: TriggerDuration,
    /// The exact source binding each source slot of the group Verifier reads.
    pub sources: BTreeMap<SlotName, GroupSource>,
}

/// One source slot's pinned binding: the recorded owner/subject/Verifier
/// tuple whose grouped executions fill the slot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct GroupSource {
    /// The authored source declaration.
    pub reference: SourceText,
    /// Component alias, or `$owner`, of the source binding on the owner.
    pub subject_occurrence_key: String,
    /// Exact Card the source binding verifies.
    pub card_uid: CardUid,
    /// Exact source Verifier Card version.
    pub verifier_card_uid: CardUid,
}

/// The source evidence one group execution pinned when it was enqueued.
///
/// Every retry of the execution judges exactly this snapshot; later source
/// rows only ever reach a later group execution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct GroupCoverage {
    /// Newest eligible source row time (input `recorded_at` or result
    /// `ended_at`) the snapshot covers.
    pub through: DateTime<Utc>,
    /// Distinct eligible source rows, inputs and results counted apart, the
    /// snapshot covers; a later-visible row timed earlier raises it.
    pub members: u64,
    /// The pinned selection of each source slot.
    pub sources: BTreeMap<SlotName, GroupSourceSnapshot>,
}

/// One source slot's pinned selection within a [`GroupCoverage`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct GroupSourceSnapshot {
    /// The authored source declaration.
    pub reference: SourceText,
    /// The source executions the slot's list holds one value for, in list
    /// order: input `recorded_at`, then execution ID.
    pub execution_ids: Vec<ExecutionId>,
    /// The deployment source-execution ceiling, present only when the
    /// eligible executions exceeded it; nothing was read and the group
    /// judgment is inconclusive.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exceeded_ceiling: Option<u32>,
}

/// Current state of one Operator dispatch in an execution status response.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct OperatorDispatchState {
    /// The dispatch.
    pub dispatch_id: OperatorDispatchId,
    /// The frozen Operator Card UID or inline-spec digest.
    pub operator: FrozenTarget,
    /// Independent delivery status.
    pub status: OperatorDispatchStatus,
    /// Last delivery error, when one occurred.
    pub error: Option<VerificationError>,
}

/// Largest direct execution request body: 1 MiB.
pub const MAX_EXECUTE_BODY_BYTES: usize = 1024 * 1024;

/// Server deadline of one direct execution, from Verifier load through
/// judgment; past it the server answers `WYRD_VERIFICATION_504_EXECUTION_TIMED_OUT`.
///
/// Clients wait at least this long for an execute response so the server's
/// timeout, not the client's, decides a slow judgment.
pub const EXECUTION_DEADLINE: std::time::Duration = std::time::Duration::from_mins(1);

/// The verdict of one Drift feature or of a whole Drift report.
///
/// Serialized in the Drift engine's own spelling (`NoDrift`, `Drift`,
/// `Inconclusive`), so a report reads the same wherever it is stored.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub enum DriftVerdict {
    /// The score did not exceed its threshold.
    NoDrift,
    /// The score exceeded its threshold.
    Drift,
    /// The target data could not be scored.
    Inconclusive,
}

/// The Drift engine's report of one drift check.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct DriftReport {
    /// The Drift method the Verifier's profile selected.
    pub method: DriftMethod,
    /// One report per scored feature, keyed by feature name.
    pub features: BTreeMap<FeatureName, FeatureDriftReport>,
    /// The aggregated verdict: `Drift` when any feature drifted.
    pub verdict: DriftVerdict,
}

/// The Drift report of one feature.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct FeatureDriftReport {
    /// The feature this report describes.
    pub feature: FeatureName,
    /// PSI score, total SPC chart signals, or absolute Custom delta; absent
    /// when the feature is inconclusive.
    #[serde(default)]
    pub score: Option<f64>,
    /// The threshold `score` was compared against; absent when the feature is
    /// inconclusive.
    #[serde(default)]
    pub threshold: Option<f64>,
    /// The feature's verdict.
    pub verdict: DriftVerdict,
    /// The method's evidence behind a scored PSI or SPC feature: frozen bins
    /// with baseline and target counts, or chart limits and signals.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "server", schema(value_type = Option<Object>))]
    pub evidence: Option<serde_json::Value>,
}

#[cfg(test)]
mod tests {
    //! Stored-name round trips of the control-plane enums.

    use super::*;

    /// Status enums round-trip through their stored snake_case names.
    ///
    /// # Panics
    /// Panics when a stored name does not parse back to its variant.
    #[test]
    fn status_names_round_trip() {
        for status in [
            VerificationExecutionStatus::Pending,
            VerificationExecutionStatus::TimedOut,
            VerificationExecutionStatus::Errored,
        ] {
            let name: &'static str = status.into();
            assert_eq!(
                name.parse::<VerificationExecutionStatus>().ok(),
                Some(status)
            );
        }
        let readiness: &'static str = VerifierReadiness::BaselineNotReady.into();
        assert_eq!(readiness, "baseline_not_ready");
    }
}
