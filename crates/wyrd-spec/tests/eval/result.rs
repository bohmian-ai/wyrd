use chrono::{TimeZone, Utc};
use wyrd_spec::vala::eval::ids::TaskId;
use wyrd_spec::vala::eval::operator::ComparisonOperator;
use wyrd_spec::vala::eval::result::AssertionResult;

#[test]
fn assertion_result_round_trip_minimal() {
    let r = AssertionResult {
        task_id: TaskId::new("a").unwrap(),
        passed: true,
        actual: serde_json::json!("hello"),
        expected: serde_json::json!("hello"),
        operator: ComparisonOperator::Equals,
        message: None,
        stage: 0,
        started_at: Utc.timestamp_opt(1_700_000_000, 0).unwrap(),
        duration_ms: 42,
    };
    let s = serde_json::to_string(&r).unwrap();
    let back: AssertionResult = serde_json::from_str(&s).unwrap();
    assert_eq!(r, back);
}

#[test]
fn assertion_result_with_message() {
    let r = AssertionResult {
        task_id: TaskId::new("a").unwrap(),
        passed: false,
        actual: serde_json::json!("hi"),
        expected: serde_json::json!("hello"),
        operator: ComparisonOperator::Equals,
        message: Some("equality mismatch at byte 1".into()),
        stage: 0,
        started_at: Utc.timestamp_opt(0, 0).unwrap(),
        duration_ms: 0,
    };
    let s = serde_json::to_string(&r).unwrap();
    let back: AssertionResult = serde_json::from_str(&s).unwrap();
    assert_eq!(r, back);
}

#[test]
fn assertion_result_rejects_unknown_field() {
    let json = r#"{
        "task_id":"a","passed":true,"actual":0,"expected":0,
        "operator":"equals",
        "stage":0,"started_at":"1970-01-01T00:00:00Z","duration_ms":0,
        "extra":"nope"
    }"#;
    let r: Result<AssertionResult, _> = serde_json::from_str(json);
    assert!(r.is_err());
}
