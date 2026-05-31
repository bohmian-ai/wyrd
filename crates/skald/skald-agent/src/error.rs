//! Agent error catalog.

use skald_runtime::SkaldRuntimeError;
use skald_spec::ProviderName;
use skald_tool::SkaldToolError;
use thiserror::Error;

/// Result alias used throughout the crate.
pub type AgentResult<T> = Result<T, AgentError>;

/// Failures the agent surface can raise.
#[derive(Debug, Error)]
pub enum AgentError {
    /// The provider registry returned no client for the agent's provider name.
    #[error("provider not registered: {provider:?}")]
    ProviderNotFound {
        /// Missing provider name from the agent def.
        provider: ProviderName,
    },
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
    /// The system prompt failed to render into native messages at bind time.
    #[error("system prompt invalid for {provider:?}: {detail}")]
    SystemPrompt {
        /// Provider the system prompt was being shaped for.
        provider: ProviderName,
        /// Reason the conversion failed.
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
            Self::ProviderNotFound { .. } => "SKALD_AGENT_404_PROVIDER",
            Self::ToolNotFound { .. } => "SKALD_AGENT_404_TOOL",
            Self::InvalidToolArgs { .. } => "SKALD_AGENT_422_TOOL_ARGS",
            Self::SystemPrompt { .. } => "SKALD_AGENT_422_SYSTEM_PROMPT",
            Self::Prompt { .. } => "SKALD_AGENT_422_PROMPT",
            Self::LoopMessageType { .. } => "SKALD_AGENT_422_LOOP_MESSAGE_TYPE",
            Self::ProviderMismatch { .. } => "SKALD_AGENT_409_PROVIDER_MISMATCH",
            Self::MaxIterations { .. } => "SKALD_AGENT_500_MAX_ITERATIONS",
            Self::Provider(_) => "SKALD_AGENT_502_PROVIDER",
            Self::Tool(source) => source.code(),
        }
    }
}
