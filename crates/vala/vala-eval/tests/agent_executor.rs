use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use serde_json::{Value, json};
use uuid::Uuid;
use vala_eval::context::ExecutionContext;
use vala_eval::executor::{EvalReport, Executors, TaskRunOutcome, execute_plan};
use vala_eval::store::TaskRegistry;
use vala_eval::tasks::{
    AgentTaskExecutor, AssertionTaskExecutor, JudgeTaskExecutor, TraceTaskExecutor,
};
use vala_eval::{InMemoryTraceSource, MockJudgeInvoker};
use wyrd_spec::vala::eval::{
    AgentAssertionTask, ComparisonOperator, EvalSpec, EvalTask, JsonPath, RecordId, RunId, TaskId,
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
