//! Sanctioned `skald-agent` re-exports.
//!
//! Importing from `wyrd::agent` is the supported way to reach the agent runtime
//! surface in Rust. Direct `skald_agent::*` imports work inside the workspace
//! but are not part of the stable public API.
//!
//! # Public Surface
//!
//! - `Agent` is the live agent, built from a resolved native prompt. It runs the
//!   bounded tool loop through `Agent::run` or `Agent::run_prompt`.
//! - `AgentRun` is the output of one successful run.
//! - `AgentError` carries stable `SKALD_AGENT_*` codes.
//! - `RunConfig` configures bounded execution.
//! - `FinishReason` reports why execution stopped.
//!
//! Internal modules such as loop runtime, message extraction, request building,
//! and registry helpers are intentionally not re-exported here.

pub use skald_agent::{Agent, AgentError, AgentResult, AgentRun, FinishReason, RunConfig};
