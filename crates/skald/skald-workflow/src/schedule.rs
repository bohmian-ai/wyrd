//! Workflow scheduling helpers.

use std::collections::{HashMap, HashSet};

use crate::error::{WorkflowError, WorkflowResult};
use crate::tasklist::TaskList;

/// Build a depth-based execution plan for the workflow task graph.
///
/// Step `0` contains tasks with no dependencies. Step `N` contains tasks whose
/// dependencies have completed by step `N - 1`.
///
/// # Errors
///
/// Returns `WorkflowError::Lock` when task lock acquisition fails.
pub fn execution_plan(task_list: &TaskList) -> WorkflowResult<HashMap<i32, HashSet<String>>> {
    let mut depth: HashMap<String, i32> = HashMap::new();
    for id in task_list.execution_order() {
        let Some(task) = task_list.get_task(id) else {
            continue;
        };
        let guard = task.read().map_err(|_| WorkflowError::Lock)?;
        let task_depth = guard
            .dependencies()
            .iter()
            .map(|dep| depth.get(dep).copied().unwrap_or(0))
            .max()
            .map_or(0, |dep_depth| dep_depth + 1);
        depth.insert(id.clone(), task_depth);
    }

    let mut plan: HashMap<i32, HashSet<String>> = HashMap::new();
    for (id, step) in depth {
        plan.entry(step).or_default().insert(id);
    }
    Ok(plan)
}
