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
use vala_eval::{InMemoryTraceSource, JudgeError, MockJudgeInvoker};
use wyrd_semver::VersionBlock;
use wyrd_spec::envelope::CardKind;
use wyrd_spec::ids::{CardName, SpaceName};
use wyrd_spec::reference::CardRef;
use wyrd_spec::vala::eval::{
    AssertionTask, ComparisonOperator, EvalSpec, EvalTask, JsonPath, LlmJudgeTask, RecordId, RunId,
    TaskId,
};

fn tid(value: &str) -> TaskId {
    TaskId::new(value).expect("static task id is valid")
}

fn jp(value: &str) -> JsonPath {
    JsonPath::new(value).expect("static JSONPath is valid")
}

fn judge_card_ref() -> CardRef {
    CardRef {
        kind: CardKind::Prompt,
        name: CardName::new("eval-judge").expect("static card name is valid"),
        version: VersionBlock::parse("1.0.0").expect("static version is valid"),
        space: SpaceName::new("default").expect("valid space"),
        uid: None,
    }
}

fn llm_judge(
    id: &str,
    expected: Value,
    op: ComparisonOperator,
    deps: &[&str],
    max_retries: u32,
) -> EvalTask {
    EvalTask::LlmJudge(LlmJudgeTask {
        id: tid(id),
        judge_ref: judge_card_ref(),
        context_path: None,
        expected,
        operator: op,
        depends_on: deps.iter().map(|dep| tid(dep)).collect(),
        max_retries,
        condition: None,
    })
}

fn assertion(
    id: &str,
    path: &str,
    op: ComparisonOperator,
    expected: Value,
    deps: &[&str],
) -> EvalTask {
    EvalTask::Assertion(AssertionTask {
        id: tid(id),
        context_path: Some(jp(path)),
        item_context_path: None,
        operator: op,
        expected,
        depends_on: deps.iter().map(|dep| tid(dep)).collect(),
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

fn executors(mock: Arc<MockJudgeInvoker>) -> Executors {
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

fn context(base: Value) -> ExecutionContext {
    ExecutionContext::new(
        base,
        RunId::from_string("run-llm-judge".to_owned()),
        RecordId(Uuid::from_u128(10)),
        None,
    )
}

async fn drive(spec: EvalSpec, base: Value, mock: Arc<MockJudgeInvoker>) -> EvalReport {
    let plan = spec.execution_plan().expect("test spec has valid DAG");
    let registry = TaskRegistry::from_plan(&plan, spec.tasks.clone()).expect("registry builds");
    execute_plan(&plan, &context(base), &registry, &executors(mock))
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
async fn missing_invoker_is_construction_concern_not_runtime() {
    let mock = MockJudgeInvoker::new([Ok(json!("pass"))]);
    let spec = spec_of(vec![llm_judge(
        "judge",
        json!("pass"),
        ComparisonOperator::Equals,
        &[],
        0,
    )]);
    let report = drive(spec, json!({"response": "ok"}), mock).await;
    assert!(result(&report, "judge").passed);
}

#[tokio::test]
async fn happy_path_stores_verdict_addressable_downstream() {
    let mock = MockJudgeInvoker::new([Ok(json!({"verdict": "pass"}))]);
    let spec = spec_of(vec![
        llm_judge(
            "judge",
            json!({"verdict": "pass"}),
            ComparisonOperator::Equals,
            &[],
            0,
        ),
        assertion(
            "downstream",
            "$.task.judge.parsed.verdict",
            ComparisonOperator::Equals,
            json!("pass"),
            &["judge"],
        ),
    ]);
    let report = drive(spec, json!({"response": "ok"}), mock).await;
    assert!(result(&report, "judge").passed);
    assert!(result(&report, "downstream").passed);
}

#[tokio::test]
async fn retries_then_succeeds() {
    let mock = MockJudgeInvoker::new([
        Err(JudgeError::Retryable {
            reason: "provider 503".to_owned(),
        }),
        Err(JudgeError::Timeout { elapsed_ms: 250 }),
        Ok(json!("pass")),
    ]);
    let spec = spec_of(vec![llm_judge(
        "judge",
        json!("pass"),
        ComparisonOperator::Equals,
        &[],
        3,
    )]);
    let report = drive(spec, json!({"response": "ok"}), Arc::clone(&mock)).await;
    assert!(result(&report, "judge").passed);
    assert_eq!(mock.calls().await.len(), 3);
}

#[tokio::test]
async fn retry_budget_exhausted_errors() {
    let mock = MockJudgeInvoker::new([
        Err(JudgeError::Retryable {
            reason: "provider 503".to_owned(),
        }),
        Err(JudgeError::Retryable {
            reason: "provider still 503".to_owned(),
        }),
    ]);
    let spec = spec_of(vec![llm_judge(
        "judge",
        json!("pass"),
        ComparisonOperator::Equals,
        &[],
        1,
    )]);
    let report = drive(spec, json!({"response": "ok"}), Arc::clone(&mock)).await;
    let result = result(&report, "judge");
    assert!(!result.passed);
    assert_eq!(mock.calls().await.len(), 2);
    assert!(
        result
            .message
            .as_deref()
            .unwrap_or("")
            .contains("judge failed after 2 attempt")
    );
}

#[tokio::test]
async fn invalid_structured_output_short_circuits() {
    let mock = MockJudgeInvoker::new([Err(JudgeError::InvalidStructuredOutput {
        reason: "missing verdict".to_owned(),
    })]);
    let spec = spec_of(vec![llm_judge(
        "judge",
        json!("pass"),
        ComparisonOperator::Equals,
        &[],
        3,
    )]);
    let report = drive(spec, json!({"response": "ok"}), Arc::clone(&mock)).await;
    let result = result(&report, "judge");
    assert!(!result.passed);
    assert_eq!(mock.calls().await.len(), 1);
    assert!(
        result
            .message
            .as_deref()
            .unwrap_or("")
            .contains("missing verdict")
    );
}

#[tokio::test]
async fn context_path_narrows_invoker_input() {
    let mock = MockJudgeInvoker::new([Ok(json!("pass"))]);
    let mut task = match llm_judge("judge", json!("pass"), ComparisonOperator::Equals, &[], 0) {
        EvalTask::LlmJudge(task) => task,
        _ => unreachable!("fixture returns judge"),
    };
    task.context_path = Some(jp("$.response"));
    let spec = spec_of(vec![EvalTask::LlmJudge(task)]);

    let report = drive(
        spec,
        json!({"response": "what the judge sees", "extra": "hidden"}),
        Arc::clone(&mock),
    )
    .await;
    assert!(result(&report, "judge").passed);
    let calls = mock.calls().await;
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].1, json!("what the judge sees"));
}
