use wyrd_spec::vala::eval::agent::AgentAssertionTask;
use wyrd_spec::vala::eval::ids::{JsonPath, TaskId};
use wyrd_spec::vala::eval::operator::ComparisonOperator;

#[test]
fn agent_assertion_round_trip() {
    let task = AgentAssertionTask {
        id: TaskId::new("agent_one").unwrap(),
        workflow_field_path: JsonPath::new("$.tool_calls[0].name").unwrap(),
        operator: ComparisonOperator::Equals,
        expected: serde_json::json!("search"),
        depends_on: vec![],
        condition: None,
    };

    let serialized = serde_json::to_string(&task).unwrap();
    let deserialized: AgentAssertionTask = serde_json::from_str(&serialized).unwrap();
    assert_eq!(task, deserialized);
}
