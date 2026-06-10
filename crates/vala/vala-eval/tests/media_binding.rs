use std::collections::BTreeMap;
use std::sync::Arc;

use serde_json::{Value, json};
use uuid::Uuid;
use vala_eval::MockJudgeInvoker;
use vala_eval::context::ExecutionContext;
use vala_eval::executor::{EvalReport, Executors, TaskRunOutcome, execute_plan};
use vala_eval::store::TaskRegistry;
use vala_eval::tasks::{
    AgentTaskExecutor, AssertionTaskExecutor, EvalMediaBinding, JudgeTaskExecutor, MediaBindings,
    TraceTaskExecutor,
};
use wyrd_spec::envelope::CardKind;
use wyrd_spec::ids::CardName;
use wyrd_spec::reference::CardRef;
use wyrd_spec::vala::eval::{
    ComparisonOperator, EvalSpec, EvalTask, LlmJudgeTask, RecordId, RunId, TaskId,
};
use wyrd_spec::version::VersionBlock;

fn tid(value: &str) -> TaskId {
    TaskId::new(value).expect("static task id is valid")
}

fn judge_card_ref() -> CardRef {
    CardRef {
        kind: CardKind::Prompt,
        name: CardName::new("media-judge").expect("static card name is valid"),
        version: VersionBlock::parse("1.0.0").expect("static version is valid"),
        space: None,
        uid: None,
    }
}

fn judge_task() -> EvalTask {
    EvalTask::LlmJudge(LlmJudgeTask {
        id: tid("judge"),
        judge_ref: judge_card_ref(),
        context_path: None,
        expected: json!("pass"),
        operator: ComparisonOperator::Equals,
        depends_on: Vec::new(),
        max_retries: 0,
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
        trace: Arc::new(TraceTaskExecutor::new()),
        agent: Arc::new(AgentTaskExecutor::new()),
    }
}

async fn drive(
    spec: EvalSpec,
    base: Value,
    media: MediaBindings,
    required_media: Vec<String>,
    mock: Arc<MockJudgeInvoker>,
) -> EvalReport {
    let plan = spec.execution_plan().expect("test spec has valid DAG");
    let registry = TaskRegistry::from_plan(&plan, spec.tasks.clone()).expect("registry builds");
    let cx = ExecutionContext::new(
        base,
        RunId::from_string("run-media".to_owned()),
        RecordId(Uuid::from_u128(11)),
        None,
    )
    .with_media(media, required_media);
    execute_plan(&plan, &cx, &registry, &executors(mock))
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
async fn bound_media_appears_in_invoker_context_under_media_key() {
    let mut media = MediaBindings::new();
    media.insert(EvalMediaBinding {
        id: "image_under_review".to_owned(),
        payload: json!({
            "kind": "image",
            "url": "file:///tmp/example.png",
        }),
    });
    let mock = MockJudgeInvoker::new([Ok(json!("pass"))]);
    let report = drive(
        spec_of(vec![judge_task()]),
        json!({"response": "look at this"}),
        media,
        Vec::new(),
        Arc::clone(&mock),
    )
    .await;
    assert!(result(&report, "judge").passed);
    let calls = mock.calls().await;
    assert_eq!(calls.len(), 1);
    assert_eq!(
        calls[0].1["media"]["image_under_review"]["payload"]["url"],
        json!("file:///tmp/example.png")
    );
}

#[tokio::test]
async fn required_media_id_unbound_errors_without_invoking_judge() {
    let mock = MockJudgeInvoker::new([Ok(json!("pass"))]);
    let report = drive(
        spec_of(vec![judge_task()]),
        json!({"response": "look at this"}),
        MediaBindings::new(),
        vec!["image_under_review".to_owned()],
        Arc::clone(&mock),
    )
    .await;
    let result = result(&report, "judge");
    assert!(!result.passed);
    assert!(
        result
            .message
            .as_deref()
            .unwrap_or("")
            .contains("required media id")
    );
    assert_eq!(mock.calls().await.len(), 0);
}
