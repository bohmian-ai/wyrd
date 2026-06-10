//! `AgentAssertionTask` executor.

use async_trait::async_trait;
use chrono::Utc;
use serde_json::Value;

use wyrd_spec::vala::eval::{AgentAssertionTask, AssertionResult, EvalTask};

use crate::context::{ContextSnapshot, TaskOutput, extract_jsonpath_from};
use crate::error::EvalExecError;
use crate::executor::TaskExecutor;
use crate::operators;

/// Executor for agent assertion tasks.
#[derive(Default)]
pub struct AgentTaskExecutor;

impl AgentTaskExecutor {
    /// Fresh agent executor.
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl TaskExecutor for AgentTaskExecutor {
    async fn execute(
        &self,
        task: &EvalTask,
        snapshot: &ContextSnapshot,
        stage: u32,
    ) -> Result<TaskOutput, EvalExecError> {
        let EvalTask::AgentAssertion(agent_task) = task else {
            return Err(EvalExecError::DagInvalid {
                reason: format!(
                    "stage {stage} dispatched {} task {:?} to agent executor",
                    task.discriminator(),
                    task.id().as_str()
                ),
            });
        };
        let result = execute_agent(agent_task, snapshot, stage)?;
        Ok(TaskOutput::Agent(result))
    }
}

fn execute_agent(
    task: &AgentAssertionTask,
    snapshot: &ContextSnapshot,
    stage: u32,
) -> Result<AssertionResult, EvalExecError> {
    let started_at = Utc::now();
    let view = snapshot.build_scoped_view(&task.depends_on);
    let observed = extract_jsonpath_from(&view, &task.workflow_field_path)?;
    let verdict = operators::evaluate_operator(&observed, &task.operator, &task.expected).map_err(
        |error| EvalExecError::OperatorTypeMismatch {
            task_id: task.id.clone(),
            operator: format!("{:?}", task.operator),
            reason: error.to_string(),
        },
    )?;
    let message = if !verdict.passed && observed == Value::Null {
        Some(format!(
            "workflow_field_path {} did not resolve on the agent envelope",
            task.workflow_field_path.as_str()
        ))
    } else if !verdict.passed {
        Some(format!(
            "operator {:?} returned false for workflow_field_path {}",
            task.operator,
            task.workflow_field_path.as_str()
        ))
    } else {
        None
    };

    Ok(AssertionResult {
        task_id: task.id.clone(),
        passed: verdict.passed,
        actual: verdict.observed,
        expected: verdict.expected.unwrap_or_else(|| task.expected.clone()),
        operator: task.operator.clone(),
        message,
        stage,
        started_at,
        duration_ms: elapsed_ms(started_at),
    })
}

fn elapsed_ms(start: chrono::DateTime<chrono::Utc>) -> u64 {
    let diff = Utc::now().signed_duration_since(start);
    diff.num_milliseconds().max(0) as u64
}
