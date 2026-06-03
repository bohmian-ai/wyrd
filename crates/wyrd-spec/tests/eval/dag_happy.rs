use std::collections::BTreeMap;

use wyrd_spec::vala::eval::assertion::AssertionTask;
use wyrd_spec::vala::eval::ids::{JsonPath, TaskId};
use wyrd_spec::vala::eval::operator::ComparisonOperator;
use wyrd_spec::vala::eval::plan::validate_dag;
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
fn linear_three_stages() {
    let mut tasks = BTreeMap::new();
    let (task_id, eval_task) = task("a", &[]);
    tasks.insert(task_id, eval_task);
    let (task_id, eval_task) = task("b", &["a"]);
    tasks.insert(task_id, eval_task);
    let (task_id, eval_task) = task("c", &["b"]);
    tasks.insert(task_id, eval_task);

    let plan = validate_dag(&tasks).unwrap();
    assert_eq!(plan.task_count, 3);
    assert_eq!(plan.stages.len(), 3);
    assert_eq!(plan.stages[0].tasks[0].as_str(), "a");
    assert_eq!(plan.stages[1].tasks[0].as_str(), "b");
    assert_eq!(plan.stages[2].tasks[0].as_str(), "c");
}

#[test]
fn diamond_three_stages_parallel_middle() {
    let mut tasks = BTreeMap::new();
    let (task_id, eval_task) = task("a", &[]);
    tasks.insert(task_id, eval_task);
    let (task_id, eval_task) = task("b", &["a"]);
    tasks.insert(task_id, eval_task);
    let (task_id, eval_task) = task("c", &["a"]);
    tasks.insert(task_id, eval_task);
    let (task_id, eval_task) = task("d", &["b", "c"]);
    tasks.insert(task_id, eval_task);

    let plan = validate_dag(&tasks).unwrap();
    assert_eq!(plan.stages.len(), 3);
    let mid: Vec<_> = plan.stages[1].tasks.iter().map(TaskId::as_str).collect();
    assert_eq!(mid, vec!["b", "c"]);
}

#[test]
fn disconnected_components_share_stage_zero() {
    let mut tasks = BTreeMap::new();
    let (task_id, eval_task) = task("a", &[]);
    tasks.insert(task_id, eval_task);
    let (task_id, eval_task) = task("b", &[]);
    tasks.insert(task_id, eval_task);

    let plan = validate_dag(&tasks).unwrap();
    assert_eq!(plan.stages.len(), 1);
    let stage_zero: Vec<_> = plan.stages[0].tasks.iter().map(TaskId::as_str).collect();
    assert_eq!(stage_zero, vec!["a", "b"]);
}
