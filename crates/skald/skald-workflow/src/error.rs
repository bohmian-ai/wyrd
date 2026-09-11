//! Workflow error catalog.

use skald_agent::AgentError;
use skald_spec::ProviderName;
use thiserror::Error;
use wyrd_spec::error::WyrdError;

/// Result alias used throughout the crate.
pub type WorkflowResult<T> = Result<T, WorkflowError>;

/// Failures the workflow surface can raise.
#[derive(Debug, Error)]
pub enum WorkflowError {
    /// A task lookup referenced an unknown task id.
    #[error("task '{0}' not found")]
    TaskNotFound(String),
    /// A task referenced an agent id not present in the workflow.
    #[error("agent '{0}' not found")]
    AgentNotFound(String),
    /// A task declared a dependency on an unknown task id.
    #[error("dependency '{0}' not found")]
    DependencyNotFound(String),
    /// Duplicate task id within the workflow.
    #[error("task '{0}' already exists")]
    TaskAlreadyExists(String),
    /// A task declared itself as a dependency.
    #[error("task '{0}' depends on itself")]
    TaskDependsOnItself(String),
    /// Retries exhausted on a task.
    #[error("task '{0}' exceeded max retries")]
    MaxRetriesExceeded(String),
    /// Agent run completed without a provider response the workflow can consume.
    #[error("task '{0}' agent run produced no final provider response")]
    AgentMissingFinalResponse(String),
    /// Output validation failed for a task.
    #[error("task '{task_id}' output failed schema validation: {received}")]
    ResponseValidationFailed {
        /// Task id whose output failed.
        task_id: String,
        /// Schema name from the prompt response type, if available.
        expected_schema: String,
        /// Validator, parse, or missing-output detail.
        received: String,
    },
    /// A step prompt referenced a variable absent from input and upstream output.
    #[error(
        "step '{step_id}' references variable '{name}' not present in workflow input or upstream output"
    )]
    MissingParameter {
        /// Step id whose prompt references the variable.
        step_id: String,
        /// Variable name that failed to resolve.
        name: String,
    },
    /// No ready tasks remain but the workflow is not complete.
    #[error("workflow stalled; pending: {0:?}")]
    Stalled(Vec<String>),
    /// No message conversion exists for a source and destination provider pair.
    #[error("unsupported handoff {src:?}->{dst:?}")]
    UnsupportedHandoff {
        /// Upstream task provider.
        src: ProviderName,
        /// Downstream task provider.
        dst: ProviderName,
    },
    /// Workflow topology contains a cycle.
    #[error("workflow topology has a cycle involving '{0}'")]
    Cycle(String),
    /// Task lock acquisition failed.
    #[error("task lock acquisition failed")]
    Lock,
    /// Failure inside an agent propagated up.
    #[error(transparent)]
    Agent(#[from] AgentError),
    /// Workflow spec validation surfaced through the user-facing surface.
    #[error(transparent)]
    Spec(#[from] wyrd_spec::card::workflow::WorkflowValidationError),
    /// Generic boundary failure raised by the workflow user surface.
    #[error("{0}")]
    Other(String),
}

impl WorkflowError {
    /// Stable machine-readable code for cross-boundary mapping.
    pub fn code(&self) -> &'static str {
        match self {
            Self::TaskNotFound(_) => "SKALD_WORKFLOW_404_TASK",
            Self::AgentNotFound(_) => "SKALD_WORKFLOW_404_AGENT",
            Self::DependencyNotFound(_) => "WYRD_WORKFLOW_422_MISSING_DEPENDENCY",
            Self::TaskAlreadyExists(_) => "WYRD_WORKFLOW_422_DUPLICATE_STEP_ID",
            Self::TaskDependsOnItself(_) => "WYRD_WORKFLOW_422_MISSING_DEPENDENCY",
            Self::MaxRetriesExceeded(_) => "SKALD_WORKFLOW_500_MAX_RETRIES",
            Self::AgentMissingFinalResponse(_) => "SKALD_WORKFLOW_500_AGENT_RESPONSE_MISSING",
            Self::ResponseValidationFailed { .. } => "SKALD_WORKFLOW_422_OUTPUT_SCHEMA",
            Self::MissingParameter { .. } => "WYRD_WORKFLOW_422_MISSING_PARAMETER",
            Self::Stalled(_) => "SKALD_WORKFLOW_500_STALLED",
            Self::UnsupportedHandoff { .. } => "SKALD_WORKFLOW_501_UNSUPPORTED_HANDOFF",
            Self::Cycle(_) => "WYRD_WORKFLOW_422_CYCLE",
            Self::Lock => "SKALD_WORKFLOW_500_LOCK",
            Self::Agent(source) => source.code(),
            Self::Spec(error) => match error {
                wyrd_spec::card::workflow::WorkflowValidationError::DuplicateStep => {
                    "WYRD_WORKFLOW_422_DUPLICATE_STEP_ID"
                }
                wyrd_spec::card::workflow::WorkflowValidationError::MissingDependency => {
                    "WYRD_WORKFLOW_422_MISSING_DEPENDENCY"
                }
                wyrd_spec::card::workflow::WorkflowValidationError::Cycle => {
                    "WYRD_WORKFLOW_422_CYCLE"
                }
            },
            Self::Other(_) => "SKALD_WORKFLOW_500_INTERNAL",
        }
    }
}

impl From<WorkflowError> for WyrdError {
    /// Project an owned workflow failure onto the derive-backed Wyrd catalog.
    fn from(error: WorkflowError) -> Self {
        Self::from(&error)
    }
}

impl From<&WorkflowError> for WyrdError {
    /// Project a workflow failure onto the derive-backed Wyrd catalog.
    ///
    /// The catalog owns every public metadata field, so this projection only
    /// selects the variant and supplies the message plus structured details.
    /// `Agent` and `Spec` delegate to the owning projection so a nested agent
    /// failure keeps its own code rather than collapsing onto a generic
    /// workflow failure.
    fn from(error: &WorkflowError) -> Self {
        let message = error.to_string();
        match error {
            WorkflowError::TaskNotFound(task) => Self::WorkflowTaskNotFound {
                message,
                details: serde_json::json!({ "task": task }),
            },
            WorkflowError::AgentNotFound(agent) => Self::WorkflowAgentNotFound {
                message,
                details: serde_json::json!({ "agent": agent }),
            },
            WorkflowError::DependencyNotFound(task) | WorkflowError::TaskDependsOnItself(task) => {
                Self::WorkflowMissingDependency {
                    message,
                    details: serde_json::json!({ "task": task }),
                }
            }
            WorkflowError::TaskAlreadyExists(task) => Self::WorkflowDuplicateStepId {
                message,
                details: serde_json::json!({ "task": task }),
            },
            WorkflowError::MaxRetriesExceeded(task) => Self::WorkflowMaxRetries {
                message,
                details: serde_json::json!({ "task": task }),
            },
            WorkflowError::AgentMissingFinalResponse(task) => Self::WorkflowAgentResponseMissing {
                message,
                details: serde_json::json!({ "task": task }),
            },
            WorkflowError::ResponseValidationFailed {
                task_id,
                expected_schema,
                received,
            } => Self::WorkflowOutputSchema {
                message,
                details: serde_json::json!({
                    "task": task_id,
                    "expected_schema": expected_schema,
                    "received": received,
                }),
            },
            WorkflowError::MissingParameter { step_id, name } => Self::WorkflowMissingParameter {
                message,
                details: serde_json::json!({ "step": step_id, "variable": name }),
            },
            WorkflowError::Stalled(pending) => Self::WorkflowStalled {
                message,
                details: serde_json::json!({ "pending": pending }),
            },
            WorkflowError::UnsupportedHandoff { src, dst } => Self::WorkflowUnsupportedHandoff {
                message,
                details: serde_json::json!({
                    "source_provider": format!("{src:?}"),
                    "destination_provider": format!("{dst:?}"),
                }),
            },
            WorkflowError::Cycle(task) => Self::WorkflowCycle {
                message,
                details: serde_json::json!({ "task": task }),
            },
            WorkflowError::Lock => Self::WorkflowLock {
                message,
                details: serde_json::json!({}),
            },
            WorkflowError::Agent(source) => Self::from(source),
            WorkflowError::Spec(source) => Self::from(*source),
            WorkflowError::Other(detail) => Self::WorkflowInternal {
                message,
                details: serde_json::json!({ "reason": detail }),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{AgentError, WorkflowError, WyrdError};
    use skald_spec::ProviderName;

    /// Every workflow failure must reach a distinct catalog variant so the
    /// public boundaries read code, status, title, and remediation from the
    /// catalog rather than from a handwritten projection.
    #[test]
    fn workflow_errors_project_onto_the_catalog() {
        let cases: Vec<(WorkflowError, &str)> = vec![
            (
                WorkflowError::TaskNotFound("t".to_owned()),
                "WYRD_WORKFLOW_404_TASK",
            ),
            (
                WorkflowError::AgentNotFound("a".to_owned()),
                "WYRD_WORKFLOW_404_AGENT",
            ),
            (
                WorkflowError::DependencyNotFound("t".to_owned()),
                "WYRD_WORKFLOW_422_MISSING_DEPENDENCY",
            ),
            (
                WorkflowError::TaskDependsOnItself("t".to_owned()),
                "WYRD_WORKFLOW_422_MISSING_DEPENDENCY",
            ),
            (
                WorkflowError::TaskAlreadyExists("t".to_owned()),
                "WYRD_WORKFLOW_422_DUPLICATE_STEP_ID",
            ),
            (
                WorkflowError::MaxRetriesExceeded("t".to_owned()),
                "WYRD_WORKFLOW_500_MAX_RETRIES",
            ),
            (
                WorkflowError::AgentMissingFinalResponse("t".to_owned()),
                "WYRD_WORKFLOW_500_AGENT_RESPONSE_MISSING",
            ),
            (
                WorkflowError::ResponseValidationFailed {
                    task_id: "t".to_owned(),
                    expected_schema: "S".to_owned(),
                    received: "x".to_owned(),
                },
                "WYRD_WORKFLOW_422_OUTPUT_SCHEMA",
            ),
            (
                WorkflowError::MissingParameter {
                    step_id: "s".to_owned(),
                    name: "v".to_owned(),
                },
                "WYRD_WORKFLOW_422_MISSING_PARAMETER",
            ),
            (
                WorkflowError::Stalled(vec!["t".to_owned()]),
                "WYRD_WORKFLOW_500_STALLED",
            ),
            (
                WorkflowError::UnsupportedHandoff {
                    src: ProviderName::OpenAi,
                    dst: ProviderName::Anthropic,
                },
                "WYRD_WORKFLOW_501_UNSUPPORTED_HANDOFF",
            ),
            (
                WorkflowError::Cycle("t".to_owned()),
                "WYRD_WORKFLOW_422_CYCLE",
            ),
            (WorkflowError::Lock, "WYRD_WORKFLOW_500_LOCK"),
            (
                WorkflowError::Other("boom".to_owned()),
                "WYRD_WORKFLOW_500_INTERNAL",
            ),
        ];
        for (error, expected) in cases {
            assert_eq!(WyrdError::from(&error).code(), expected, "for {error:?}");
        }
    }

    /// A nested agent failure keeps its own catalog identity instead of
    /// collapsing onto a generic workflow failure.
    #[test]
    fn nested_agent_failure_keeps_its_own_code() {
        let error = WorkflowError::Agent(AgentError::Timeout {
            duration: std::time::Duration::from_millis(10),
        });
        assert_eq!(WyrdError::from(&error).code(), "WYRD_AGENT_504_TIMEOUT");
    }
}
