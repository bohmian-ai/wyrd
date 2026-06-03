//! Agent error catalog.

use crate::journal::JournalError;
use crate::session::SessionError;
use skald_runtime::SkaldRuntimeError;
use skald_spec::ProviderName;
use skald_tool::SkaldToolError;
use thiserror::Error;

/// Result alias used throughout the crate.
pub type AgentResult<T> = Result<T, AgentError>;

/// Failures the agent surface can raise.
#[derive(Debug, Error)]
pub enum AgentError {
    /// The agent dispatched a tool name that is not in its tool registry.
    #[error("tool '{name}' is not registered for this agent")]
    ToolNotFound {
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
        }
    }

    /// Suggested HTTP status for this failure.
    pub const fn status(&self) -> u16 {
        match self {
            Self::ToolNotFound { .. } => 404,
            Self::InvalidToolArgs { .. } | Self::Prompt { .. } | Self::LoopMessageType { .. } => {
                422
            }
            Self::ProviderMismatch { .. } => 409,
            Self::DelegationDepthExceeded { .. } => 412,
            Self::MaxIterations { .. }
            | Self::CallbackPanic { .. }
            | Self::SessionRecentFailed { .. }
            | Self::SessionAppendFailed { .. }
            | Self::JournalAppendFailed { .. } => 500,
            Self::Provider(_) => 502,
            Self::Tool(_) => 400,
        }
    }

    /// Stable problem-title text.
    pub const fn title(&self) -> &'static str {
        match self {
            Self::ToolNotFound { .. } => "Tool not registered for agent",
            Self::InvalidToolArgs { .. } => "Tool arguments invalid",
            Self::MaxIterations { .. } => "Agent exceeded max iterations",
            Self::Provider(_) => "Provider call failed",
            Self::Tool(_) => "Tool declaration malformed",
            Self::Prompt { .. } => "Agent prompt rendering failed",
            Self::LoopMessageType { .. } => "Loop message type mismatch",
            Self::ProviderMismatch { .. } => "Provider mismatch",
            Self::DelegationDepthExceeded { .. } => "Agent delegation depth exceeded",
            Self::CallbackPanic { .. } => "User-supplied callback panicked",
            Self::SessionRecentFailed { .. } => "Session memory recent fetch failed",
            Self::SessionAppendFailed { .. } => "Session memory append failed",
            Self::JournalAppendFailed { .. } => "Journal append failed",
        }
    }

    /// Operator-facing remediation hint.
    pub const fn remediation(&self) -> &'static str {
        match self {
            Self::ToolNotFound { .. } => {
                "Register the tool before running the agent or remove the tool call from the provider response."
            }
            Self::InvalidToolArgs { .. } => {
                "Adjust the provider-emitted tool arguments to match the tool input schema."
            }
            Self::MaxIterations { .. } => {
                "Increase RunConfig::max_iterations or adjust the agent prompt and tools so the loop can terminate."
            }
            Self::Provider(_) => "Inspect the provider backend and retry once it is healthy.",
            Self::Tool(_) => "Inspect the tool declaration name, description, and JSON schema.",
            Self::Prompt { .. } => {
                "Inspect the prompt template and variables passed to Agent::run_prompt."
            }
            Self::LoopMessageType { .. } => {
                "Keep loop history in the same provider-native message family as the agent prompt."
            }
            Self::ProviderMismatch { .. } => {
                "Run the agent with a prompt targeting the same provider as the agent's resolved prompt."
            }
            Self::DelegationDepthExceeded { .. } => {
                "Reduce nested agent-as-tool calls (cap = 3) or restructure the workflow."
            }
            Self::CallbackPanic { .. } => {
                "Inspect the panic payload and fix the callback implementation. Callbacks must not panic."
            }
            Self::SessionRecentFailed { .. } => {
                "Inspect the session backend; ensure connectivity and that session_id exists."
            }
            Self::SessionAppendFailed { .. } => {
                "Inspect the session backend; ensure write permissions."
            }
            Self::JournalAppendFailed { .. } => {
                "Inspect the journal backend; for NoopJournal this should never fire."
            }
        }
    }
}
