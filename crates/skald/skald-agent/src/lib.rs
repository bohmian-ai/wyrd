//! Agentic runtime for bounded Skald loops, tool execution, callbacks, session
//! memory, journals, and observers.
//!
//! Users enter through [`Agent`]: `Agent::new(prompt).with_tool(tool).run("draft
//! the doc").await?`. `Agent::new` takes an already-resolved
//! [`skald_prompt::Prompt`] and is infallible; execution errors surface from
//! [`Agent::run`].
//!
//! Adjacent crates own adjacent surfaces: [`wyrd_spec::AgentCard`] is the
//! durable on-disk envelope, [`skald_tool::AgentTool`] and [`AgentDelegateTool`]
//! provide callable tools and delegation, and `skald-observer` auto-attaches
//! observers when configured by the Wyrd runtime.

#![deny(missing_docs)]

pub mod agent;
pub mod callbacks;
pub mod card_error;
pub mod conversation;
pub mod delegate;
pub mod delegation;
pub mod error;
pub mod journal;
pub mod loop_runtime;
#[cfg(feature = "python")]
pub mod py_error;
#[cfg(feature = "python")]
pub mod python;
pub mod registry;
pub mod request_builder;
pub mod run;
pub mod session;

pub use agent::{
    Agent, AgentCallbacks, AgentWire, LocalPromptResolver, PromptResolver,
    agent_run_config_spec_from_run_config, clear_prompt_card_registry, default_prompt_resolver,
    derive_cascade_children, register_prompt_card, run_config_from_agent_run_config_spec,
};
pub use callbacks::{
    AfterAgentFn, AfterModelFn, AfterToolFn, AgentContext, BeforeAgentFn, BeforeModelFn,
    BeforeToolFn, CallbackOutcome,
};
pub use card_error::AgentCardError;
pub use conversation::{Conversation, ConversationTurn};
pub use delegate::AgentDelegateTool;
pub use error::{AgentError, AgentResult};
pub use journal::{Journal, JournalError, JournalEvent, NoopJournal};
#[cfg(feature = "python")]
pub use python::python_register;
pub use registry::system_messages;
pub use run::{AgentRun, FinishReason, RunConfig, RunError};
pub use session::{NoSession, Role, SessionError, SessionId, SessionMemory, SessionTurn};
pub use skald_observer::{NoopObserver, Observer};
