use std::collections::BTreeMap;

use wyrd_spec::vala::eval::assertion::AssertionTask;
use wyrd_spec::vala::eval::ids::{JsonPath, TaskId};
use wyrd_spec::vala::eval::operator::ComparisonOperator;
use wyrd_spec::vala::eval::plan::{DagError, validate_dag};
use wyrd_spec::vala::eval::task::EvalTask;

#[test]
fn task_depending_on_itself_rejected() {
    let mut tasks = BTreeMap::new();
    let id = TaskId::new("loop").unwrap();
    tasks.insert(
        id.clone(),
        EvalTask::Assertion(AssertionTask {
            id: id.clone(),
            context_path: Some(JsonPath::new("$.x").unwrap()),
            item_context_path: None,
            operator: ComparisonOperator::IsNotNull,
            expected: serde_json::Value::Null,
            depends_on: vec![id],
            condition: None,
        }),
    );

    let err = validate_dag(&tasks).unwrap_err();
    match err {
        DagError::SelfLoop { task } => assert_eq!(task.as_str(), "loop"),
        other => panic!("expected SelfLoop, got {other:?}"),
    }
}
