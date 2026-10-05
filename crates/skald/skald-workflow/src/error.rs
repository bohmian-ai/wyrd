//! Workflow error catalog.
//!
//! [`WorkflowError`] covers failures that prevent a Workflow from being built
//! or from starting: contract validation, resolved-graph validation, input
//! shape and size, and route/binding suitability. Once execution starts, step
//! and run outcomes are returned inside the portable
//! [`wyrd_spec::card::workflow::WorkflowRun`] instead of as errors.

use thiserror::Error;
use wyrd_spec::card::workflow::WorkflowValidationError;
use wyrd_spec::error::WyrdError;

/// Result alias used throughout the crate.
pub type WorkflowResult<T> = Result<T, WorkflowError>;

/// Failures the workflow surface can raise before execution starts.
#[derive(Debug, Error)]
pub enum WorkflowError {
    /// A step has no resolved Agent, for example a referenced Agent Card that
    /// was never hydrated through an [`crate::AgentResolver`].
    #[error("step '{0}' has no resolved agent")]
    AgentNotFound(String),
    /// Pure Workflow contract validation failed.
    #[error(transparent)]
    Spec(#[from] WorkflowValidationError),
    /// A derive-backed catalog failure: resolved-graph validation, invocation
    /// input, size bounds, or route and binding suitability.
    #[error(transparent)]
    Wyrd(#[from] WyrdError),
}

impl WorkflowError {
    /// Stable machine-readable code for cross-boundary mapping.
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::AgentNotFound(_) => "WYRD_WORKFLOW_404_AGENT",
            Self::Spec(error) => WyrdError::from(error.clone()).code(),
            Self::Wyrd(error) => error.code(),
        }
    }
}

impl From<WorkflowError> for WyrdError {
    /// Project an owned workflow failure onto the derive-backed Wyrd catalog.
    fn from(error: WorkflowError) -> Self {
        match error {
            WorkflowError::AgentNotFound(step) => Self::WorkflowAgentNotFound {
                message: format!("step '{step}' has no resolved agent"),
                details: serde_json::json!({ "step": step }),
            },
            WorkflowError::Spec(source) => Self::from(source),
            WorkflowError::Wyrd(source) => source,
        }
    }
}

impl From<&WorkflowError> for WyrdError {
    /// Project a borrowed workflow failure onto the derive-backed Wyrd catalog.
    fn from(error: &WorkflowError) -> Self {
        match error {
            WorkflowError::AgentNotFound(step) => Self::WorkflowAgentNotFound {
                message: error.to_string(),
                details: serde_json::json!({ "step": step }),
            },
            WorkflowError::Spec(source) => Self::from(source.clone()),
            WorkflowError::Wyrd(source) => source.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{WorkflowError, WyrdError};
    use wyrd_spec::card::workflow::WorkflowValidationError;

    /// Every workflow failure reaches its catalog variant, so the public
    /// boundaries read code, status, title, and remediation from the catalog.
    #[test]
    fn workflow_errors_project_onto_the_catalog() {
        let cases: Vec<(WorkflowError, &str)> = vec![
            (
                WorkflowError::AgentNotFound("a".to_owned()),
                "WYRD_WORKFLOW_404_AGENT",
            ),
            (
                WorkflowError::Spec(WorkflowValidationError::Cycle {
                    field: "steps".to_owned(),
                }),
                "WYRD_WORKFLOW_422_CYCLE",
            ),
            (
                WorkflowError::Wyrd(WyrdError::WorkflowRunRequest {
                    message: "bad".to_owned(),
                    details: serde_json::json!({}),
                }),
                "WYRD_WORKFLOW_422_RUN_REQUEST",
            ),
        ];
        for (error, expected) in cases {
            assert_eq!(error.code(), expected, "for {error:?}");
            assert_eq!(WyrdError::from(&error).code(), expected, "for {error:?}");
        }
    }
}
