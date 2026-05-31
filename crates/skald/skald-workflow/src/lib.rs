//! Skald workflow engine data, task state, context, and validation.
//!
//! S04 ships serializable definitions, live task validation, context snapshots,
//! and the workflow error catalog.

#![allow(clippy::module_name_repetitions)]

pub mod context;
pub mod def;
pub mod error;
pub mod observer_ext;
pub mod run;
pub mod schedule;
pub mod task;
pub mod tasklist;
pub mod workflow;

pub use context::{Context, ContextSnapshot};
pub use def::{TaskDef, WorkflowDef, default_max_retries};
pub use error::{WorkflowError, WorkflowResult};
pub use run::{TaskEvent, TaskOutcome, WorkflowRun};
pub use schedule::execution_plan;
pub use task::{Task, TaskStatus};
pub use tasklist::{SharedTask, TaskList};
pub use workflow::Workflow;
