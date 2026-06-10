//! `LlmJudgeTask` executor placeholder.

use async_trait::async_trait;

use wyrd_spec::vala::eval::EvalTask;

use crate::context::{ContextSnapshot, TaskOutput};
use crate::error::EvalExecError;
use crate::executor::TaskExecutor;

/// Executor slot for LLM judge tasks. Body lands in commit 9
/// (`10-judge-invoker-and-media.md`).
#[derive(Default)]
pub struct JudgeTaskExecutor;

impl JudgeTaskExecutor {
    /// Fresh judge executor placeholder.
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl TaskExecutor for JudgeTaskExecutor {
    async fn execute(
        &self,
        task: &EvalTask,
        _snapshot: &ContextSnapshot,
        stage: u32,
    ) -> Result<TaskOutput, EvalExecError> {
        Err(EvalExecError::DagInvalid {
            reason: format!(
                "stage {stage} {} task {:?} executor lands in 10-judge-invoker-and-media.md",
                task.discriminator(),
                task.id().as_str()
            ),
        })
    }
}
