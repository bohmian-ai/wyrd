//! Public Wyrd Rust umbrella.
//!
//! Downstream Rust consumers depend on this crate and import from
//! `wyrd::agent` or `wyrd::workflow`. The skald-* sub-package crates are
//! internal layering; external code should not import them directly.
//!
//! # Stability
//!
//! Items re-exported from this crate are stable Rust API. Items not re-exported
//! here are not part of the public surface, even if they are technically `pub`
//! in their owning crate.

#![deny(missing_docs)]
#![allow(clippy::module_name_repetitions)]

/// Agent runtime: identity, single-provider binding, bounded tool loop,
/// observer hook, and `SKALD_AGENT_*` error catalog.
pub mod agent;

/// Workflow engine: DAG scheduling, single-task path, retries, cross-provider
/// handoff, and `SKALD_WORKFLOW_*` error catalog.
pub mod workflow;
