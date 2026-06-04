//! Agent run journal trait and no-op default.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// Event emitted by the agent run journal.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum JournalEvent {
    /// Agent run started.
    AgentStart {
        /// Stable agent id.
        agent_id: String,
        /// Initial input.
        input: String,
        /// Optional session id for the run.
        session_id: Option<String>,
    },
    /// Loop iteration started.
    Iteration {
        /// Zero-based iteration index.
        index: u32,
    },
    /// Provider model call started.
    ModelCall {
        /// Iteration for this model call.
        iteration: u32,
        /// Provider name.
        provider: String,
        /// Model name.
        model: String,
    },
    /// Provider model call completed.
    ModelResult {
        /// Iteration for this model result.
        iteration: u32,
        /// Provider finish reason.
        finish_reason: String,
        /// Whether the result was synthetic.
        synthetic: bool,
    },
    /// Tool invocation started.
    ToolCall {
        /// Iteration for this tool call.
        iteration: u32,
        /// Provider tool call id.
        call_id: String,
        /// Tool name.
        tool_name: String,
        /// Tool arguments.
        args: serde_json::Value,
    },
    /// Tool invocation completed.
    ToolResult {
        /// Iteration for this tool result.
        iteration: u32,
        /// Provider tool call id.
        call_id: String,
        /// Whether the tool call succeeded.
        ok: bool,
        /// Tool output.
        output: serde_json::Value,
    },
    /// Agent run finished.
    AgentFinish {
        /// Stable agent id.
        agent_id: String,
        /// Agent finish reason.
        finish_reason: String,
        /// Number of iterations executed.
        iterations: u32,
    },
    /// Agent run failed.
    AgentError {
        /// Stable agent id.
        agent_id: String,
        /// Stable error code.
        code: String,
        /// Human-readable error message.
        message: String,
    },
}

/// Journal backend contract for agent run events.
#[async_trait]
pub trait Journal: Send + Sync {
    /// Appends one journal event.
    async fn append(&self, event: JournalEvent) -> Result<(), JournalError>;
}

/// Default journal backend that drops all events.
pub struct NoopJournal;

#[async_trait]
impl Journal for NoopJournal {
    async fn append(&self, _event: JournalEvent) -> Result<(), JournalError> {
        Ok(())
    }
}

/// Journal backend failures.
#[derive(Debug, thiserror::Error)]
pub enum JournalError {
    /// Event append failed.
    #[error("journal append failed: {0}")]
    AppendFailed(String),
}
