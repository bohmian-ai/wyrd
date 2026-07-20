//! Crate-local orchestrator errors.

use thiserror::Error;
use wyrd_spec::vala::eval::ScenarioId;

use crate::{EvalExecError, EvalPlanError};

/// Errors raised by the orchestrator state machine and embedded driver.
#[derive(Debug, Error)]
pub enum OrchestratorError {
    /// Submission addressed a different scenario than the outstanding directive.
    #[error("submission for scenario {got:?} does not match outstanding scenario {expected:?}")]
    ScenarioMismatch {
        /// Scenario id on the submission.
        got: ScenarioId,
        /// Scenario id on the outstanding directive.
        expected: ScenarioId,
    },

    /// Submission addressed a different turn than the outstanding directive.
    #[error("submission for turn {got} does not match outstanding turn {expected}")]
    TurnMismatch {
        /// Turn on the submission.
        got: u32,
        /// Turn on the outstanding directive.
        expected: u32,
    },

    /// The run is already complete.
    #[error("eval run already completed")]
    RunComplete,

    /// Submission kind did not match the outstanding directive.
    #[error("submission kind mismatch (expected {expected}, got {got})")]
    SubmissionKindMismatch {
        /// Expected submission kind.
        expected: &'static str,
        /// Actual submission kind.
        got: &'static str,
    },

    /// A scenario complete directive was acknowledged for a cursor that was not ready.
    #[error("scenario {scenario_id:?} is not ready for scoring")]
    ScenarioNotReady {
        /// Scenario id requested by the scorer.
        scenario_id: ScenarioId,
    },

    /// Mechanic results were produced without a publisher subject to aggregate under.
    #[error("publisher subject_ref is required to aggregate mechanic results")]
    MissingSubjectForMechanicResults,

    /// Internal state invariant was violated.
    #[error("orchestrator invariant violated: {reason}")]
    Invariant {
        /// Human-readable invariant.
        reason: String,
    },

    /// Scenario loading failed.
    #[error("scenario loading failed: {0}")]
    Scenarios(#[from] EvalPlanError),

    /// Engine scoring failed.
    #[error("engine scoring failed: {0}")]
    Scoring(#[from] EvalExecError),

    /// Eval planning failed before execution.
    #[error("eval planning failed: {reason}")]
    Plan {
        /// Planning failure.
        reason: String,
    },

    /// Server simulator failed after exhausting retries.
    #[error("server simulator failed after {attempts} attempts: {reason}")]
    SimulatorFailed {
        /// Attempts made before giving up.
        attempts: u32,
        /// Last underlying failure.
        reason: String,
    },

    /// Embedded callback failed.
    #[error("embedded callback failed: {reason}")]
    EmbeddedCallback {
        /// Callback failure detail.
        reason: String,
    },
}
