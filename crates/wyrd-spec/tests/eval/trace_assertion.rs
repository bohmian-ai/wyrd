use wyrd_spec::vala::eval::ids::{JsonPath, TaskId};
use wyrd_spec::vala::eval::operator::ComparisonOperator;
use wyrd_spec::vala::eval::trace::TraceAssertionTask;

#[test]
fn trace_assertion_round_trip() {
    let task = TraceAssertionTask {
        id: TaskId::new("trace_one").unwrap(),
        span_selector: JsonPath::new("$.spans[?(@.name=='retrieve')]").unwrap(),
        operator: ComparisonOperator::IsNonEmpty,
        expected: serde_json::Value::Null,
        depends_on: vec![],
        condition: None,
    };

    let serialized = serde_json::to_string(&task).unwrap();
    let deserialized: TraceAssertionTask = serde_json::from_str(&serialized).unwrap();
    assert_eq!(task, deserialized);
}
