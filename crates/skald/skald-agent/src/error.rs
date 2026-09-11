//! Agent error catalog.

use std::time::Duration;

use crate::journal::JournalError;
use crate::session::SessionError;
use skald_runtime::SkaldRuntimeError;
use skald_spec::ProviderName;
use skald_tool::SkaldToolError;
use thiserror::Error;
use wyrd_spec::error::WyrdError;

/// Result alias used throughout the crate.
pub type AgentResult<T> = Result<T, AgentError>;

/// Failures the agent surface can raise.
#[derive(Debug, Error)]
pub enum AgentError {
    /// Tool lookup failed in one of two post-S12B contexts:
    /// build-time resolution while `Agent::from_wire` / `Agent::from_card`
    /// hydrate runtime-local tool names, or loop runtime call-time dispatch
    /// after a provider emits a tool call.
    #[error("tool '{name}' is not registered for this agent")]
    ToolNotFound {
        /// Tool name from the provider response.
        name: String,
    },
    /// The model requested a tool that is not attached to this agent.
    #[error("tool '{name}' is not attached to this agent")]
    ToolNotInAgent {
        /// Tool name from the provider response.
        name: String,
    },
    /// Tool argument validation rejected a provider-emitted call payload.
    #[error("tool '{tool}' arguments invalid: {detail}")]
    InvalidToolArgs {
        /// Tool name from the call.
        tool: String,
        /// Reason the args failed validation.
        detail: String,
    },
    /// Bounded loop hit the iteration cap.
    #[error("agent '{agent}' exceeded max iterations ({cap})")]
    MaxIterations {
        /// Agent id.
        agent: String,
        /// Configured ceiling.
        cap: u32,
    },
    /// A `skald-runtime` provider call failed.
    #[error("provider call failed: {0}")]
    Provider(#[from] SkaldRuntimeError),
    /// Tool declaration was malformed.
    #[error(transparent)]
    Tool(#[from] SkaldToolError),
    /// `Agent::run_prompt` was given a prompt that could not be rendered.
    #[error("agent '{agent}' prompt rendering failed: {detail}")]
    Prompt {
        /// Agent id.
        agent: String,
        /// Underlying render or projection failure message.
        detail: String,
    },
    /// A message in the loop history does not match the agent's bound provider.
    #[error("loop history contains a non-{provider:?} message: {detail}")]
    LoopMessageType {
        /// Provider the agent is bound to.
        provider: ProviderName,
        /// Description of the mismatch.
        detail: String,
    },
    /// `Agent::run_prompt` was given a prompt whose rendered provider does not
    /// match the agent's bound provider.
    #[error(
        "agent '{agent}' bound to {agent_provider:?} cannot run a prompt for {prompt_provider:?}"
    )]
    ProviderMismatch {
        /// Agent id.
        agent: String,
        /// Provider the agent is bound to.
        agent_provider: ProviderName,
        /// Provider the rendered prompt request targets.
        prompt_provider: ProviderName,
    },
    /// Agent-as-tool delegation exceeded the configured nesting depth.
    #[error("agent delegation depth exceeded for chain: {chain:?}")]
    DelegationDepthExceeded {
        /// Delegation chain at the time the cap was exceeded.
        chain: Vec<String>,
    },
    /// User-supplied callback panicked.
    #[error("callback '{hook}' panicked: {payload}")]
    CallbackPanic {
        /// Callback hook name.
        hook: String,
        /// Panic payload rendered for diagnostics.
        payload: String,
    },
    /// Session memory failed to fetch recent turns.
    #[error("session '{session_id}' recent fetch failed: {source}")]
    SessionRecentFailed {
        /// Session id requested by the agent run.
        session_id: String,
        /// Backend failure.
        source: SessionError,
    },
    /// Session memory failed to append a turn.
    #[error("session '{session_id}' append failed: {source}")]
    SessionAppendFailed {
        /// Session id requested by the agent run.
        session_id: String,
        /// Backend failure.
        source: SessionError,
    },
    /// Journal backend failed to append an event.
    #[error("journal append failed: {source}")]
    JournalAppendFailed {
        /// Backend failure.
        source: JournalError,
    },
    /// Agent run exceeded the configured timeout.
    #[error("agent run exceeded configured timeout ({duration:?})")]
    Timeout {
        /// Configured timeout duration.
        duration: Duration,
    },
    /// Provider returned non-object JSON for a structured-output prompt.
    #[error("agent '{agent}' returned a non-JSON structured response: {detail}")]
    StructuredOutputDecode {
        /// Agent id.
        agent: String,
        /// Parser detail.
        detail: String,
    },
    /// A constructor or method argument was invalid.
    #[error("invalid argument '{name}': {detail}")]
    InvalidArgument {
        /// Argument name.
        name: String,
        /// Reason the argument was rejected.
        detail: String,
    },
}

impl AgentError {
    /// Convenience constructor for the bounded-loop terminal failure.
    pub fn max_iterations(agent: impl Into<String>, cap: u32) -> Self {
        Self::MaxIterations {
            agent: agent.into(),
            cap,
        }
    }

    /// Stable machine-readable code for cross-boundary mapping.
    pub fn code(&self) -> &'static str {
        match self {
            Self::ToolNotFound { .. } => "SKALD_AGENT_404_TOOL",
            Self::ToolNotInAgent { .. } => "SKALD_AGENT_404_TOOL_NOT_IN_AGENT",
            Self::InvalidToolArgs { .. } => "SKALD_AGENT_422_TOOL_ARGS",
            Self::Prompt { .. } => "SKALD_AGENT_422_PROMPT",
            Self::LoopMessageType { .. } => "SKALD_AGENT_422_LOOP_MESSAGE_TYPE",
            Self::ProviderMismatch { .. } => "SKALD_AGENT_409_PROVIDER_MISMATCH",
            Self::MaxIterations { .. } => "SKALD_AGENT_500_MAX_ITERATIONS",
            Self::Provider(_) => "SKALD_AGENT_502_PROVIDER",
            Self::Tool(source) => source.code(),
            Self::DelegationDepthExceeded { .. } => "SKALD_AGENT_412_DELEGATION_DEPTH",
            Self::CallbackPanic { .. } => "SKALD_AGENT_500_CALLBACK_PANIC",
            Self::SessionRecentFailed { .. } => "SKALD_SESSION_500_RECENT",
            Self::SessionAppendFailed { .. } => "SKALD_SESSION_500_APPEND",
            Self::JournalAppendFailed { .. } => "SKALD_AGENT_500_JOURNAL",
            Self::Timeout { .. } => "SKALD_AGENT_504_TIMEOUT",
            Self::StructuredOutputDecode { .. } => "SKALD_AGENT_422_STRUCTURED_DECODE",
            Self::InvalidArgument { .. } => "SKALD_AGENT_422_INVALID_ARGUMENT",
        }
    }
}

impl From<AgentError> for WyrdError {
    /// Project an owned agent failure onto the derive-backed Wyrd catalog.
    fn from(error: AgentError) -> Self {
        Self::from(&error)
    }
}

impl From<&AgentError> for WyrdError {
    /// Project an agent failure onto the derive-backed Wyrd catalog.
    ///
    /// This is the single source of public metadata for every agent failure
    /// that crosses a public Wyrd boundary: the catalog owns
    /// `code`, `status`, `title`, and `remediation`, while this projection
    /// supplies the human-readable message and the structured `details`
    /// payload carrying the variant's own fields.
    fn from(error: &AgentError) -> Self {
        let message = error.to_string();
        match error {
            AgentError::ToolNotFound { name } => Self::AgentToolNotFound {
                message,
                details: serde_json::json!({ "tool": name }),
            },
            AgentError::ToolNotInAgent { name } => Self::AgentToolNotInAgent {
                message,
                details: serde_json::json!({ "tool": name }),
            },
            AgentError::InvalidToolArgs { tool, detail } => Self::AgentToolArgs {
                message,
                details: serde_json::json!({ "tool": tool, "reason": detail }),
            },
            AgentError::MaxIterations { agent, cap } => Self::AgentMaxIterations {
                message,
                details: serde_json::json!({ "agent": agent, "max_iterations": cap }),
            },
            AgentError::Provider(source) => Self::AgentProviderCall {
                message,
                details: serde_json::json!({ "provider_code": source.code() }),
            },
            AgentError::Tool(source) => Self::ToolInvalidSchema {
                message,
                details: serde_json::json!({ "tool_code": source.code() }),
            },
            AgentError::Prompt { agent, detail } => Self::AgentPromptRender {
                message,
                details: serde_json::json!({ "agent": agent, "reason": detail }),
            },
            AgentError::LoopMessageType { provider, detail } => Self::AgentLoopMessageType {
                message,
                details: serde_json::json!({
                    "provider": format!("{provider:?}"),
                    "reason": detail,
                }),
            },
            AgentError::ProviderMismatch {
                agent,
                agent_provider,
                prompt_provider,
            } => Self::AgentProviderMismatch {
                message,
                details: serde_json::json!({
                    "agent": agent,
                    "agent_provider": format!("{agent_provider:?}"),
                    "prompt_provider": format!("{prompt_provider:?}"),
                }),
            },
            AgentError::DelegationDepthExceeded { chain } => Self::AgentDelegationDepth {
                message,
                details: serde_json::json!({ "chain": chain }),
            },
            AgentError::CallbackPanic { hook, payload } => Self::AgentCallbackPanic {
                message,
                details: serde_json::json!({ "hook": hook, "payload": payload }),
            },
            AgentError::SessionRecentFailed { session_id, .. } => Self::SessionRecentFailed {
                message,
                details: serde_json::json!({ "session_id": session_id }),
            },
            AgentError::SessionAppendFailed { session_id, .. } => Self::SessionAppendFailed {
                message,
                details: serde_json::json!({ "session_id": session_id }),
            },
            AgentError::JournalAppendFailed { .. } => Self::AgentJournalAppend {
                message,
                details: serde_json::json!({}),
            },
            AgentError::Timeout { duration } => Self::AgentTimeout {
                message,
                details: serde_json::json!({ "timeout_ms": duration.as_millis() }),
            },
            AgentError::StructuredOutputDecode { agent, detail } => Self::AgentStructuredDecode {
                message,
                details: serde_json::json!({ "agent": agent, "reason": detail }),
            },
            AgentError::InvalidArgument { name, detail } => Self::AgentInvalidArgument {
                message,
                details: serde_json::json!({ "argument": name, "reason": detail }),
            },
        }
    }
}
