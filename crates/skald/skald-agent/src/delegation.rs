//! Task-local state for nested agent delegation.
//!
//! Delegation depth is scoped to the current Tokio task. Delegate tools must
//! await sub-agent runs inside these scopes so nested calls see the current
//! chain and depth cap.

/// Maximum supported nested agent-as-tool depth.
pub const DELEGATION_DEPTH_CAP: u32 = 3;

tokio::task_local! {
    /// Current nested delegation depth for this Tokio task.
    pub static WYRD_AGENT_DELEGATION_DEPTH: u32;

    /// Current nested delegation chain for this Tokio task.
    pub static WYRD_AGENT_DELEGATION_CHAIN: std::cell::RefCell<Vec<String>>;
}
