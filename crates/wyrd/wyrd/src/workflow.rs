//! Sanctioned `skald-workflow` re-exports.
//!
//! Importing from `wyrd::workflow` is the supported way to reach the workflow
//! engine in Rust. Direct `skald_workflow::*` imports work inside the workspace
//! but are not part of the stable public API.
//!
//! # Public Surface
//!
//! - `Workflow` is the live workflow, built via `Workflow::from_definition`.
//! - `WorkflowDef` is the declarative form, including graph validation.
//! - `TaskDef`, `Task`, and `TaskStatus` are task shapes.
//! - `Context` and `ContextSnapshot` are per-run shared state.
//! - `WorkflowRun`, `TaskOutcome`, and `TaskEvent` are result envelopes.
//! - `WorkflowError` carries stable `SKALD_WORKFLOW_*` codes.
//! - `default_max_retries` returns the default retry budget.
//!
//! Low-level task-list structures and scheduling internals are intentionally
//! not re-exported here.

pub use skald_workflow::{
    Context, ContextSnapshot, Task, TaskDef, TaskEvent, TaskOutcome, TaskStatus, Workflow,
    WorkflowDef, WorkflowError, WorkflowResult, WorkflowRun, default_max_retries,
};
