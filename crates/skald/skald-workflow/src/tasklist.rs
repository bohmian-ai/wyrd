//! Runtime task graph for workflow execution.

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, RwLock};

use skald_spec::Prompt;

use crate::def::TaskDef;
use crate::error::{WorkflowError, WorkflowResult};
use crate::task::{Task, TaskStatus};

/// Shared live task handle used by the level-parallel executor.
pub type SharedTask = Arc<RwLock<Task>>;

/// Runtime task graph for one workflow.
pub struct TaskList {
    tasks: HashMap<String, SharedTask>,
    execution_order: Vec<String>,
}

impl TaskList {
    /// Build an empty task list.
    pub fn new() -> Self {
        Self {
            tasks: HashMap::new(),
            execution_order: Vec::new(),
        }
    }

    /// Insert a task and rebuild the cached topological order.
    ///
    /// # Errors
    ///
    /// Returns `WorkflowError` for duplicate ids, self-dependencies, missing
    /// dependencies, invalid task prompts, output-schema compilation failures,
    /// lock poisoning, or a cycle in the resulting graph.
    pub fn add_task(&mut self, def: TaskDef) -> WorkflowResult<()> {
        if self.tasks.contains_key(&def.id) {
            return Err(WorkflowError::TaskAlreadyExists(def.id));
        }
        if def.dependencies.iter().any(|dep| dep == &def.id) {
            return Err(WorkflowError::TaskDependsOnItself(def.id));
        }
        for dep in &def.dependencies {
            if !self.tasks.contains_key(dep) {
                return Err(WorkflowError::DependencyNotFound(dep.clone()));
            }
        }

        let task = Task::build(def)?;
        let id = task.id.clone();
        let previous_order = self.execution_order.clone();
        self.tasks.insert(id.clone(), Arc::new(RwLock::new(task)));
        if let Err(err) = self.rebuild_execution_order() {
            self.tasks.remove(&id);
            self.execution_order = previous_order;
            return Err(err);
        }
        Ok(())
    }

    /// Recompute the cached topological order with deterministic tie-breaking.
    ///
    /// # Errors
    ///
    /// Returns `WorkflowError::Lock` when task lock acquisition fails, or
    /// `WorkflowError::Cycle` when the graph is cyclic.
    pub fn rebuild_execution_order(&mut self) -> WorkflowResult<()> {
        self.execution_order = self.topological_sort()?;
        Ok(())
    }

    /// Return a deterministic topological ordering of the current task graph.
    ///
    /// # Errors
    ///
    /// Returns `WorkflowError::Lock` when task lock acquisition fails, or
    /// `WorkflowError::Cycle` when the graph is cyclic.
    pub fn topological_sort(&self) -> WorkflowResult<Vec<String>> {
        let mut in_degree: HashMap<String, usize> = HashMap::new();
        let mut adjacent: HashMap<String, Vec<String>> = HashMap::new();

        for (id, task) in &self.tasks {
            in_degree.entry(id.clone()).or_insert(0);
            let guard = task.read().map_err(|_| WorkflowError::Lock)?;
            for dep in guard.dependencies() {
                adjacent.entry(dep.clone()).or_default().push(id.clone());
                *in_degree.entry(id.clone()).or_insert(0) += 1;
            }
        }

        for successors in adjacent.values_mut() {
            successors.sort();
        }

        let mut ready_vec: Vec<String> = in_degree
            .iter()
            .filter_map(|(id, degree)| (*degree == 0).then_some(id.clone()))
            .collect();
        ready_vec.sort();
        let mut ready: VecDeque<String> = ready_vec.into();
        let mut order = Vec::with_capacity(self.tasks.len());

        while let Some(id) = ready.pop_front() {
            order.push(id.clone());
            let Some(successors) = adjacent.get(&id) else {
                continue;
            };
            let mut newly_ready = Vec::new();
            for successor in successors {
                if let Some(degree) = in_degree.get_mut(successor) {
                    *degree = degree.saturating_sub(1);
                    if *degree == 0 {
                        newly_ready.push(successor.clone());
                    }
                }
            }
            newly_ready.sort();
            for id in newly_ready {
                ready.push_back(id);
            }
        }

        if order.len() != self.tasks.len() {
            let remaining = in_degree
                .iter()
                .find_map(|(id, degree)| (*degree > 0).then_some(id.clone()))
                .unwrap_or_default();
            return Err(WorkflowError::Cycle(remaining));
        }

        Ok(order)
    }

    /// Return pending tasks whose dependencies are completed, in execution order.
    ///
    /// # Errors
    ///
    /// Returns `WorkflowError::Lock` when task lock acquisition fails.
    pub fn get_ready_tasks(&self) -> WorkflowResult<Vec<SharedTask>> {
        let mut ready = Vec::new();
        for id in &self.execution_order {
            let Some(task) = self.tasks.get(id) else {
                continue;
            };
            let guard = task.read().map_err(|_| WorkflowError::Lock)?;
            if guard.status != TaskStatus::Pending {
                continue;
            }

            let mut all_done = true;
            for dep in guard.dependencies() {
                let dep_task = self
                    .tasks
                    .get(dep)
                    .ok_or_else(|| WorkflowError::DependencyNotFound(dep.clone()))?;
                let dep_guard = dep_task.read().map_err(|_| WorkflowError::Lock)?;
                if dep_guard.status != TaskStatus::Completed {
                    all_done = false;
                    break;
                }
            }
            if all_done {
                ready.push(Arc::clone(task));
            }
        }
        Ok(ready)
    }

    /// Return true when every task is completed.
    ///
    /// # Errors
    ///
    /// Returns `WorkflowError::Lock` when task lock acquisition fails.
    pub fn is_complete(&self) -> WorkflowResult<bool> {
        for task in self.tasks.values() {
            let guard = task.read().map_err(|_| WorkflowError::Lock)?;
            if guard.status != TaskStatus::Completed {
                return Ok(false);
            }
        }
        Ok(true)
    }

    /// Return ids for tasks not yet completed, in execution order.
    ///
    /// # Errors
    ///
    /// Returns `WorkflowError::Lock` when task lock acquisition fails.
    pub fn pending_ids(&self) -> WorkflowResult<Vec<String>> {
        let mut pending = Vec::new();
        for id in &self.execution_order {
            let Some(task) = self.tasks.get(id) else {
                continue;
            };
            let guard = task.read().map_err(|_| WorkflowError::Lock)?;
            if guard.status != TaskStatus::Completed {
                pending.push(id.clone());
            }
        }
        Ok(pending)
    }

    /// Reset failed tasks that still have retry budget and return exhausted ids.
    ///
    /// # Errors
    ///
    /// Returns `WorkflowError::Lock` when task lock acquisition fails.
    pub fn reset_failed_tasks(&self) -> WorkflowResult<Vec<String>> {
        let mut exhausted = Vec::new();
        for task in self.tasks.values() {
            let mut guard = task.write().map_err(|_| WorkflowError::Lock)?;
            if guard.status == TaskStatus::Failed {
                if guard.retry_count >= guard.max_retries {
                    exhausted.push(guard.id.clone());
                } else {
                    guard.status = TaskStatus::Pending;
                    guard.retry_count += 1;
                }
            }
        }
        Ok(exhausted)
    }

    /// Last id in topological order, if any.
    pub fn get_last_task_id(&self) -> Option<&str> {
        self.execution_order.last().map(String::as_str)
    }

    /// Return the live task for an id.
    pub fn get_task(&self, id: &str) -> Option<SharedTask> {
        self.tasks.get(id).cloned()
    }

    /// Iterate live tasks in topological order.
    pub fn iter_in_order(&self) -> impl Iterator<Item = (&String, &SharedTask)> {
        self.execution_order
            .iter()
            .filter_map(move |id| self.tasks.get(id).map(|task| (id, task)))
    }

    /// Number of tasks in the graph.
    pub fn len(&self) -> usize {
        self.tasks.len()
    }

    /// True when the graph has no tasks.
    pub fn is_empty(&self) -> bool {
        self.tasks.is_empty()
    }

    /// Borrow the cached topological order.
    pub fn execution_order(&self) -> &[String] {
        &self.execution_order
    }

    /// Return a cloned prompt for diagnostics.
    ///
    /// # Errors
    ///
    /// Returns `WorkflowError::Lock` when task lock acquisition fails.
    pub fn prompt_for(&self, id: &str) -> WorkflowResult<Option<Prompt>> {
        let Some(task) = self.tasks.get(id) else {
            return Ok(None);
        };
        let guard = task.read().map_err(|_| WorkflowError::Lock)?;
        Ok(Some(guard.prompt.clone()))
    }
}

impl Default for TaskList {
    fn default() -> Self {
        Self::new()
    }
}
