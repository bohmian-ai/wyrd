//! Live workflow execution.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use skald_agent::{Agent, Observer, ToolRegistry};
use skald_runtime::ProviderRegistry;

use crate::context::Context;
use crate::def::WorkflowDef;
use crate::error::{WorkflowError, WorkflowResult};
use crate::schedule::execution_plan;
use crate::task::TaskStatus;
use crate::tasklist::{SharedTask, TaskList};

/// Live workflow built from a validated [`WorkflowDef`].
pub struct Workflow {
    /// Stable workflow id.
    pub id: String,
    /// Human-readable workflow name.
    pub name: String,
    agents: HashMap<String, Arc<Agent>>,
    task_list: TaskList,
    _observer: Arc<dyn Observer>,
}

impl Workflow {
    /// Build a live workflow from its declarative definition.
    ///
    /// # Errors
    ///
    /// Returns `WorkflowError` when graph validation fails, agent binding fails,
    /// a task references an unknown agent, or task construction fails.
    pub async fn from_def(
        def: WorkflowDef,
        providers: &ProviderRegistry,
        tools: &ToolRegistry,
        observer: Arc<dyn Observer>,
    ) -> WorkflowResult<Self> {
        def.validate_graph()?;

        let mut agents = HashMap::with_capacity(def.agents.len());
        for agent_def in def.agents {
            let id = agent_def.id.clone();
            let agent = Agent::from_def(agent_def, providers, tools, Arc::clone(&observer)).await?;
            agents.insert(id, Arc::new(agent));
        }

        let mut task_list = TaskList::new();
        for task_def in dependency_ordered_tasks(def.tasks)? {
            if !agents.contains_key(&task_def.agent_id) {
                return Err(WorkflowError::AgentNotFound(task_def.agent_id));
            }
            task_list.add_task(task_def)?;
        }

        Ok(Self {
            id: def.id,
            name: def.name,
            agents,
            task_list,
            _observer: observer,
        })
    }

    /// Borrow the runtime task graph.
    pub fn task_list(&self) -> &TaskList {
        &self.task_list
    }

    /// Return the depth-based execution plan for this workflow.
    ///
    /// # Errors
    ///
    /// Returns `WorkflowError::Lock` when task lock acquisition fails.
    pub fn execution_plan(&self) -> WorkflowResult<HashMap<i32, HashSet<String>>> {
        execution_plan(&self.task_list)
    }

    /// Run the workflow DAG level by level.
    ///
    /// # Errors
    ///
    /// Returns `WorkflowError` when no ready tasks remain, agent execution
    /// fails, task locks fail, or a spawned task cannot return an outcome.
    pub async fn run(self: &Arc<Self>, context: Context) -> WorkflowResult<()> {
        let context = Arc::new(context);
        loop {
            if self.task_list.is_complete()? {
                return Ok(());
            }

            let ready = self.task_list.get_ready_tasks()?;
            if ready.is_empty() {
                return Err(WorkflowError::Stalled(self.task_list.pending_ids()?));
            }

            let mut handles = Vec::with_capacity(ready.len());
            for task in ready {
                let workflow = Arc::clone(self);
                let context = Arc::clone(&context);
                handles.push(tokio::spawn(async move {
                    workflow.run_one(task, &context).await
                }));
            }

            let mut first_error = None;
            for handle in handles {
                match handle.await {
                    Ok(Ok(())) => {}
                    Ok(Err(err)) => {
                        if first_error.is_none() {
                            first_error = Some(err);
                        }
                    }
                    Err(_join_error) => {
                        if first_error.is_none() {
                            first_error = Some(WorkflowError::Lock);
                        }
                    }
                }
            }

            if let Some(err) = first_error {
                return Err(err);
            }
        }
    }

    async fn run_one(&self, task: SharedTask, _context: &Context) -> WorkflowResult<()> {
        let (agent_id, prompt) = {
            let mut guard = task.write().map_err(|_| WorkflowError::Lock)?;
            guard.status = TaskStatus::Running;
            (guard.agent_id.clone(), guard.prompt.clone())
        };

        let agent = self
            .agents
            .get(&agent_id)
            .ok_or_else(|| WorkflowError::AgentNotFound(agent_id.clone()))?;
        let run = agent.run_prompt(&prompt, &[]).await?;

        let mut guard = task.write().map_err(|_| WorkflowError::Lock)?;
        guard.status = TaskStatus::Completed;
        guard.result = Some(run.output);
        Ok(())
    }
}

fn dependency_ordered_tasks(
    mut tasks: Vec<crate::def::TaskDef>,
) -> WorkflowResult<Vec<crate::def::TaskDef>> {
    let mut ordered = Vec::with_capacity(tasks.len());
    let mut inserted: HashSet<String> = HashSet::with_capacity(tasks.len());

    while !tasks.is_empty() {
        let mut ready: Vec<usize> = tasks
            .iter()
            .enumerate()
            .filter_map(|(index, task)| {
                task.dependencies
                    .iter()
                    .all(|dependency| inserted.contains(dependency))
                    .then_some(index)
            })
            .collect();
        ready.sort_by(|left, right| tasks[*left].id.cmp(&tasks[*right].id));

        let Some(index) = ready.first().copied() else {
            return Err(WorkflowError::Cycle(String::new()));
        };
        let task = tasks.remove(index);
        inserted.insert(task.id.clone());
        ordered.push(task);
    }

    Ok(ordered)
}
