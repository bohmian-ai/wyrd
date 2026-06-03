use wyrd_spec::vala::eval::condition::{ConditionCombinator, EvalCondition};
use wyrd_spec::vala::eval::ids::{JsonPath, ScenarioId, TaskId};
use wyrd_spec::vala::eval::operator::ComparisonOperator;
use wyrd_spec::vala::eval::scenario::{
    EvalScenario, EvalScenarioCollection, MAX_TURNS_HARD_CAP, ScenarioTask,
};

fn task(id: &str) -> ScenarioTask {
    ScenarioTask {
        id: TaskId::new(id).unwrap(),
        operator: ComparisonOperator::Contains,
        expected: serde_json::json!("ok"),
        condition: None,
    }
}

fn scenario(id: &str) -> EvalScenario {
    EvalScenario {
        id: ScenarioId::new(id).unwrap(),
        initial_query: "Hello".into(),
        expected_outcome: Some("World".into()),
        predefined_turns: vec![],
        simulated_user_persona: None,
        termination_signal: None,
        max_turns: 8,
        tasks: vec![task("t1")],
    }
}

#[test]
fn scenario_default_max_turns_is_eight() {
    let json = r#"{
        "id": "s1",
        "initial_query": "Hello",
        "tasks": []
    }"#;
    let s: EvalScenario = serde_json::from_str(json).unwrap();
    assert_eq!(s.max_turns, 8);
}

#[test]
fn scenario_round_trip() {
    let s = scenario("s1");
    let json = serde_json::to_string(&s).unwrap();
    let back: EvalScenario = serde_json::from_str(&json).unwrap();
    assert_eq!(s, back);
}

#[test]
fn scenario_validates_happy() {
    scenario("s1").validate().unwrap();
}

#[test]
fn scenario_id_constructor_rejects_empty() {
    assert!(ScenarioId::new("").is_err());
}

#[test]
fn scenario_rejects_empty_initial_query() {
    let mut s = scenario("s1");
    s.initial_query = String::new();
    assert!(s.validate().is_err());
}

#[test]
fn scenario_rejects_zero_max_turns() {
    let mut s = scenario("s1");
    s.max_turns = 0;
    assert!(s.validate().is_err());
}

#[test]
fn scenario_rejects_overlarge_max_turns() {
    let mut s = scenario("s1");
    s.max_turns = MAX_TURNS_HARD_CAP + 1;
    assert!(s.validate().is_err());
}

#[test]
fn scenario_rejects_duplicate_task_ids() {
    let mut s = scenario("s1");
    s.tasks.push(task("t1"));
    assert!(s.validate().is_err());
}

#[test]
fn scenario_propagates_bad_condition() {
    let mut s = scenario("s1");
    s.tasks[0].condition = Some(EvalCondition {
        path: JsonPath::new("$.x").unwrap(),
        operator: ComparisonOperator::Equals,
        expected: serde_json::Value::Null,
        combinator: Some(ConditionCombinator::And),
        subsequent: None,
    });
    assert!(s.validate().is_err());
}

#[test]
fn collection_round_trip() {
    let c = EvalScenarioCollection {
        collection_id: "qa-v1".into(),
        scenarios: vec![scenario("s1"), scenario("s2")],
    };
    let json = serde_json::to_string(&c).unwrap();
    let back: EvalScenarioCollection = serde_json::from_str(&json).unwrap();
    assert_eq!(c, back);
}

#[test]
fn collection_validates_happy() {
    EvalScenarioCollection {
        collection_id: "qa-v1".into(),
        scenarios: vec![scenario("s1"), scenario("s2")],
    }
    .validate()
    .unwrap();
}

#[test]
fn collection_rejects_duplicate_scenario_ids() {
    let c = EvalScenarioCollection {
        collection_id: "qa-v1".into(),
        scenarios: vec![scenario("s1"), scenario("s1")],
    };
    assert!(c.validate().is_err());
}

#[test]
fn collection_rejects_empty_collection_id() {
    let c = EvalScenarioCollection {
        collection_id: String::new(),
        scenarios: vec![scenario("s1")],
    };
    assert!(c.validate().is_err());
}

#[test]
fn collection_rejects_unknown_field() {
    let json = r#"{
        "collection_id":"c", "scenarios":[], "extra": 1
    }"#;
    let r: Result<EvalScenarioCollection, _> = serde_json::from_str(json);
    assert!(r.is_err());
}
