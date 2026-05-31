//! Sanctioned `skald-agent` re-exports.
//!
//! Importing from `wyrd::agent` is the supported way to reach the agent runtime
//! surface in Rust. Direct `skald_agent::*` imports work inside the workspace
//! but are not part of the stable public API.
//!
//! # Public Surface
//!
//! - `Agent` is the live agent, built via `Agent::from_def`. It runs the
//!   bounded tool loop through `Agent::run` or `Agent::run_prompt`.
//! - `AgentDef` is the declarative form.
//! - `AgentBuilder` is the programmatic Rust construction path.
//! - `AgentRun` is the output of one successful run.
//! - `AgentError` carries stable `SKALD_AGENT_*` codes.
//! - `AgentTool`, `AgentToolError`, and `ToolRegistry` expose executable
//!   tools.
//! - `Observer` and `NoopObserver` provide the observability hook.
//! - `RunConfig` configures bounded execution.
//! - `FinishReason` reports why execution stopped.
//!
//! Internal modules such as loop runtime, message extraction, request building,
//! and registry helpers are intentionally not re-exported here.

pub use skald_agent::{
    Agent, AgentBuilder, AgentDef, AgentError, AgentResult, AgentRun, AgentTool, AgentToolError,
    FinishReason, NoopObserver, Observer, RunConfig, ToolRegistry,
};
