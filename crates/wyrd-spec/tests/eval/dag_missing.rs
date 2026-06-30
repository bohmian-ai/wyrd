use std::collections::BTreeMap;

use wyrd_spec::vala::eval::assertion::AssertionTask;
use wyrd_spec::vala::eval::ids::{JsonPath, TaskId};
use wyrd_spec::vala::eval::operator::ComparisonOperator;
use wyrd_spec::vala::eval::plan::{DagError, validate_dag};
use wyrd_spec::vala::eval::task::EvalTask;

#[test]
fn unknown_dependency_rejected() {
    let mut tasks = BTreeMap::new();
    let id = TaskId::new("a").unwrap();
    let missing = TaskId::new("ghost").unwrap();
    tasks.insert(
        id.clone(),
        EvalTask::Assertion(AssertionTask {
            id: id.clone(),
            context_path: Some(JsonPath::new("$.x").unwrap()),
            item_context_path: None,
            operator: ComparisonOperator::IsNotNull,
            expected: serde_json::Value::Null,
            depends_on: vec![missing.clone()],
            condition: None,
        }),
    );

    let err = validate_dag(&tasks).unwrap_err();
    match err {
        DagError::MissingDependency { task, dep } => {
            assert_eq!(task.as_str(), "a");
            assert_eq!(dep.as_str(), "ghost");
        }
        other => panic!("expected MissingDependency, got {other:?}"),
    }
}
