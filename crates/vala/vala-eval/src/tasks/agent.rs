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

#[cfg(test)]
mod agent_executor {
    use std::collections::BTreeMap;
    use std::sync::Arc;
    use std::time::Duration;

    use crate::context::ExecutionContext;
    use crate::executor::{EvalReport, Executors, TaskRunOutcome, execute_plan};
    use crate::store::TaskRegistry;
    use crate::tasks::{
        AgentTaskExecutor, AssertionTaskExecutor, JudgeTaskExecutor, TraceTaskExecutor,
    };
    use crate::{InMemoryTraceSource, MockJudgeInvoker};
    use serde_json::{Value, json};
    use uuid::Uuid;
    use wyrd_spec::vala::eval::{
        AgentAssertionTask, ComparisonOperator, EvalSpec, EvalTask, JsonPath, RecordId, RunId,
        TaskId,
    };

    fn tid(value: &str) -> TaskId {
        TaskId::new(value).expect("static task id is valid")
    }

    fn jp(value: &str) -> JsonPath {
        JsonPath::new(value).expect("static JSONPath is valid")
    }

    fn agent_task(id: &str, path: &str, operator: ComparisonOperator, expected: Value) -> EvalTask {
        EvalTask::AgentAssertion(AgentAssertionTask {
            id: tid(id),
            workflow_field_path: jp(path),
            operator,
            expected,
            depends_on: Vec::new(),
            condition: None,
        })
    }

    fn spec_of(tasks: Vec<EvalTask>) -> EvalSpec {
        let mut map = BTreeMap::new();
        for task in tasks {
            map.insert(task.id().clone(), task);
        }
        EvalSpec {
            subject_ref: None,
            dataset: None,
            tasks: map,
            workflow: None,
            sampling: None,
            pass_gate: None,
            context_capture: None,
        }
    }

    fn executors() -> Executors {
        let mock = MockJudgeInvoker::new(std::iter::empty());
        Executors {
            assertion: Arc::new(AssertionTaskExecutor::new()),
            judge: Arc::new(JudgeTaskExecutor::new(mock)),
            trace: Arc::new(TraceTaskExecutor::new(
                Arc::new(InMemoryTraceSource::new()),
                Duration::from_millis(250),
            )),
            agent: Arc::new(AgentTaskExecutor::new()),
        }
    }

    async fn drive(spec: EvalSpec, base: Value) -> EvalReport {
        let plan = spec.execution_plan().expect("test spec has valid DAG");
        let registry = TaskRegistry::from_plan(&plan, spec.tasks.clone()).expect("registry builds");
        let cx = ExecutionContext::new(
            base,
            RunId::from_string("run-agent".to_owned()),
            RecordId(Uuid::from_u128(13)),
            None,
        );
        execute_plan(&plan, &cx, &registry, &executors())
            .await
            .expect("plan executes")
    }

    fn result<'a>(report: &'a EvalReport, id: &str) -> &'a wyrd_spec::vala::eval::AssertionResult {
        report
            .outcomes
            .iter()
            .find_map(|outcome| match outcome {
                TaskRunOutcome::Ran(result) if result.task_id == tid(id) => Some(result),
                _ => None,
            })
            .expect("task result exists")
    }

    #[tokio::test]
    async fn agent_assertion_passes_on_envelope_field() {
        let report = drive(
            spec_of(vec![agent_task(
                "agent_response",
                "$.response",
                ComparisonOperator::IsNonEmpty,
                json!(null),
            )]),
            json!({"response": "ready"}),
        )
        .await;
        assert!(result(&report, "agent_response").passed);
    }

    #[tokio::test]
    async fn agent_assertion_missing_field_records_message() {
        let report = drive(
            spec_of(vec![agent_task(
                "agent_missing",
                "$.never_there",
                ComparisonOperator::Equals,
                json!("present"),
            )]),
            json!({"response": "ready"}),
        )
        .await;
        let result = result(&report, "agent_missing");
        assert!(!result.passed);
        assert!(
            result
                .message
                .as_deref()
                .expect("failed result has message")
                .contains("$.never_there")
        );
    }

    #[tokio::test]
    async fn agent_assertion_collection_operator_on_tools_called() {
        let report = drive(
            spec_of(vec![agent_task(
                "agent_tools",
                "$.tools_called",
                ComparisonOperator::Length { expected: 2 },
                json!(null),
            )]),
            json!({"tools_called": ["search", "calc"]}),
        )
        .await;
        assert!(result(&report, "agent_tools").passed);
    }
}
