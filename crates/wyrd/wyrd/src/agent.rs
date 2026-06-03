//! User-facing agent surface.
//!
//! The canonical import path is `use wyrd::agent::Agent;`.
//!
//! # Public Surface
//!
//! `Agent` is the Wyrd card authoring holder for an agent with resolved runtime
//! state. Advanced envelope access remains under `wyrd_cards::envelope`.

pub use skald_agent::observer::{NoopObserver, Observer};
pub use skald_agent::{
    AgentContext, AgentDelegateTool, AgentRun, CallbackOutcome, Conversation, ConversationTurn,
    FinishReason, Journal, JournalEvent, NoSession, NoopJournal, Role, RunConfig, SessionId,
    SessionMemory, SessionTurn,
};
pub use skald_prompt::Prompt;
pub use skald_tool::{AgentTool, ToolDef, ToolError};
pub use wyrd_cards::agent::{AgentBuilder, AgentMetadata, AgentWithMeta as Agent};
