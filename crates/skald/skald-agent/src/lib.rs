//! Skald agent definitions, run settings, errors, and observer hook.
//!
//! S01 ships data, errors, and observer only.

#![allow(clippy::module_name_repetitions)]

pub mod def;
pub mod error;
pub mod observer;
pub mod run;

pub use def::AgentDef;
pub use error::{AgentError, AgentResult};
pub use observer::{NoopObserver, Observer};
pub use run::{AgentRun, RunConfig};

// Re-export `FinishReason` so callers do not depend on `skald-spec` directly
// just to inspect agent termination cause.
pub use skald_spec::FinishReason;
