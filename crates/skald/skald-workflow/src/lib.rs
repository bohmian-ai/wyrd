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
//! [`DagExecutor::run`] owns the level-parallel DAG schedule and returns a
//! [`WorkflowRun`] envelope when every task has completed.
//! [`DagExecutor::execute_task`] is the single-task entrypoint used by external
//! orchestrators that own their own loop and step one task at a time.
//!
//! ## User surface
//!
//! [`Workflow`] is the user-facing authoring + run surface. It mirrors the
//! [`skald_agent::Agent`] pyclass-is-the-class pattern: meta + spec + cascade
//! state on one struct, with the same struct serving Rust and Python. The
//! `Workflow::run` method delegates to the internal [`DagExecutor`].
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
#[cfg(feature = "python")]
pub mod python;
pub mod run;
pub mod schedule;
pub mod task;
pub mod tasklist;
pub mod workflow;
pub mod workflow_surface;

pub use context::{Context, ContextSnapshot};
pub use def::{TaskDef, WorkflowAgent, WorkflowDef, default_max_retries};
pub use error::{WorkflowError, WorkflowResult};
pub use handoff::{extract_messages_for_handoff, handoff_messages};
pub use run::{StepEvent, StepOutcome, TaskEvent, TaskOutcome, WorkflowRun};
pub use task::{Task, TaskStatus};
pub use tasklist::TaskList;
pub use workflow::DagExecutor;
pub use workflow_surface::{Workflow, WorkflowInput};

#[cfg(feature = "python")]
pub use python::python_register;
