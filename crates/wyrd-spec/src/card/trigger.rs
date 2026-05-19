//! Trigger Card spec.

use serde::{Deserialize, Serialize};

use crate::reference::CardRef;

/// Declarative trigger that invokes an Operator when its source condition fires.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct TriggerSpec {
    /// Trigger source.
    pub source: TriggerSource,
    /// Target Operator Card reference.
    pub target: CardRef,
    /// Minimum cooldown between firings, in seconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cooldown_seconds: Option<u32>,
    /// Non-secret trigger configuration.
    #[serde(default, skip_serializing_if = "serde_json::Value::is_null")]
    pub config: serde_json::Value,
}

/// Trigger source declaration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[non_exhaustive]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TriggerSource {
    /// Drift observation emitted by a Drift Card.
    DriftObservation {
        /// Source Drift Card reference.
        card: CardRef,
    },
    /// Evaluation observation emitted by an Eval Card.
    EvalObservation {
        /// Source Eval Card reference.
        card: CardRef,
    },
    /// Cron schedule.
    Schedule {
        /// Cron expression.
        cron: String,
    },
}
