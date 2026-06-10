//! `TraceAssertionTask` executor placeholder.

use async_trait::async_trait;

use wyrd_spec::vala::eval::EvalTask;

use crate::context::{ContextSnapshot, TaskOutput};
use crate::error::EvalExecError;
use crate::executor::TaskExecutor;

/// Executor slot for trace assertion tasks. Body lands in commit 10
/// (`11-trace-and-agent-executors.md`).
#[derive(Default)]
pub struct TraceTaskExecutor;

impl TraceTaskExecutor {
    /// Fresh trace executor placeholder.
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl TaskExecutor for TraceTaskExecutor {
    async fn execute(
        &self,
        task: &EvalTask,
        _snapshot: &ContextSnapshot,
        stage: u32,
    ) -> Result<TaskOutput, EvalExecError> {
        Err(EvalExecError::DagInvalid {
            reason: format!(
                "stage {stage} {} task {:?} executor lands in 11-trace-and-agent-executors.md",
                task.discriminator(),
                task.id().as_str()
            ),
        })
    }
}
