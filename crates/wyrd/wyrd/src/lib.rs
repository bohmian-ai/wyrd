//! Public Wyrd Rust umbrella.
//!
//! Downstream Rust consumers depend on this crate and import from
//! `wyrd::agent`. The skald-* sub-package crates are internal layering;
//! external code should not import them directly.
//!
//! # Stability
//!
//! Items re-exported from this crate are stable Rust API. Items not re-exported
//! here are not part of the public surface, even if they are technically `pub`
//! in their owning crate.

#![deny(missing_docs)]
#![allow(clippy::module_name_repetitions)]

/// Initialize Wyrd subsystems.
///
/// This is idempotent. Rust users may call it manually when they want Skald
/// agent runs to resolve observers through Wyrd observer state.
pub fn init() {
    wyrd_observe_impl::init();
    skald_runtime::refresh_default_registry_from_env();
}

/// Agent runtime: identity, single-provider binding, bounded tool loop,
/// observer hook, and `SKALD_AGENT_*` error catalog.
pub mod agent;

pub use skald_agent::Agent;
pub use skald_runtime::ProviderRegistry;
pub use skald_workflow::Workflow;
pub use wyrd_spec::{AgentCard, WorkflowCard};
