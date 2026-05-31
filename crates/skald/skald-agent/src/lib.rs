//! Live Skald agent runtime: identity, single-provider binding, a bounded
//! tool-dispatch loop, and an observability hook.
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
//! Observability is injected through the [`Observer`] trait;
//! skald never reaches up for a telemetry client.
//!
//! ## Live binding
//!
//! [`AgentDef`] is the serializable form; [`Agent::from_def`] is the only
//! place a live provider client is bound. There is no undefined placeholder and
//! no post-deserialize rebuild step.
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
pub mod builder;
pub mod def;
pub mod error;
pub mod loop_runtime;
pub mod messages;
pub mod observer;
pub mod registry;
pub mod request_builder;
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
