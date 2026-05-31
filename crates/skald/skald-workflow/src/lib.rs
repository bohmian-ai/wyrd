//! Skald workflow engine: DAG scheduling, per-task retry and validation, and
//! cross-provider message handoff.
//!
//! ## Independence
//!
//! `skald-workflow` depends on `skald-agent` and neutral infrastructure only.
//! It does not depend on Wyrd or Vala crates.
//!
//! ## Dual entrypoints
//!
//! [`Workflow::run`] owns the level-parallel DAG schedule and returns a
//! [`WorkflowRun`] envelope when every task has completed.
//! [`Workflow::execute_task`] is the single-task entrypoint used by external
//! orchestrators that own their own loop and step one task at a time.
//!
//! ## Handoff
//!
//! When a downstream task's agent uses a different provider than the upstream
//! task whose output it consumes, the carried messages are translated through
//! [`skald_spec::convert::convert_message_dyn`], the same converter the LLM
//! gateway uses. Workflow-local conversion logic does not exist.
//!
//! ## Errors
//!
//! All public failures surface as [`WorkflowError`] with stable
//! `SKALD_WORKFLOW_*` codes.

#![deny(missing_docs)]
#![allow(clippy::module_name_repetitions)]

pub mod context;
pub mod def;
pub mod error;
pub mod handoff;
pub mod observer_ext;
pub mod run;
pub mod schedule;
pub mod task;
pub mod tasklist;
pub mod workflow;

pub use context::{Context, ContextSnapshot};
pub use def::{TaskDef, WorkflowDef, default_max_retries};
pub use error::{WorkflowError, WorkflowResult};
pub use handoff::{extract_messages_for_handoff, handoff_messages};
pub use run::{TaskEvent, TaskOutcome, WorkflowRun};
pub use schedule::execution_plan;
pub use task::{Task, TaskStatus};
pub use tasklist::{SharedTask, TaskList};
pub use workflow::Workflow;
