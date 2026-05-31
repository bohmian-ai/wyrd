//! Skald workflow engine data, task state, context, and validation.
//!
//! S04 ships serializable definitions, live task validation, context snapshots,
//! and the workflow error catalog.

#![allow(clippy::module_name_repetitions)]

pub mod context;
pub mod def;
pub mod error;
pub mod task;

pub use context::{Context, ContextSnapshot};
pub use def::{TaskDef, WorkflowDef, default_max_retries};
pub use error::{WorkflowError, WorkflowResult};
pub use task::{Task, TaskStatus};
