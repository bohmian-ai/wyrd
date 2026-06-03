use wyrd_spec::vala::eval::condition::{ConditionCombinator, EvalCondition, MAX_CONDITION_DEPTH};
use wyrd_spec::vala::eval::ids::JsonPath;
use wyrd_spec::vala::eval::operator::ComparisonOperator;

fn simple_condition() -> EvalCondition {
    EvalCondition {
        path: JsonPath::new("$.response").unwrap(),
        operator: ComparisonOperator::Contains,
        expected: serde_json::json!("Paris"),
        combinator: None,
        subsequent: None,
    }
}

#[test]
fn single_node_serde_round_trip() {
    let condition = simple_condition();
    let serialized = serde_json::to_string(&condition).unwrap();
    let deserialized: EvalCondition = serde_json::from_str(&serialized).unwrap();
    assert_eq!(condition, deserialized);
}

#[test]
fn single_node_validates() {
    simple_condition().validate().unwrap();
}

#[test]
fn chain_two_nodes_and() {
    let subsequent = simple_condition();
    let condition = EvalCondition {
        path: JsonPath::new("$.score").unwrap(),
        operator: ComparisonOperator::GreaterThan,
        expected: serde_json::json!(0.5),
        combinator: Some(ConditionCombinator::And),
        subsequent: Some(Box::new(subsequent)),
    };
    assert_eq!(condition.depth(), 2);
    condition.validate().unwrap();
}

#[test]
fn combinator_without_subsequent_rejected() {
    let condition = EvalCondition {
        path: JsonPath::new("$.x").unwrap(),
        operator: ComparisonOperator::Equals,
        expected: serde_json::json!(0),
        combinator: Some(ConditionCombinator::And),
        subsequent: None,
    };
    assert!(condition.validate().is_err());
}

#[test]
fn subsequent_without_combinator_rejected() {
    let condition = EvalCondition {
        path: JsonPath::new("$.x").unwrap(),
        operator: ComparisonOperator::Equals,
        expected: serde_json::json!(0),
        combinator: None,
        subsequent: Some(Box::new(simple_condition())),
    };
    assert!(condition.validate().is_err());
}

#[test]
fn chain_depth_exceeds_max_rejected() {
    let mut current = simple_condition();
    for _ in 0..MAX_CONDITION_DEPTH {
        current = EvalCondition {
            path: JsonPath::new("$.x").unwrap(),
            operator: ComparisonOperator::Equals,
            expected: serde_json::json!(0),
            combinator: Some(ConditionCombinator::And),
            subsequent: Some(Box::new(current)),
        };
    }

    assert!(current.depth() > MAX_CONDITION_DEPTH);
    assert!(current.validate().is_err());
}

#[test]
fn rejects_unknown_field() {
    let json = r#"{
        "path": "$.x", "operator": {"operator":"equals"},
        "expected": 0, "noise": true
    }"#;
    let result: Result<EvalCondition, _> = serde_json::from_str(json);
    assert!(result.is_err());
}
