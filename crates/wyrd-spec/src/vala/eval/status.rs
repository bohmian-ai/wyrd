//! Eval runtime status values shared by the primitive surface.

use serde::{Deserialize, Serialize};

/// Lifecycle status of an eval record as it moves through the runtime inbox.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum EvalStatus {
    /// Enqueued and not yet claimed by an eval worker.
    Pending,
    /// Waiting for upstream trace data before evaluation can continue.
    AwaitingTrace,
    /// Claimed and currently being evaluated.
    Processing,
    /// Completed successfully, including pass or fail outcomes.
    Completed,
    /// Reached a terminal failure state.
    Failed,
    /// Exhausted retries and moved to dead-letter storage.
    DeadLettered,
}
