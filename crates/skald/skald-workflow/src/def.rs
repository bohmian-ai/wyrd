//! Serializable workflow definitions.

use std::collections::{HashMap, HashSet, VecDeque};

use serde::{Deserialize, Serialize};
use skald_agent::RunConfig;
use skald_spec::Prompt;

use crate::error::{WorkflowError, WorkflowResult};

/// Declarative form of a workflow.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkflowDef {
    /// Stable workflow id.
    pub id: String,
    /// Human-readable workflow name.
    pub name: String,
    /// Declared agents that later bind to live providers.
    pub agents: Vec<WorkflowAgent>,
    /// Tasks in declaration order.
    pub tasks: Vec<TaskDef>,
}

/// Declarative form of one task.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TaskDef {
    /// Stable task id; unique inside the workflow.
    pub id: String,
    /// Agent id that will run this task.
    pub agent_id: String,
    /// Native prompt the task runs.
    pub prompt: Prompt,
    /// Task ids this task depends on. Empty means ready immediately.
    #[serde(default)]
    pub dependencies: Vec<String>,
    /// Maximum execution retries on validation or provider failure.
    #[serde(default = "default_max_retries")]
    pub max_retries: u32,
}

/// Workflow-local declaration for an agent runtime.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkflowAgent {
    /// Stable agent id referenced by tasks.
    pub id: String,
    /// Resolved native prompt carried by the runtime agent.
    pub prompt: Prompt,
    /// Agent loop configuration.
    #[serde(default)]
    pub run_config: RunConfig,
}

/// Default maximum retry count for a task.
pub const fn default_max_retries() -> u32 {
    3
}

impl WorkflowDef {
    /// Validate the task graph as pure data before live state is built.
    ///
    /// # Errors
    ///
    /// Returns `WorkflowError` for duplicate task ids, self-dependencies,
    /// missing dependencies, dependency cycles, or prompts that cannot drive
    /// the agent loop.
    pub fn validate_graph(&self) -> WorkflowResult<()> {
        let ids = self.validate_ids_and_self_deps()?;
        self.validate_dependencies(&ids)?;
        self.validate_prompt_loop_requests()?;
        self.validate_acyclic()
    }

    fn validate_ids_and_self_deps(&self) -> WorkflowResult<HashSet<&str>> {
        let mut ids = HashSet::with_capacity(self.tasks.len());
        for task in &self.tasks {
            if !ids.insert(task.id.as_str()) {
                return Err(WorkflowError::TaskAlreadyExists(task.id.clone()));
            }
            if task.dependencies.iter().any(|dep| dep == &task.id) {
                return Err(WorkflowError::TaskDependsOnItself(task.id.clone()));
            }
        }
        Ok(ids)
    }

    fn validate_dependencies(&self, ids: &HashSet<&str>) -> WorkflowResult<()> {
        for task in &self.tasks {
            for dep in &task.dependencies {
                if !ids.contains(dep.as_str()) {
                    return Err(WorkflowError::DependencyNotFound(dep.clone()));
                }
            }
        }
        Ok(())
    }

    fn validate_prompt_loop_requests(&self) -> WorkflowResult<()> {
        for task in &self.tasks {
            skald_agent::request_builder::validate_prompt_loop_request(
                &task.id,
                &task.prompt.request,
            )?;
        }
        Ok(())
    }

    fn validate_acyclic(&self) -> WorkflowResult<()> {
        let mut in_degree: HashMap<&str, usize> = self
            .tasks
            .iter()
            .map(|task| (task.id.as_str(), task.dependencies.len()))
            .collect();
        let mut adjacent: HashMap<&str, Vec<&str>> = HashMap::new();
        for task in &self.tasks {
            for dep in &task.dependencies {
                adjacent
                    .entry(dep.as_str())
                    .or_default()
                    .push(task.id.as_str());
            }
        }

        let mut ready: VecDeque<&str> = in_degree
            .iter()
            .filter_map(|(id, degree)| (*degree == 0).then_some(*id))
            .collect();
        let mut visited = 0usize;
        while let Some(id) = ready.pop_front() {
            visited += 1;
            if let Some(successors) = adjacent.get(id) {
                for successor in successors {
                    if let Some(entry) = in_degree.get_mut(successor) {
                        *entry = entry.saturating_sub(1);
                        if *entry == 0 {
                            ready.push_back(*successor);
                        }
                    }
                }
            }
        }

        if visited != self.tasks.len() {
            let witness = cycle_witness(&in_degree);
            return Err(WorkflowError::Cycle(witness));
        }
        Ok(())
    }
}

fn cycle_witness(in_degree: &HashMap<&str, usize>) -> String {
    for (id, degree) in in_degree {
        if *degree > 0 {
            return (*id).to_owned();
        }
    }
    String::new()
}
