//! Status: Locked (2026-05-18)
//! source: execution/PLAN_DELTA_LEDGER.md#plan-delta-aah-1
//! source: reference/spec/specs.rs (TriggerSpec / TriggerSource - canonical wire shape)

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::card::common::NonSecretValue;
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
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub config: BTreeMap<String, NonSecretValue>,
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
