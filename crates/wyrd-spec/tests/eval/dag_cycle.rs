use std::collections::BTreeMap;

use wyrd_spec::vala::eval::assertion::AssertionTask;
use wyrd_spec::vala::eval::ids::{JsonPath, TaskId};
use wyrd_spec::vala::eval::operator::ComparisonOperator;
use wyrd_spec::vala::eval::plan::{DagError, validate_dag};
use wyrd_spec::vala::eval::task::EvalTask;

fn task(id: &str, deps: &[&str]) -> (TaskId, EvalTask) {
    let task_id = TaskId::new(id).unwrap();
    let task = EvalTask::Assertion(AssertionTask {
        id: task_id.clone(),
        context_path: Some(JsonPath::new("$.x").unwrap()),
        item_context_path: None,
        operator: ComparisonOperator::IsNotNull,
        expected: serde_json::Value::Null,
        depends_on: deps.iter().map(|dep| TaskId::new(*dep).unwrap()).collect(),
        condition: None,
    });
    (task_id, task)
}

#[test]
fn triangle_cycle_returns_witness() {
    let mut tasks = BTreeMap::new();
    let (task_id, eval_task) = task("a", &["c"]);
    tasks.insert(task_id, eval_task);
    let (task_id, eval_task) = task("b", &["a"]);
    tasks.insert(task_id, eval_task);
    let (task_id, eval_task) = task("c", &["b"]);
    tasks.insert(task_id, eval_task);

    let err = validate_dag(&tasks).unwrap_err();
    match err {
        DagError::Cycle { cycle } => {
            let names: Vec<&str> = cycle.iter().map(TaskId::as_str).collect();
            assert_eq!(names.first(), Some(&"a"));
            assert_eq!(names.last(), Some(&"a"));
            assert_eq!(cycle.len() - 1, 3);
        }
        other => panic!("expected Cycle, got {other:?}"),
    }
}

#[test]
fn smallest_cycle_witness_picks_3_over_5() {
    let mut tasks = BTreeMap::new();
    for (id, dep) in [
        ("a", "c"),
        ("b", "a"),
        ("c", "b"),
        ("v", "w"),
        ("w", "z"),
        ("x", "v"),
        ("y", "x"),
        ("z", "y"),
    ] {
        let (task_id, eval_task) = task(id, &[dep]);
        tasks.insert(task_id, eval_task);
    }

    let err = validate_dag(&tasks).unwrap_err();
    match err {
        DagError::Cycle { cycle } => {
            assert_eq!(cycle.len() - 1, 3, "smallest cycle is 3, got {cycle:?}");
        }
        other => panic!("expected Cycle, got {other:?}"),
    }
}
