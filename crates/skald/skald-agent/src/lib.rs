//! Live Skald agent runtime: identity, native prompt execution, and a bounded
//! tool-dispatch loop.
//!
//! This crate sits above [`skald_runtime`] and below the Wyrd API holders that
//! will project skald agents into the public card surface. The runtime is
//! provider-native end to end: every request is a
//! [`skald_spec::ProviderRequest`] built for the agent's chosen provider, and
//! every response is read via [`skald_spec::ResponseAdapter`].
//!
//! ## Independence
//!
//! `skald-agent` depends on `skald-spec`, `skald-runtime`, `skald-tool`, and
//! neutral infrastructure only. There is no dependency on Wyrd or Vala crates.
//!
//! ## Live binding
//!
//! Construct [`Agent`] directly from an already-resolved [`skald_prompt::Prompt`]
//! and register runtime clients at call time.
//!
//! ## Bounded loop
//!
//! [`Agent::run`] runs a bounded tool-dispatch loop with a configurable
//! [`RunConfig::max_iterations`] cap. Each iteration sends a native
//! [`skald_spec::ProviderRequest`], reads the [`skald_spec::ProviderResponse`]
//! via [`skald_spec::ResponseAdapter`], and either terminates when there are no
//! tool calls or dispatches each tool through the registered [`AgentTool`]
//! implementations and continues.
//!
//! ## Errors
//!
//! All public failures surface as [`AgentError`] with stable `SKALD_AGENT_*`
//! codes for boundary mapping.

#![deny(missing_docs)]
#![allow(clippy::module_name_repetitions)]

pub mod agent;
pub mod callbacks;
pub mod conversation;
pub mod error;
pub mod journal;
pub mod loop_runtime;
pub mod observer;
pub mod observer_provider;
pub mod registry;
pub mod request_builder;
pub mod run;
pub mod session;

pub use agent::Agent;
pub use callbacks::{
    AfterAgentFn, AfterModelFn, AfterToolFn, AgentContext, BeforeAgentFn, BeforeModelFn,
    BeforeToolFn, CallbackOutcome,
};
pub use conversation::{Conversation, ConversationTurn};
pub use error::{AgentError, AgentResult};
pub use journal::{Journal, JournalError, JournalEvent, NoopJournal};
pub use observer::{NoopObserver, Observer};
pub use observer_provider::{ObserverProvider, current_observer, set_observer_provider};
pub use registry::system_messages;
pub use run::{AgentRun, FinishReason, RunConfig, RunError};
pub use session::{NoSession, Role, SessionError, SessionId, SessionMemory, SessionTurn};
