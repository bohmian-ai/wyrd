use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use serde_json::{Value, json};
use uuid::Uuid;
use vala_eval::scenario::{RecordWithMedia, ScenarioExecutionInputs, execute_scenario};
use vala_eval::store::TaskRegistry;
use vala_eval::tasks::{
    AgentTaskExecutor, AssertionTaskExecutor, EvalMediaBinding, JudgeTaskExecutor, MediaBindings,
    TraceTaskExecutor,
};
use vala_eval::{Executors, InMemoryTraceSource, MockJudgeInvoker};
use wyrd_semver::VersionBlock;
use wyrd_spec::envelope::CardKind;
use wyrd_spec::ids::{CardName, SpaceName};
use wyrd_spec::reference::CardRef;
use wyrd_spec::vala::eval::{
    AssertionTask, ComparisonOperator, EvalScenario, EvalSpec, EvalTask, LlmJudgeTask, RecordId,
    RunId, ScenarioId, ScenarioTask, TaskId,
};
use wyrd_spec::vala::ids::TraceId;

fn tid(value: &str) -> TaskId {
    TaskId::new(value).expect("static task id is valid")
}

fn sid(value: &str) -> ScenarioId {
    ScenarioId::new(value).expect("static scenario id is valid")
}

fn judge_card_ref() -> CardRef {
    CardRef {
        kind: CardKind::Prompt,
        name: CardName::new("scenario-judge").expect("static card name is valid"),
        version: VersionBlock::parse("1.0.0").expect("static version is valid"),
        space: SpaceName::new("default").expect("valid space"),
        uid: None,
    }
}

fn assertion_task() -> EvalTask {
    EvalTask::Assertion(AssertionTask {
        id: tid("assert_score"),
        context_path: Some(
            wyrd_spec::vala::eval::JsonPath::new("$.score").expect("static JSONPath is valid"),
        ),
        item_context_path: None,
        operator: ComparisonOperator::Equals,
        expected: json!(true),
        depends_on: Vec::new(),
        condition: None,
    })
}

fn judge_task() -> EvalTask {
    EvalTask::LlmJudge(LlmJudgeTask {
        id: tid("judge_response"),
        judge_ref: judge_card_ref(),
        context_path: None,
        expected: json!({"passed": true}),
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

fn scenario() -> EvalScenario {
    EvalScenario {
        id: sid("happy_path"),
        initial_query: "Say hello".to_owned(),
        expected_outcome: Some("friendly greeting".to_owned()),
        predefined_turns: Vec::new(),
        simulated_user_persona: None,
        termination_signal: None,
        max_turns: 8,
        tasks: vec![ScenarioTask {
            id: tid("contains_hello"),
            operator: ComparisonOperator::Contains,
            expected: json!("hello"),
            condition: None,
        }],
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

fn media() -> MediaBindings {
    let mut media = MediaBindings::new();
    media.insert(EvalMediaBinding {
        id: "screenshot".to_owned(),
        payload: json!({"kind": "image", "uri": "file:///tmp/screenshot.png"}),
    });
    media
}

fn record(id: u128, trace_hex: &str, context: Value) -> RecordWithMedia {
    RecordWithMedia {
        record_id: RecordId(Uuid::from_u128(id)),
        trace_id: Some(TraceId::from_hex(trace_hex).expect("static trace id is valid")),
        context,
        media: media(),
        required_media: vec!["screenshot".to_owned()],
    }
}

#[tokio::test]
async fn one_pass_emits_mechanic_and_passenger_results() {
    let spec = spec_of(vec![assertion_task(), judge_task()]);
    let plan = spec.execution_plan().expect("test spec has valid DAG");
    let registry = TaskRegistry::from_plan(&plan, spec.tasks.clone()).expect("registry builds");
    let mock = MockJudgeInvoker::new([Ok(json!({"passed": true})), Ok(json!({"passed": true}))]);
    let executors = executors(Arc::clone(&mock));
    let scenario = scenario();
    let records = vec![
        record(
            1,
            "00000000000000000000000000000001",
            json!({"score": true, "response": "first"}),
        ),
        record(
            2,
            "00000000000000000000000000000002",
            json!({"score": true, "response": "second"}),
        ),
    ];
    let final_response = json!("hello from the agent");

    let results = execute_scenario(ScenarioExecutionInputs {
        scenario: &scenario,
        plan: &plan,
        registry: &registry,
        executors: &executors,
        records: &records,
        final_response: &final_response,
        run_id: RunId::from_string("run-scenario".to_owned()),
    })
    .await
    .expect("scenario executes");

    assert_eq!(results.scenario_id, sid("happy_path"));
    assert_eq!(results.mechanic.len(), 4);
    assert!(results.mechanic.iter().all(|result| result.result.passed));
    assert_eq!(results.passenger.len(), 1);
    assert!(results.passenger[0].passed);
    assert_eq!(results.passenger[0].actual, Some(final_response.clone()));

    let calls = mock.calls().await;
    assert_eq!(calls.len(), 2);
    assert_eq!(
        calls[0].1["media"]["screenshot"]["payload"]["uri"],
        json!("file:///tmp/screenshot.png")
    );
    assert_eq!(
        calls[1].1["media"]["screenshot"]["payload"]["uri"],
        json!("file:///tmp/screenshot.png")
    );
}

#[tokio::test]
async fn passenger_pass_runs_even_when_no_records() {
    let spec = spec_of(vec![assertion_task(), judge_task()]);
    let plan = spec.execution_plan().expect("test spec has valid DAG");
    let registry = TaskRegistry::from_plan(&plan, spec.tasks.clone()).expect("registry builds");
    let mock = MockJudgeInvoker::new([]);
    let executors = executors(Arc::clone(&mock));
    let scenario = scenario();
    let final_response = json!("hello after a crash");

    let results = execute_scenario(ScenarioExecutionInputs {
        scenario: &scenario,
        plan: &plan,
        registry: &registry,
        executors: &executors,
        records: &[],
        final_response: &final_response,
        run_id: RunId::from_string("run-no-records".to_owned()),
    })
    .await
    .expect("scenario executes");

    assert!(results.mechanic.is_empty());
    assert_eq!(results.passenger.len(), 1);
    assert!(results.passenger[0].passed);
    assert_eq!(results.passenger[0].actual, Some(final_response));
    assert_eq!(mock.calls().await.len(), 0);
}
