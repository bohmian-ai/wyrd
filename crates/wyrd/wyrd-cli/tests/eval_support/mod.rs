#![allow(dead_code)]

use std::collections::BTreeMap;
use std::path::Path;

use chrono::Utc;
use serde_json::json;
use uuid::Uuid;
use wyrd_semver::VersionBlock;
use wyrd_spec::api_version::ApiVersion;
use wyrd_spec::envelope::{Card, CardKind, Metadata, Relationships, Spec};
use wyrd_spec::ids::{CardName, SpaceName};
use wyrd_spec::metadata::{Annotations, Labels};
use wyrd_spec::reference::CardRef;
use wyrd_spec::vala::eval::record::EvalRecordObservation;
use wyrd_spec::vala::eval::{
    AssertionTask, ComparisonOperator, EvalPassGate, EvalScenario, EvalSpec, EvalTask, JsonPath,
    LlmJudgeTask, RecordId, ScenarioId, ScenarioTask, TaskId,
};
use wyrd_spec::vala::ids::RunId;

pub fn tid(value: &str) -> TaskId {
    TaskId::new(value).expect("static task id is valid")
}

pub fn sid(value: &str) -> ScenarioId {
    ScenarioId::new(value).expect("static scenario id is valid")
}

pub fn card_ref(kind: CardKind, name: &str) -> CardRef {
    CardRef {
        kind,
        name: CardName::new(name).expect("static card name is valid"),
        version: VersionBlock::parse("1.0.0").expect("static version is valid"),
        space: SpaceName::new("tests").expect("static space is valid"),
        uid: None,
    }
}

pub fn eval_ref() -> CardRef {
    card_ref(CardKind::Eval, "cli-eval")
}

pub fn subject_ref() -> CardRef {
    card_ref(CardKind::Agent, "agent-under-test")
}

pub fn eval_card(spec: EvalSpec) -> Card {
    Card {
        api_version: ApiVersion::default(),
        kind: CardKind::Eval,
        metadata: Metadata {
            name: CardName::new("cli-eval").expect("static card name is valid"),
            version: Some(
                VersionBlock::parse("1.0.0")
                    .expect("static version is valid")
                    .into(),
            ),
            bump: None,
            space: Some(SpaceName::new("tests").expect("static space is valid")),
            uid: None,
            labels: Labels::default(),
            annotations: Annotations::default(),
            spec_hash: None,
            artifact_hash: None,
            origin: None,
        },
        spec: Spec::Eval(spec),
        relationships: Relationships::default(),
        status: None,
    }
}

pub fn assertion_task() -> EvalTask {
    EvalTask::Assertion(AssertionTask {
        id: tid("ok_check"),
        context_path: Some(JsonPath::new("$.ok").expect("static jsonpath is valid")),
        item_context_path: None,
        operator: ComparisonOperator::Equals,
        expected: json!(true),
        depends_on: Vec::new(),
        condition: None,
    })
}

pub fn judge_task() -> EvalTask {
    EvalTask::LlmJudge(LlmJudgeTask {
        id: tid("judge_check"),
        judge_ref: card_ref(CardKind::Prompt, "judge"),
        context_path: None,
        expected: json!({"passed": true}),
        operator: ComparisonOperator::Equals,
        depends_on: Vec::new(),
        max_retries: 0,
        condition: None,
    })
}

pub fn spec(tasks: Vec<EvalTask>, pass_gate: Option<EvalPassGate>) -> EvalSpec {
    let tasks = tasks
        .into_iter()
        .map(|task| (task.id().clone(), task))
        .collect::<BTreeMap<_, _>>();
    EvalSpec {
        subject_ref: Some(subject_ref()),
        dataset: None,
        tasks,
        workflow: None,
        sampling: None,
        pass_gate,
        context_capture: None,
    }
}

pub fn scenario() -> EvalScenario {
    scenario_with("happy_path", 1)
}

pub fn scenario_with(id: &str, max_turns: u32) -> EvalScenario {
    EvalScenario {
        id: sid(id),
        initial_query: "Start".to_owned(),
        expected_outcome: Some("Agent says DONE.".to_owned()),
        predefined_turns: Vec::new(),
        simulated_user_persona: None,
        termination_signal: None,
        max_turns,
        tasks: vec![ScenarioTask {
            id: tid("final_contains_done"),
            operator: ComparisonOperator::Contains,
            expected: json!("DONE"),
            condition: None,
        }],
    }
}

pub fn record(ok: bool) -> EvalRecordObservation {
    EvalRecordObservation {
        record_id: RecordId(Uuid::from_u128(if ok { 1 } else { 2 })),
        run_id: RunId::from_string("run-records".to_owned()),
        session_id: None,
        eval_ref: Some(eval_ref()),
        context: json!({ "ok": ok }),
        trace_id: None,
        span_id: None,
        created_at: Utc::now(),
        media: None,
    }
}

pub fn write_eval_card(path: &Path, spec: EvalSpec) {
    let card = eval_card(spec);
    let raw = serde_json::to_string_pretty(&card).expect("card serializes");
    std::fs::write(path, raw).expect("card fixture writes");
}

pub fn write_scenarios(path: &Path) {
    let raw = serde_json::to_string_pretty(&vec![scenario()]).expect("scenario serializes");
    std::fs::write(path, raw).expect("scenario fixture writes");
}

pub fn write_scenarios_many(path: &Path, scenarios: Vec<EvalScenario>) {
    let raw = serde_json::to_string_pretty(&scenarios).expect("scenarios serialize");
    std::fs::write(path, raw).expect("scenario fixture writes");
}

pub fn write_records(path: &Path, records: &[EvalRecordObservation]) {
    let mut raw = String::new();
    for record in records {
        raw.push_str(&serde_json::to_string(record).expect("record serializes"));
        raw.push('\n');
    }
    std::fs::write(path, raw).expect("records fixture writes");
}
