use wyrd_spec::vala::eval::assertion::AssertionTask;
use wyrd_spec::vala::eval::ids::{JsonPath, TaskId};
use wyrd_spec::vala::eval::operator::ComparisonOperator;

#[test]
fn assertion_round_trip_minimal() {
    let task = AssertionTask {
        id: TaskId::new("step_one").unwrap(),
        context_path: Some(JsonPath::new("$.response").unwrap()),
        item_context_path: None,
        operator: ComparisonOperator::Contains,
        expected: serde_json::json!("hello"),
        depends_on: vec![],
        condition: None,
    };

    let serialized = serde_json::to_string(&task).unwrap();
    let deserialized: AssertionTask = serde_json::from_str(&serialized).unwrap();
    assert_eq!(task, deserialized);
}

#[test]
fn assertion_round_trip_with_depends_on_and_condition() {
    use wyrd_spec::vala::eval::condition::EvalCondition;

    let task = AssertionTask {
        id: TaskId::new("step_two").unwrap(),
        context_path: None,
        item_context_path: Some(JsonPath::new("$.docs").unwrap()),
        operator: ComparisonOperator::IsNonEmpty,
        expected: serde_json::Value::Null,
        depends_on: vec![TaskId::new("step_one").unwrap()],
        condition: Some(EvalCondition {
            path: JsonPath::new("$.flag").unwrap(),
            operator: ComparisonOperator::IsTruthy,
            expected: serde_json::Value::Null,
            combinator: None,
            subsequent: None,
        }),
    };

    let serialized = serde_json::to_string(&task).unwrap();
    let deserialized: AssertionTask = serde_json::from_str(&serialized).unwrap();
    assert_eq!(task, deserialized);
}

#[test]
fn assertion_rejects_unknown_field() {
    let json = r#"{
        "id":"a","operator":"equals","expected":0,
        "context_path":"$.x","extra":"nope"
    }"#;
    let result: Result<AssertionTask, _> = serde_json::from_str(json);
    assert!(result.is_err());
}
