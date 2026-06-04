//! User-facing agent surface.
//!
//! The canonical import path is `use wyrd::agent::Agent;`.
//!
//! # Public Surface
//!
//! `Agent` is the Wyrd card authoring holder and Skald runtime for an agent
//! with resolved prompt state. `Workflow` lives in the same module because a
//! Wyrd workflow is composition of agents.

pub use skald_agent::observer::{NoopObserver, Observer};
pub use skald_agent::{
    Agent, AgentCallbacks, AgentContext, AgentDelegateTool, AgentRun, AgentWire, CallbackOutcome,
    Conversation, ConversationTurn, FinishReason, Journal, JournalEvent, LocalPromptResolver,
    NoSession, NoopJournal, PromptResolver, Role, RunConfig, SessionId, SessionMemory, SessionTurn,
    clear_prompt_card_registry, default_prompt_resolver, register_prompt_card,
};
pub use skald_prompt::Prompt;
pub use skald_runtime::ProviderRegistry;
pub use skald_tool::{AgentTool, ToolDef, ToolError};
pub use skald_workflow::{StepEvent, StepOutcome, TaskStatus as StepStatus, Workflow, WorkflowRun};
pub use wyrd_spec::{AgentCard, WorkflowCard};
