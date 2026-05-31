//! Runtime configuration and per-run output for the bounded tool loop.

use serde::{Deserialize, Serialize};
use skald_spec::{FinishReason, ProviderResponse};

/// Configuration for the bounded tool loop.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RunConfig {
    /// Maximum loop iterations before [`crate::AgentError::MaxIterations`].
    pub max_iterations: u32,
}

impl Default for RunConfig {
    fn default() -> Self {
        Self { max_iterations: 10 }
    }
}

/// Output of one successful `Agent::run`.
#[derive(Debug, Clone)]
pub struct AgentRun {
    /// Final native provider response.
    pub output: ProviderResponse,
    /// Number of loop iterations executed (1-based).
    pub iterations: u32,
    /// Why the loop terminated.
    pub finish: FinishReason,
}
