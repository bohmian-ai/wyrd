//! Skald agent definitions, binding, run settings, errors, and observer hook.
//!
//! S02 binds declared agents to live providers and executable tools.

#![allow(clippy::module_name_repetitions)]

pub mod agent;
pub mod builder;
pub mod def;
pub mod error;
pub mod observer;
pub mod registry;
pub mod run;
pub mod tool;

pub use agent::Agent;
pub use builder::AgentBuilder;
pub use def::AgentDef;
pub use error::{AgentError, AgentResult};
pub use observer::{NoopObserver, Observer};
pub use registry::system_messages;
pub use run::{AgentRun, RunConfig};
pub use tool::{AgentTool, AgentToolError, ToolRegistry};

// Re-export `FinishReason` so callers do not depend on `skald-spec` directly
// just to inspect agent termination cause.
pub use skald_spec::FinishReason;
