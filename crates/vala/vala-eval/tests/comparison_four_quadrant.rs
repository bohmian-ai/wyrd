//! Four-quadrant comparison tests.

use std::collections::BTreeMap;
use std::path::PathBuf;

use chrono::Utc;
use serde_json::json;

use vala_eval::{
    AggregationInput, ChangeFlag, CompareConfig, EvalResults, MechanicSubjectInput, ResultsConfig,
    RunIdentity, ScenarioAggregationInput, SubjectKey, TaskSummary, aggregate_run, compare,
};
use wyrd_semver::VersionBlock;
use wyrd_spec::envelope::CardKind;
use wyrd_spec::ids::{CardName, SpaceName};
use wyrd_spec::reference::CardRef;
use wyrd_spec::vala::eval::{AssertionResult, ComparisonOperator, ScenarioId, TaskId};
use wyrd_spec::vala::ids::RunId;

type TaskCounts<'a> = Vec<(&'a str, usize, usize)>;
type SubjectCounts<'a> = Vec<(CardRef, TaskCounts<'a>)>;

fn card_ref(kind: CardKind, name: &str) -> CardRef {
    CardRef {
        kind,
        name: CardName::new(name).expect("static card name is valid"),
        version: VersionBlock::parse("1.0.0").expect("static version is valid"),
        space: SpaceName::new("tests").expect("static space is valid"),
        uid: None,
    }
}

fn eval_ref() -> CardRef {
    card_ref(CardKind::Eval, "rubric")
}

fn assertion(task: &str, passed: bool) -> AssertionResult {
    AssertionResult {
        task_id: TaskId::new(task).expect("static task id is valid"),
        passed,
        actual: None,
        expected: json!(null),
        operator: ComparisonOperator::Equals,
        message: None,
        stage: 0,
        started_at: Utc::now(),
        duration_ms: 0,
    }
}

fn build_run(
    run_id: &str,
    subjects: SubjectCounts<'_>,
    scenario_id: &str,
    passenger_passed: Option<bool>,
) -> EvalResults {
    let mut mechanic_results = BTreeMap::new();
    for (subject_ref, task_counts) in subjects {
        let mut task_results = BTreeMap::new();
        for (task, pass_count, fail_count) in task_counts {
            let task_id = TaskId::new(task).expect("static task id is valid");
            let mut rows = Vec::with_capacity(pass_count + fail_count);
            for _ in 0..pass_count {
                rows.push(assertion(task, true));
            }
            for _ in 0..fail_count {
                rows.push(assertion(task, false));
            }
            task_results.insert(task_id, rows);
        }

        let subject_key = SubjectKey::from_ref(&subject_ref);
        mechanic_results.insert(
            subject_key,
            MechanicSubjectInput {
                subject_ref,
                task_results,
            },
        );
    }

    let passenger_tasks = passenger_passed
        .map(|passed| {
            vec![TaskSummary {
                task_id: TaskId::new("final").expect("static task id is valid"),
                passed,
                stage: 0,
                message: None,
                duration_ms: 1,
            }]
        })
        .unwrap_or_default();

    aggregate_run(AggregationInput {
        identity: RunIdentity {
            run_id: RunId::from_string(run_id.to_owned()),
            eval_ref: eval_ref(),
            started_at: Utc::now(),
            ended_at: Utc::now(),
        },
        scenarios: vec![ScenarioAggregationInput {
            scenario_id: ScenarioId::new(scenario_id).expect("static scenario id is valid"),
            mechanic_results,
            passenger_tasks,
            conversation_history: vec![],
            started_at: Utc::now(),
            duration_ms: 10,
        }],
        config: ResultsConfig {
            context_capture: None,
            pass_gate: None,
        },
    })
    .expect("aggregation succeeds")
}

#[test]
fn regression_detected_on_pass_rate_drop_above_threshold() {
    let subject = card_ref(CardKind::Agent, "agent_a");
    let baseline = build_run(
        "b",
        vec![(subject.clone(), vec![("t", 10, 0)])],
        "s",
        Some(true),
    );
    let candidate = build_run(
        "c",
        vec![(subject.clone(), vec![("t", 5, 5)])],
        "s",
        Some(true),
    );

    let out = compare(&baseline, &candidate, &CompareConfig::default());
    let key = SubjectKey::from_ref(&subject);

    assert_eq!(out.subjects[&key].flag, ChangeFlag::Regressed);
    assert!(out.regressed_subjects.contains(&key));
    assert_eq!(out.system.flag, ChangeFlag::Regressed);
    assert!((out.system.pass_rate_delta + 0.5).abs() < 1e-9);
}

#[test]
fn stable_run_produces_no_regression() {
    let subject = card_ref(CardKind::Agent, "agent_a");
    let baseline = build_run(
        "b",
        vec![(subject.clone(), vec![("t", 10, 0)])],
        "s",
        Some(true),
    );
    let candidate = build_run(
        "c",
        vec![(subject.clone(), vec![("t", 10, 0)])],
        "s",
        Some(true),
    );

    let out = compare(&baseline, &candidate, &CompareConfig::default());
    let key = SubjectKey::from_ref(&subject);

    assert_eq!(out.subjects[&key].flag, ChangeFlag::Unchanged);
    assert!(out.regressed_subjects.is_empty());
    assert!(out.improved_subjects.is_empty());
}

#[test]
fn improvement_detected_on_pass_rate_rise_above_threshold() {
    let subject = card_ref(CardKind::Agent, "agent_a");
    let baseline = build_run(
        "b",
        vec![(subject.clone(), vec![("t", 0, 10)])],
        "s",
        Some(false),
    );
    let candidate = build_run(
        "c",
        vec![(subject.clone(), vec![("t", 10, 0)])],
        "s",
        Some(true),
    );

    let out = compare(&baseline, &candidate, &CompareConfig::default());
    let key = SubjectKey::from_ref(&subject);

    assert_eq!(out.subjects[&key].flag, ChangeFlag::Improved);
    assert!(out.improved_subjects.contains(&key));
}

#[test]
fn task_status_change_surfaced() {
    let subject = card_ref(CardKind::Agent, "agent_a");
    let baseline = build_run(
        "b",
        vec![(subject.clone(), vec![("t", 10, 0)])],
        "s",
        Some(true),
    );
    let candidate = build_run(
        "c",
        vec![(subject.clone(), vec![("t", 0, 10)])],
        "s",
        Some(false),
    );

    let out = compare(&baseline, &candidate, &CompareConfig::default());
    let key = SubjectKey::from_ref(&subject);
    let changes = &out.subjects[&key].task_status_changes;

    assert_eq!(changes.len(), 1);
    assert_eq!(
        changes[0].task_id,
        TaskId::new("t").expect("static task id is valid")
    );
    assert!(changes[0].baseline_passed);
    assert!(!changes[0].candidate_passed);
}

#[test]
fn missing_tasks_surfaced_in_both_directions() {
    let subject = card_ref(CardKind::Agent, "agent_a");
    let baseline = build_run(
        "b",
        vec![(subject.clone(), vec![("t1", 5, 5), ("t2", 3, 0)])],
        "s",
        Some(true),
    );
    let candidate = build_run(
        "c",
        vec![(subject.clone(), vec![("t1", 5, 5), ("t3", 3, 0)])],
        "s",
        Some(true),
    );

    let out = compare(&baseline, &candidate, &CompareConfig::default());
    let key = SubjectKey::from_ref(&subject);
    let subject_delta = &out.subjects[&key];

    assert_eq!(
        subject_delta.missing_tasks_candidate_only,
        vec![TaskId::new("t2").expect("static task id is valid")]
    );
    assert_eq!(
        subject_delta.missing_tasks_baseline_only,
        vec![TaskId::new("t3").expect("static task id is valid")]
    );
}

#[test]
fn new_and_removed_scenarios_surfaced() {
    let subject = card_ref(CardKind::Agent, "agent_a");
    let baseline = build_run(
        "b",
        vec![(subject.clone(), vec![("t", 5, 5)])],
        "s_only_b",
        Some(true),
    );
    let candidate = build_run(
        "c",
        vec![(subject, vec![("t", 5, 5)])],
        "s_only_c",
        Some(true),
    );

    let out = compare(&baseline, &candidate, &CompareConfig::default());

    assert_eq!(
        out.scenarios_baseline_only,
        vec![ScenarioId::new("s_only_b").expect("static scenario id is valid")]
    );
    assert_eq!(
        out.scenarios_candidate_only,
        vec![ScenarioId::new("s_only_c").expect("static scenario id is valid")]
    );
    assert!(out.scenarios.is_empty());
}

#[test]
fn new_and_removed_subjects_surfaced() {
    let agent_a = card_ref(CardKind::Agent, "agent_a");
    let agent_b = card_ref(CardKind::Agent, "agent_b");
    let baseline = build_run(
        "b",
        vec![(agent_a.clone(), vec![("t", 5, 5)])],
        "s",
        Some(true),
    );
    let candidate = build_run(
        "c",
        vec![(agent_b.clone(), vec![("t", 5, 5)])],
        "s",
        Some(true),
    );

    let out = compare(&baseline, &candidate, &CompareConfig::default());

    assert_eq!(out.subjects_baseline_only.len(), 1);
    assert_eq!(
        out.subjects_baseline_only[0].subject_key,
        SubjectKey::from_ref(&agent_a)
    );
    assert_eq!(out.subjects_candidate_only.len(), 1);
    assert_eq!(
        out.subjects_candidate_only[0].subject_key,
        SubjectKey::from_ref(&agent_b)
    );
    assert!(out.subjects.is_empty());
}

#[test]
fn empty_baseline_comparison_yields_zero_deltas_and_no_panic() {
    let empty = aggregate_run(AggregationInput {
        identity: RunIdentity {
            run_id: RunId::from_string("empty".to_owned()),
            eval_ref: eval_ref(),
            started_at: Utc::now(),
            ended_at: Utc::now(),
        },
        scenarios: vec![],
        config: ResultsConfig {
            context_capture: None,
            pass_gate: None,
        },
    })
    .expect("aggregation succeeds");
    let subject = card_ref(CardKind::Agent, "agent_a");
    let candidate = build_run("c", vec![(subject, vec![("t", 5, 5)])], "s", Some(true));

    let out = compare(&empty, &candidate, &CompareConfig::default());

    assert!(out.subjects.is_empty());
    assert!(out.scenarios.is_empty());
    assert_eq!(out.subjects_candidate_only.len(), 1);
    assert_eq!(out.scenarios_candidate_only.len(), 1);
    assert_eq!(out.system.flag, ChangeFlag::Improved);
}

#[test]
fn threshold_exact_boundary_does_not_flag() {
    let subject = card_ref(CardKind::Agent, "agent_a");
    let baseline = build_run(
        "b",
        vec![(subject.clone(), vec![("t", 20, 0)])],
        "s",
        Some(true),
    );
    let candidate = build_run(
        "c",
        vec![(subject.clone(), vec![("t", 19, 1)])],
        "s",
        Some(true),
    );

    let out = compare(&baseline, &candidate, &CompareConfig::default());
    let key = SubjectKey::from_ref(&subject);

    assert_eq!(out.subjects[&key].flag, ChangeFlag::Unchanged);
    assert_eq!(out.system.flag, ChangeFlag::Unchanged);
}

#[test]
fn zero_threshold_flags_any_movement() {
    let subject = card_ref(CardKind::Agent, "agent_a");
    let baseline = build_run(
        "b",
        vec![(subject.clone(), vec![("t", 10, 0)])],
        "s",
        Some(true),
    );
    let candidate = build_run(
        "c",
        vec![(subject.clone(), vec![("t", 9, 1)])],
        "s",
        Some(true),
    );
    let config = CompareConfig {
        regression_threshold: 0.0,
        per_task_pass_cut: 0.5,
    };

    let out = compare(&baseline, &candidate, &config);
    let key = SubjectKey::from_ref(&subject);

    assert_eq!(out.subjects[&key].flag, ChangeFlag::Regressed);
}

#[test]
fn huge_threshold_never_flags() {
    let subject = card_ref(CardKind::Agent, "agent_a");
    let baseline = build_run(
        "b",
        vec![(subject.clone(), vec![("t", 10, 0)])],
        "s",
        Some(true),
    );
    let candidate = build_run(
        "c",
        vec![(subject.clone(), vec![("t", 0, 10)])],
        "s",
        Some(false),
    );
    let config = CompareConfig {
        regression_threshold: 10.0,
        per_task_pass_cut: 0.5,
    };

    let out = compare(&baseline, &candidate, &config);
    let key = SubjectKey::from_ref(&subject);

    assert_eq!(out.subjects[&key].flag, ChangeFlag::Unchanged);
    assert_eq!(out.system.flag, ChangeFlag::Unchanged);
}

#[test]
fn comparison_save_load_round_trip() {
    let subject = card_ref(CardKind::Agent, "agent_a");
    let baseline = build_run(
        "b",
        vec![(subject.clone(), vec![("t", 10, 0)])],
        "s",
        Some(true),
    );
    let candidate = build_run("c", vec![(subject, vec![("t", 5, 5)])], "s", Some(true));
    let out = compare(&baseline, &candidate, &CompareConfig::default());

    let raw = out.to_json().expect("comparison serializes");
    let back = vala_eval::ComparisonResults::from_json(&raw).expect("comparison deserializes");
    assert_eq!(out, back);

    let mut tmp: PathBuf = std::env::temp_dir();
    tmp.push(format!("vala-eval-compare-{}.json", std::process::id()));
    out.save(&tmp).expect("comparison saves");
    let back = vala_eval::ComparisonResults::load(&tmp).expect("comparison loads");
    std::fs::remove_file(&tmp).expect("temporary comparison file is removed");
    assert_eq!(out, back);
}
