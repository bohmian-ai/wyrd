//! Workflow error catalog.

use skald_agent::AgentError;
use skald_spec::ProviderName;
use thiserror::Error;

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
}

impl WorkflowError {
    /// Stable machine-readable code for cross-boundary mapping.
    pub fn code(&self) -> &'static str {
        match self {
            Self::TaskNotFound(_) => "SKALD_WORKFLOW_404_TASK",
            Self::AgentNotFound(_) => "SKALD_WORKFLOW_404_AGENT",
            Self::DependencyNotFound(_) => "SKALD_WORKFLOW_422_DEP_MISSING",
            Self::TaskAlreadyExists(_) => "SKALD_WORKFLOW_409_TASK_EXISTS",
            Self::TaskDependsOnItself(_) => "SKALD_WORKFLOW_422_SELF_DEP",
            Self::MaxRetriesExceeded(_) => "SKALD_WORKFLOW_500_MAX_RETRIES",
            Self::ResponseValidationFailed { .. } => "SKALD_WORKFLOW_422_OUTPUT_SCHEMA",
            Self::Stalled(_) => "SKALD_WORKFLOW_500_STALLED",
            Self::UnsupportedHandoff { .. } => "SKALD_WORKFLOW_501_UNSUPPORTED_HANDOFF",
            Self::Cycle(_) => "SKALD_WORKFLOW_422_CYCLE",
            Self::Lock => "SKALD_WORKFLOW_500_LOCK",
            Self::Agent(source) => source.code(),
        }
    }
}
