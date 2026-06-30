use std::path::{Path, PathBuf};

use serde_json::json;
use tempfile::TempDir;
use vala_eval::{EvalPlanError, load_scenario_collection};
use wyrd_spec::vala::eval::{ComparisonOperator, EvalScenario, ScenarioId, ScenarioTask, TaskId};

fn sid(value: &str) -> ScenarioId {
    ScenarioId::new(value).expect("static scenario id is valid")
}

fn tid(value: &str) -> TaskId {
    TaskId::new(value).expect("static task id is valid")
}

fn scenario(id: &str) -> EvalScenario {
    EvalScenario {
        id: sid(id),
        initial_query: format!("question for {id}"),
        expected_outcome: Some("answer".to_owned()),
        predefined_turns: Vec::new(),
        simulated_user_persona: None,
        termination_signal: None,
        max_turns: 8,
        tasks: vec![ScenarioTask {
            id: tid("contains_answer"),
            operator: ComparisonOperator::Contains,
            expected: json!("answer"),
            condition: None,
        }],
    }
}

fn invalid_scenario(id: &str) -> EvalScenario {
    EvalScenario {
        initial_query: String::new(),
        ..scenario(id)
    }
}

fn write_file(dir: &TempDir, name: &str, body: &str) -> PathBuf {
    let path = dir.path().join(name);
    std::fs::write(&path, body).expect("test fixture writes");
    path
}

fn load(path: &Path) -> Result<vala_eval::EvalScenarioCollection, EvalPlanError> {
    load_scenario_collection(path)
}

#[test]
fn loads_json_collection() {
    let dir = TempDir::new().expect("temp dir creates");
    let body = serde_json::to_string(&vec![scenario("alpha")]).expect("scenario serializes");
    let path = write_file(&dir, "scenarios.json", &body);

    let collection = load(&path).expect("json collection loads");

    assert_eq!(collection.scenarios.len(), 1);
    assert_eq!(collection.scenarios[0].id, sid("alpha"));
}

#[test]
fn loads_yaml_collection() {
    let dir = TempDir::new().expect("temp dir creates");
    let body = serde_yaml::to_string(&vec![scenario("alpha")]).expect("scenario serializes");
    let path = write_file(&dir, "scenarios.yaml", &body);

    let collection = load(&path).expect("yaml collection loads");

    assert_eq!(collection.scenarios.len(), 1);
    assert_eq!(collection.scenarios[0].id, sid("alpha"));
}

#[test]
fn loads_jsonl_collection() {
    let dir = TempDir::new().expect("temp dir creates");
    let first = serde_json::to_string(&scenario("alpha")).expect("scenario serializes");
    let second = serde_json::to_string(&scenario("beta")).expect("scenario serializes");
    let path = write_file(&dir, "scenarios.jsonl", &format!("{first}\n\n{second}\n"));

    let collection = load(&path).expect("jsonl collection loads");

    assert_eq!(collection.scenarios.len(), 2);
    assert_eq!(collection.scenarios[0].id, sid("alpha"));
    assert_eq!(collection.scenarios[1].id, sid("beta"));
}

#[test]
fn missing_file_errors_typed() {
    let dir = TempDir::new().expect("temp dir creates");
    let path = dir.path().join("missing.json");

    let error = load(&path).expect_err("missing file fails");

    assert!(matches!(error, EvalPlanError::ScenarioFileMissing { .. }));
}

#[test]
fn malformed_json_errors_typed() {
    let dir = TempDir::new().expect("temp dir creates");
    let path = write_file(&dir, "scenarios.json", "{");

    let error = load(&path).expect_err("malformed json fails");

    assert!(matches!(
        error,
        EvalPlanError::ScenarioFileUnreadable { line: None, .. }
    ));
}

#[test]
fn malformed_jsonl_carries_line_number() {
    let dir = TempDir::new().expect("temp dir creates");
    let first = serde_json::to_string(&scenario("alpha")).expect("scenario serializes");
    let path = write_file(&dir, "scenarios.jsonl", &format!("{first}\n{{\n"));

    let error = load(&path).expect_err("malformed jsonl fails");

    assert!(matches!(
        error,
        EvalPlanError::ScenarioFileUnreadable { line: Some(2), .. }
    ));
}

#[test]
fn unknown_extension_errors_typed() {
    let dir = TempDir::new().expect("temp dir creates");
    let path = write_file(&dir, "scenarios.txt", "[]");

    let error = load(&path).expect_err("unknown extension fails");

    assert!(matches!(
        error,
        EvalPlanError::ScenarioFileExtUnsupported { .. }
    ));
}

#[test]
fn duplicate_scenario_id_errors_typed_before_generic_validator() {
    let dir = TempDir::new().expect("temp dir creates");
    let body = serde_json::to_string(&vec![invalid_scenario("alpha"), invalid_scenario("alpha")])
        .expect("scenario serializes");
    let path = write_file(&dir, "scenarios.json", &body);

    let error = load(&path).expect_err("duplicate id fails first");

    assert!(matches!(
        error,
        EvalPlanError::ScenarioCollectionDuplicateId {
            ref scenario_id,
            ..
        } if scenario_id == "alpha"
    ));
}

#[test]
fn empty_collection_errors_typed() {
    let dir = TempDir::new().expect("temp dir creates");
    let path = write_file(&dir, "scenarios.json", "[]");

    let error = load(&path).expect_err("empty collection fails");

    assert!(matches!(
        error,
        EvalPlanError::ScenarioCollectionEmpty { .. }
    ));
}
