//! Workflow Card spec.

use std::collections::BTreeMap;
use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::card::common::{Governance, NonSecretValue, ObservationHooks, ParameterValue};
use crate::reference::CardRef;

/// Declarative workflow definition.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct WorkflowSpec {
    /// Workflow description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Typed workflow inputs.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub inputs: BTreeMap<String, ParameterValue>,
    /// Workflow steps.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub steps: Vec<WorkflowStep>,
    /// Output descriptors.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub outputs: BTreeMap<String, serde_json::Value>,
    /// Governance and audit controls.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub governance: Option<Governance>,
    /// Observation hooks.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub observation_hooks: Option<ObservationHooks>,
    /// Free-form details.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub details: BTreeMap<String, NonSecretValue>,
}

impl WorkflowSpec {
    /// Validate duplicate step IDs, missing dependencies, and cycles.
    ///
    /// # Errors
    /// Returns a validation error when the workflow is not a DAG.
    pub fn validate_dag(&self) -> Result<(), WorkflowValidationError> {
        let mut ids = BTreeSet::new();
        for step in &self.steps {
            if !ids.insert(step.id.clone()) {
                return Err(WorkflowValidationError::DuplicateStep);
            }
        }
        for step in &self.steps {
            for dependency in &step.depends_on {
                if !ids.contains(dependency) {
                    return Err(WorkflowValidationError::MissingDependency);
                }
            }
        }
        for step in &self.steps {
            let mut visiting = BTreeSet::new();
            if self.has_cycle(&step.id, &mut visiting, &mut BTreeSet::new()) {
                return Err(WorkflowValidationError::Cycle);
            }
        }
        Ok(())
    }

    fn has_cycle(
        &self,
        id: &str,
        visiting: &mut BTreeSet<String>,
        visited: &mut BTreeSet<String>,
    ) -> bool {
        if visited.contains(id) {
            return false;
        }
        if !visiting.insert(id.to_string()) {
            return true;
        }
        let Some(step) = self.steps.iter().find(|step| step.id == id) else {
            return false;
        };
        for dependency in &step.depends_on {
            if self.has_cycle(dependency, visiting, visited) {
                return true;
            }
        }
        visiting.remove(id);
        visited.insert(id.to_string());
        false
    }
}

/// One workflow step.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct WorkflowStep {
    /// Stable step ID.
    pub id: String,
    /// Step action kind.
    pub action: WorkflowAction,
    /// Dependency step IDs.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub depends_on: Vec<String>,
    /// Step inputs.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub inputs: BTreeMap<String, ParameterValue>,
    /// Optional condition expression.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub condition: Option<String>,
    /// Timeout in seconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_seconds: Option<u64>,
    /// Retry policy.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retry: Option<WorkflowRetryPolicy>,
    /// Display metadata.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub display: BTreeMap<String, NonSecretValue>,
}

/// Workflow retry policy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct WorkflowRetryPolicy {
    /// Maximum retry attempts.
    pub max_retries: u32,
    /// Initial backoff in milliseconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub initial_backoff_ms: Option<u64>,
}

/// Workflow action target.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(tag = "type", content = "target", rename_all = "snake_case")]
pub enum WorkflowAction {
    /// Skill action.
    Skill(CardRef),
    /// Agent action.
    Agent(CardRef),
    /// MCP server action.
    Mcp(CardRef),
    /// Tool action.
    Tool(CardRef),
    /// Prompt action.
    Prompt(CardRef),
}

/// Workflow validation failures.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum WorkflowValidationError {
    /// Duplicate step ID.
    #[error("workflow contains a duplicate step id")]
    DuplicateStep,
    /// Missing dependency.
    #[error("workflow references a missing dependency")]
    MissingDependency,
    /// Workflow graph contains a cycle.
    #[error("workflow contains a dependency cycle")]
    Cycle,
}
