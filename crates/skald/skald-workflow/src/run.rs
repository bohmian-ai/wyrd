//! Workflow run result envelopes.

use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

use skald_spec::ProviderResponse;

use crate::task::TaskStatus;

/// Outcome returned by [`crate::Workflow::run`].
#[derive(Debug, Clone)]
pub struct WorkflowRun {
    /// Final outcome per task, keyed by task id.
    pub tasks: HashMap<String, TaskOutcome>,
    /// Per-task events collected during the run.
    pub events: Vec<TaskEvent>,
    /// Last task id in topological execution order.
    pub last_task_id: Option<String>,
}

impl WorkflowRun {
    /// Return the terminal task's outcome when the workflow has one.
    pub fn result(&self) -> Option<&TaskOutcome> {
        self.last_task_id.as_ref().and_then(|id| self.tasks.get(id))
    }
}

/// Per-task final outcome captured after workflow execution.
#[derive(Debug, Clone)]
pub struct TaskOutcome {
    /// Final task status.
    pub status: TaskStatus,
    /// Final provider response for completed tasks.
    pub result: Option<ProviderResponse>,
    /// Number of retries consumed before terminal status.
    pub retries: u32,
}

/// One observable task transition during a workflow run.
#[derive(Debug, Clone)]
pub struct TaskEvent {
    /// Task id.
    pub task_id: String,
    /// Status recorded for this transition.
    pub status: TaskStatus,
    /// Unix epoch milliseconds when the attempt started.
    pub started_at: i64,
    /// Unix epoch milliseconds when the attempt ended.
    pub ended_at: i64,
    /// Attempt index, starting at 1.
    pub attempt: u32,
    /// Stable error code for failed attempts.
    pub error: Option<String>,
}

pub(crate) fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| i64::try_from(duration.as_millis()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}
