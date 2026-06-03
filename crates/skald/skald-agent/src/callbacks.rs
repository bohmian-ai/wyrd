//! Callback type aliases and context snapshots for agent runs.

use std::sync::Arc;

use skald_spec::{ProviderRequest, ProviderResponse};
use skald_tool::AgentTool;

use crate::conversation::Conversation;

/// Outcome a callback can return to control its associated operation.
pub enum CallbackOutcome<T> {
    /// Let the original operation run.
    Continue,
    /// Skip the original operation.
    Skip,
    /// Replace the value the original operation would have returned.
    ReplaceWith(T),
}

/// Snapshot passed to every agent callback.
#[derive(Clone)]
pub struct AgentContext {
    /// Stable agent id.
    pub agent_id: String,
    /// Optional session id for the current run.
    pub session_id: Option<String>,
    /// Current loop iteration.
    pub iteration: u32,
    /// Read-only conversation snapshot for the callback fire point.
    pub conversation: Arc<Conversation>,
}

/// Callback fired before the agent run begins.
pub type BeforeAgentFn = Arc<dyn Fn(&AgentContext, &str) -> CallbackOutcome<String> + Send + Sync>;

/// Callback fired after the agent run completes.
pub type AfterAgentFn = Arc<
    dyn Fn(&AgentContext, &crate::run::AgentRun) -> CallbackOutcome<crate::run::AgentRun>
        + Send
        + Sync,
>;

/// Callback fired before one provider request is sent.
pub type BeforeModelFn =
    Arc<dyn Fn(&AgentContext, &ProviderRequest) -> CallbackOutcome<ProviderRequest> + Send + Sync>;

/// Callback fired after one provider response is received.
pub type AfterModelFn = Arc<
    dyn Fn(&AgentContext, &ProviderResponse) -> CallbackOutcome<ProviderResponse> + Send + Sync,
>;

/// Callback fired before one tool invocation.
pub type BeforeToolFn = Arc<
    dyn Fn(&AgentContext, &dyn AgentTool, &serde_json::Value) -> CallbackOutcome<serde_json::Value>
        + Send
        + Sync,
>;

/// Callback fired after one tool invocation.
pub type AfterToolFn = Arc<
    dyn Fn(
            &AgentContext,
            &dyn AgentTool,
            &Result<serde_json::Value, skald_tool::ToolError>,
        ) -> CallbackOutcome<Result<serde_json::Value, skald_tool::ToolError>>
        + Send
        + Sync,
>;
