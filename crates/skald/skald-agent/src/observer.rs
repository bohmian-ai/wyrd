//! Pluggable observability hook for the agent loop.
//!
//! Skald defines this trait; consumers implement it. Skald itself never
//! depends on higher-level observability clients.

use serde_json::value::RawValue;
use skald_spec::FinishReason;

/// Receives lifecycle events from the bounded tool loop.
///
/// Exactly one of `on_agent_finish` / `on_agent_error` fires per
/// `Agent::run` / `Agent::run_prompt` invocation, after `on_agent_start`.
/// Consumers can rely on this to close their run record exactly once.
pub trait Observer: Send + Sync {
    /// Called once per `Agent::run` / `Agent::run_prompt`, before the
    /// first iteration.
    fn on_agent_start(&self, _agent_id: &str, _iteration_cap: u32) {}
    /// Called at the top of each iteration (1-based).
    fn on_iteration(&self, _agent_id: &str, _iteration: u32) {}
    /// Called immediately before dispatching one tool call.
    fn on_tool_call(&self, _agent_id: &str, _tool: &str, _args: &RawValue) {}
    /// Called immediately after a tool call returns or fails.
    fn on_tool_result(&self, _agent_id: &str, _tool: &str, _ok: bool) {}
    /// Called once when the loop terminates normally (no more tool calls).
    fn on_agent_finish(&self, _agent_id: &str, _finish: FinishReason, _iterations: u32) {}
    /// Called once when the loop terminates via an error before returning the
    /// error to the caller. `code` is the stable `SKALD_AGENT_*` identifier.
    fn on_agent_error(&self, _agent_id: &str, _code: &'static str, _detail: &str) {}
}

/// Default no-op observer; selected when no consumer wires one in.
#[derive(Debug, Default, Clone, Copy)]
pub struct NoopObserver;

impl Observer for NoopObserver {}
