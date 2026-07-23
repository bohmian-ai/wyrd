//! Scheduled subscriptions that fire Operator cards.

use serde::{Deserialize, Serialize};

use crate::reference::Ref;

/// Declarative schedule and optional observation source for an Operator.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct TriggerSpec {
    /// Optional human-readable purpose.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Required evaluation schedule.
    pub schedule: TriggerSchedule,
    /// Optional observation stream to evaluate on each schedule tick.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<TriggerSource>,
    /// Operator fired when the source matches, or on every tick without a source.
    pub operator_ref: Ref,
}

/// Cron schedule for Trigger evaluation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct TriggerSchedule {
    /// Cron expression.
    pub cron: String,
    /// Optional IANA timezone; absent means UTC.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tz: Option<String>,
}

/// Observation source evaluated by a Trigger.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum TriggerSource {
    /// Drift observations, optionally limited to one publisher.
    DriftObservation {
        /// Drift card to evaluate.
        drift_ref: Ref,
        /// Optional publisher restriction.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        subject_filter: Option<Ref>,
    },
    /// Eval observations, optionally limited to one publisher.
    EvalObservation {
        /// Eval card to evaluate.
        eval_ref: Ref,
        /// Optional publisher restriction.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        subject_filter: Option<Ref>,
    },
}
