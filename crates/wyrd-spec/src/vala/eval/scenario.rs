//! Offline-only scenario shape.
//!
//! Scenarios are versionable test data. How they are stored, produced, and
//! resolved server-side is **not yet defined**: the eval domain model — including
//! whether scenarios are carried by a Data card, an Eval-owned object, or a new
//! kind — is still open. This module defines only the offline
//! `EvalScenarioCollection` shape and does not commit to a storage or dispatch
//! mechanism.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::error::WyrdError;

use super::condition::EvalCondition;
use super::ids::{ScenarioId, TaskId};
use super::operator::ComparisonOperator;

/// Maximum allowed scenarios per collection.
///
/// This is a soft cap, raised by a later locked decision if a real corpus
/// needs it.
pub const MAX_SCENARIOS_PER_COLLECTION: usize = 10_000;

/// Hard cap on `max_turns` per scenario.
///
/// This is a runtime safeguard against runaway loops; agents that legitimately
/// need more should split scenarios.
pub const MAX_TURNS_HARD_CAP: u32 = 256;

/// One offline test case.
///
/// The runtime drives the agent with `initial_query` and any
/// `predefined_turns`, then evaluates the produced response against the
/// scenario-level `tasks`.
///
/// `tasks` are the "passenger view" — assertions against
/// `{response, expected_outcome}`, in contrast to `EvalSpec.tasks` (the
/// "mechanic view" — assertions against the workflow context). Both views may
/// be present in one offline run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EvalScenario {
    /// Unique identifier within the enclosing collection.
    ///
    /// Validated by `ScenarioId::new` using the same charset and length rules
    /// as `TaskId`.
    pub id: ScenarioId,
    /// First user message supplied to the runtime for the scenario.
    pub initial_query: String,
    /// Optional expected final outcome for scenario-level comparisons.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_outcome: Option<String>,
    /// Additional user turns.
    ///
    /// Runtime drives them after `initial_query`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub predefined_turns: Vec<String>,
    /// Free-text persona description for synthetic user simulation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub simulated_user_persona: Option<String>,
    /// Optional substring; if the agent emits it, the scenario terminates.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub termination_signal: Option<String>,
    /// Maximum number of turns the scenario may run.
    #[serde(default = "default_max_turns")]
    pub max_turns: u32,
    /// Scenario-level tasks evaluated against `{response, expected_outcome}`.
    #[serde(default)]
    pub tasks: Vec<ScenarioTask>,
}

fn default_max_turns() -> u32 {
    8
}

impl EvalScenario {
    /// Validate per-scenario invariants.
    ///
    /// Checks non-empty `initial_query`, `max_turns` bounds, unique
    /// scenario-task ids, and per-task condition validity.
    ///
    /// # Errors
    /// Returns [`WyrdError::Validation`] when any scenario invariant fails.
    pub fn validate(&self) -> Result<(), WyrdError> {
        if self.initial_query.is_empty() {
            return Err(WyrdError::Validation {
                message: "eval_scenario.initial_query must be non-empty".to_string(),
                details: serde_json::Value::Null,
            });
        }
        if self.max_turns == 0 || self.max_turns > MAX_TURNS_HARD_CAP {
            return Err(WyrdError::Validation {
                message: format!(
                    "eval_scenario.max_turns must be 1..={MAX_TURNS_HARD_CAP}, got {}",
                    self.max_turns,
                ),
                details: serde_json::Value::Null,
            });
        }

        let mut seen: BTreeSet<&str> = BTreeSet::new();
        for task in &self.tasks {
            if !seen.insert(task.id.as_str()) {
                return Err(WyrdError::Validation {
                    message: format!(
                        "eval_scenario.tasks contains duplicate id {:?}",
                        task.id.as_str(),
                    ),
                    details: serde_json::Value::Null,
                });
            }
            if let Some(condition) = &task.condition {
                condition.validate()?;
            }
        }
        Ok(())
    }
}

/// A scenario-level task.
///
/// This is narrower than `EvalTask`: scenario evaluation operates on the
/// `{response, expected_outcome}` pair, so the structure is intentionally
/// simple. Scenario tasks have no `depends_on` field and no DAG because they
/// are always evaluated together against the same final pair.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ScenarioTask {
    /// Unique identifier within the enclosing scenario.
    pub id: TaskId,
    /// Comparison applied to the scenario response pair.
    pub operator: ComparisonOperator,
    /// Right-hand side of the comparison.
    pub expected: serde_json::Value,
    /// Optional gate evaluated before the scenario task runs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub condition: Option<EvalCondition>,
}

/// Top-level payload shape for a `DataCard` whose `content_kind` is
/// `"EvalScenarioCollection"`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EvalScenarioCollection {
    /// Stable identifier for the collection, such as `"customer-support-v2"`.
    pub collection_id: String,
    /// Offline scenarios in this collection.
    pub scenarios: Vec<EvalScenario>,
}

impl EvalScenarioCollection {
    /// Validate the collection.
    ///
    /// Checks non-empty `collection_id`, the scenario-count cap, unique
    /// scenario ids, and per-scenario validity.
    ///
    /// # Errors
    /// Returns [`WyrdError::Validation`] when any collection invariant fails.
    pub fn validate(&self) -> Result<(), WyrdError> {
        if self.collection_id.is_empty() {
            return Err(WyrdError::Validation {
                message: "eval_scenario_collection.collection_id must be non-empty".to_string(),
                details: serde_json::Value::Null,
            });
        }
        if self.scenarios.len() > MAX_SCENARIOS_PER_COLLECTION {
            return Err(WyrdError::Validation {
                message: format!(
                    "eval_scenario_collection.scenarios.len() {} exceeds cap {}",
                    self.scenarios.len(),
                    MAX_SCENARIOS_PER_COLLECTION,
                ),
                details: serde_json::Value::Null,
            });
        }

        let mut seen: BTreeSet<&str> = BTreeSet::new();
        for scenario in &self.scenarios {
            if !seen.insert(scenario.id.as_str()) {
                return Err(WyrdError::Validation {
                    message: format!(
                        "eval_scenario_collection has duplicate scenario id {:?}",
                        scenario.id.as_str(),
                    ),
                    details: serde_json::Value::Null,
                });
            }
            scenario.validate()?;
        }
        Ok(())
    }
}
