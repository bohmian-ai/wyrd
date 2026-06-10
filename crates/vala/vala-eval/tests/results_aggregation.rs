//! Aggregation pipeline, context-capture, and persistence tests.

use std::collections::BTreeMap;

use chrono::Utc;
use serde_json::json;

use vala_eval::{
    AggregationInput, EvalResults, MechanicSubjectInput, ResultsConfig, RunIdentity,
    ScenarioAggregationInput, SubjectKey, TaskSummary, aggregate_run, apply_context_capture,
};
use wyrd_spec::envelope::CardKind;
use wyrd_spec::ids::{CardName, SpaceName};
use wyrd_spec::reference::CardRef;
use wyrd_spec::vala::eval::{
    AssertionResult, ComparisonOperator, EvalContextCapture, EvalPassGate, ScenarioId, TaskId,
};
use wyrd_spec::vala::ids::RunId;
use wyrd_spec::version::VersionBlock;

type TaskRows = Vec<(TaskId, Vec<AssertionResult>)>;
type SubjectRows = Vec<(CardRef, TaskRows)>;

fn card_ref(name: &str) -> CardRef {
    CardRef {
        kind: CardKind::Agent,
        name: CardName::new(name).expect("static card name is valid"),
        version: VersionBlock::parse("1.0.0").expect("static version is valid"),
        space: Some(SpaceName::new("tests").expect("static space is valid")),
        uid: None,
    }
}

fn eval_card_ref() -> CardRef {
    CardRef {
        kind: CardKind::Eval,
        name: CardName::new("rubric").expect("static card name is valid"),
        version: VersionBlock::parse("0.1.0").expect("static version is valid"),
        space: Some(SpaceName::new("tests").expect("static space is valid")),
        uid: None,
    }
}

fn assertion(task: &str, passed: bool, actual: Option<serde_json::Value>) -> AssertionResult {
    AssertionResult {
        task_id: TaskId::new(task).expect("static task id is valid"),
        passed,
        actual,
        expected: json!(null),
        operator: ComparisonOperator::Equals,
        message: None,
        stage: 0,
        started_at: Utc::now(),
        duration_ms: 0,
    }
}

fn identity() -> RunIdentity {
    RunIdentity {
        run_id: RunId::from_string("run-test".to_owned()),
        eval_ref: eval_card_ref(),
        started_at: Utc::now(),
        ended_at: Utc::now(),
    }
}

fn input_with(scenarios: Vec<ScenarioAggregationInput>, config: ResultsConfig) -> AggregationInput {
    AggregationInput {
        identity: identity(),
        scenarios,
        config,
    }
}

fn scenario_input(
    scenario_id: &str,
    subjects: SubjectRows,
    passenger_tasks: Vec<TaskSummary>,
) -> ScenarioAggregationInput {
    let mut mechanic_results = BTreeMap::new();
    for (subject_ref, per_task) in subjects {
        let task_results = per_task.into_iter().collect();
        let key = SubjectKey::from_ref(&subject_ref);
        mechanic_results.insert(
            key,
            MechanicSubjectInput {
                subject_ref,
                task_results,
            },
        );
    }
    ScenarioAggregationInput {
        scenario_id: ScenarioId::new(scenario_id).expect("static scenario id is valid"),
        mechanic_results,
        passenger_tasks,
        conversation_history: vec![],
        started_at: Utc::now(),
        duration_ms: 10,
    }
}

#[test]
fn aggregation_pass_rates_match_per_level() {
    let task_id = TaskId::new("non_empty").expect("static task id is valid");
    let subject = card_ref("agent_a");
    let scenario = scenario_input(
        "scen_a",
        vec![(
            subject.clone(),
            vec![(
                task_id.clone(),
                vec![
                    assertion("non_empty", true, Some(json!("hi"))),
                    assertion("non_empty", false, Some(json!(""))),
                ],
            )],
        )],
        vec![TaskSummary {
            task_id: TaskId::new("final_response_ok").expect("static task id is valid"),
            passed: true,
            stage: 0,
            message: None,
            duration_ms: 1,
        }],
    );
    let output = aggregate_run(input_with(
        vec![scenario],
        ResultsConfig {
            context_capture: None,
            pass_gate: None,
        },
    ))
    .expect("aggregation succeeds");

    let key = SubjectKey::from_ref(&subject);
    assert_eq!(output.metrics.total_tasks, 2);
    assert_eq!(output.metrics.passed_tasks, 1);
    assert!((output.metrics.pass_rate - 0.5).abs() < f64::EPSILON);
    assert!((output.subjects[&key].metrics.pass_rate - 0.5).abs() < f64::EPSILON);
    assert_eq!(output.metrics.per_task_pass_rate[&task_id], 0.5);
    let scenario_id = ScenarioId::new("scen_a").expect("static scenario id is valid");
    assert!(!output.scenarios[&scenario_id].scenario_passed);
    assert_eq!(output.metrics.scenario_pass_rate, 0.0);
}

#[test]
fn pass_gate_overall_threshold_edge() {
    let scenario = scenario_input(
        "scen_a",
        vec![(
            card_ref("agent_a"),
            vec![(
                TaskId::new("t").expect("static task id is valid"),
                vec![
                    assertion("t", true, None),
                    assertion("t", true, None),
                    assertion("t", false, None),
                    assertion("t", false, None),
                ],
            )],
        )],
        vec![],
    );
    let output = aggregate_run(input_with(
        vec![scenario],
        ResultsConfig {
            context_capture: None,
            pass_gate: Some(EvalPassGate::OverallPassRate { threshold: 0.5 }),
        },
    ))
    .expect("aggregation succeeds");
    let verdict = output.pass_gate_verdict.as_ref().expect("verdict exists");
    assert!(verdict.passed, "0.5 >= 0.5 must pass");

    let scenario = scenario_input(
        "scen_b",
        vec![(
            card_ref("agent_b"),
            vec![(
                TaskId::new("t").expect("static task id is valid"),
                vec![assertion("t", true, None), assertion("t", false, None)],
            )],
        )],
        vec![],
    );
    let output = aggregate_run(input_with(
        vec![scenario],
        ResultsConfig {
            context_capture: None,
            pass_gate: Some(EvalPassGate::OverallPassRate { threshold: 0.51 }),
        },
    ))
    .expect("aggregation succeeds");
    let verdict = output.pass_gate_verdict.as_ref().expect("verdict exists");
    assert!(!verdict.passed, "0.5 < 0.51 must fail");
}

#[test]
fn pass_gate_all_pass_on_zero_tasks_fails() {
    let output = aggregate_run(input_with(
        vec![],
        ResultsConfig {
            context_capture: None,
            pass_gate: Some(EvalPassGate::AllPass),
        },
    ))
    .expect("aggregation succeeds");
    let verdict = output.pass_gate_verdict.as_ref().expect("verdict exists");
    assert!(!verdict.passed);
    assert!(verdict.reason.contains("no tasks"));
}

#[test]
fn pass_gate_all_pass_on_all_skipped_subject_set_fails() {
    let scenario = scenario_input(
        "s",
        vec![(
            card_ref("agent_a"),
            vec![(
                TaskId::new("skipped").expect("static task id is valid"),
                vec![assertion("skipped", false, None)],
            )],
        )],
        vec![],
    );
    let output = aggregate_run(input_with(
        vec![scenario],
        ResultsConfig {
            context_capture: None,
            pass_gate: Some(EvalPassGate::AllPass),
        },
    ))
    .expect("aggregation succeeds");
    assert!(!output.pass_gate_verdict.expect("verdict exists").passed);
}

#[test]
fn context_capture_full_keeps_actual_verbatim() {
    let result = assertion("t", true, Some(json!({"x": 1})));
    let output =
        apply_context_capture(result, EvalContextCapture::Full).expect("context capture succeeds");
    assert_eq!(output.actual, Some(json!({"x": 1})));
}

#[test]
fn context_capture_hash_replaces_with_sha256_hex() {
    let result = assertion("t", true, Some(json!({"x": 1, "y": "v"})));
    let output =
        apply_context_capture(result, EvalContextCapture::Hash).expect("context capture succeeds");
    let actual = output.actual.expect("actual is present");
    let hex = actual.as_str().expect("actual is hash string");
    assert_eq!(hex.len(), 64, "sha256 hex must be 64 chars");
    assert!(
        hex.chars()
            .all(|character| character.is_ascii_hexdigit() && !character.is_ascii_uppercase())
    );
}

#[test]
fn context_capture_redact_drops_actual() {
    let result = assertion("t", true, Some(json!("pii")));
    let output = apply_context_capture(result, EvalContextCapture::Redact)
        .expect("context capture succeeds");
    assert!(output.actual.is_none());
}

#[test]
fn save_load_json_round_trip_preserves_everything() {
    let scenario = scenario_input(
        "s",
        vec![(
            card_ref("agent_a"),
            vec![(
                TaskId::new("t").expect("static task id is valid"),
                vec![assertion("t", true, Some(json!("hi")))],
            )],
        )],
        vec![TaskSummary {
            task_id: TaskId::new("final").expect("static task id is valid"),
            passed: true,
            stage: 0,
            message: Some("ok".to_owned()),
            duration_ms: 2,
        }],
    );
    let output = aggregate_run(input_with(
        vec![scenario],
        ResultsConfig {
            context_capture: Some(EvalContextCapture::Full),
            pass_gate: Some(EvalPassGate::AllPass),
        },
    ))
    .expect("aggregation succeeds");

    let raw = output.to_json().expect("serialize results");
    let from_json = EvalResults::from_json(&raw).expect("deserialize results");
    assert_eq!(output, from_json);

    let temp_dir = tempfile::tempdir().expect("temp dir exists");
    let path = temp_dir.path().join("results.json");
    output.save(&path).expect("save results");
    let from_file = EvalResults::load(&path).expect("load results");
    assert_eq!(output, from_file);
}

#[test]
fn empty_run_aggregation_does_not_panic() {
    let output = aggregate_run(input_with(
        vec![],
        ResultsConfig {
            context_capture: None,
            pass_gate: None,
        },
    ))
    .expect("aggregation succeeds");
    assert_eq!(output.scenarios.len(), 0);
    assert_eq!(output.subjects.len(), 0);
    assert_eq!(output.metrics.total_tasks, 0);
    assert_eq!(output.metrics.pass_rate, 0.0);
    assert_eq!(output.metrics.scenario_pass_rate, 0.0);
    assert!(output.pass_gate_verdict.is_none());

    let table = output.as_table();
    assert!(table.contains("EvalRun run-test"));
}
