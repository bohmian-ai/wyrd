//! Server-hosted eval orchestrator.
//!
//! This module owns the turn state machine, server-simulated user support, and
//! scoring hand-off.
//! Wire shapes are imported from `wyrd_spec::vala::eval::protocol`; this module
//! does not define alternate protocol types or an eval-specific run id.

pub mod error;
pub mod scoring;
pub mod simulator;
pub mod state;

pub use crate::blocks::{AgentCardResolver, PromptCardResolver};
pub use error::OrchestratorError;
pub use scoring::ScenarioScoring;
pub use simulator::{SIMULATOR_PROMPT_TEMPLATE, ServerSimulatedUser, SimulatorPrompt};
pub use state::{NextDirective, RunState, RunStatus, ScenarioCursor, SharedRun, SubmissionOutcome};
