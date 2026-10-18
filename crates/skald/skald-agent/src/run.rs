//! Runtime configuration and per-run output for the bounded tool loop.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use skald_spec::ProviderResponse;
use wyrd_spec::error::WyrdError;

use crate::conversation::Conversation;

/// Configuration for the bounded tool loop.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RunConfig {
    /// Maximum loop iterations before [`crate::AgentError::MaxIterations`].
    pub max_iterations: u32,
    /// Maximum concurrent tool calls per model iteration.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_concurrency_cap: Option<usize>,
    /// Maximum recent session turns to recall at run start.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_recent_limit: Option<usize>,
    /// Overall run timeout. `None` disables timeout enforcement.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout: Option<std::time::Duration>,
}

impl Default for RunConfig {
    fn default() -> Self {
        Self {
            max_iterations: 10,
            tool_concurrency_cap: Some(8),
            session_recent_limit: None,
            timeout: None,
        }
    }
}

/// Why an agent run terminated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FinishReason {
    /// The model returned an assistant response with no tool calls.
    ModelStopped,
    /// The loop exhausted [`RunConfig::max_iterations`].
    MaxIterations,
    /// The run terminated because a callback returned `CallbackOutcome::Abort(...)`. The cause is available on `AgentRun.error`.
    CallbackAborted,
    /// Provider dispatch failed.
    ProviderError,
    /// Tool handling failed.
    ToolError,
    /// The configured timeout elapsed. Added in a later stage.
    Timeout,
}

/// One error recorded on a successful run.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RunError {
    /// One-based iteration where the error occurred.
    pub iteration: u32,
    /// Stable error code.
    pub code: String,
    /// Human-readable detail.
    pub detail: String,
}

/// Output of one successful `Agent::run`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentRun {
    /// Convenience projection of the final assistant text.
    pub output: String,
    /// Final native provider response, when the run reached one.
    #[serde(skip)]
    pub final_response: Option<Arc<ProviderResponse>>,
    /// Number of loop iterations executed (1-based).
    pub iterations: u32,
    /// Why the loop terminated.
    pub finish_reason: FinishReason,
    /// Full in-run conversation accumulator.
    pub conversation: Conversation,
    /// Structured terminal error for abort-shaped completions.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<WyrdError>,
    /// Errors recorded without aborting the run.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub errors: Vec<RunError>,
    /// Parsed JSON object for prompts that declared a structured output schema.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub structured_output: Option<serde_json::Map<String, serde_json::Value>>,
}
