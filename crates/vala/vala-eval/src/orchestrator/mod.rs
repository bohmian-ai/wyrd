//! Server-hosted eval orchestrator gated by `feature = "orchestrator"`.
//!
//! This module owns the turn state machine, server-simulated user support, the
//! Skald-backed judge adapter, and scoring hand-off.
//! Wire shapes are imported from `wyrd_spec::vala::eval::protocol`; this module
//! does not define alternate protocol types or an eval-specific run id.

pub mod error;
pub mod judge;
pub mod scoring;
pub mod simulator;
pub mod state;

pub use error::OrchestratorError;
pub use judge::{PromptCardResolver, SkaldJudgeInvoker};
pub use scoring::ScenarioScoring;
pub use simulator::{SIMULATOR_PROMPT_TEMPLATE, ServerSimulatedUser, SimulatorPrompt};
pub use state::{NextDirective, RunState, RunStatus, ScenarioCursor, SharedRun, SubmissionOutcome};
