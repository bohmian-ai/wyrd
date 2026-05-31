//! Live workflow execution.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use skald_agent::{Agent, Observer, ToolRegistry};
use skald_runtime::ProviderRegistry;
use skald_spec::ProviderResponse;

use crate::context::Context;
use crate::def::WorkflowDef;
use crate::error::{WorkflowError, WorkflowResult};
use crate::observer_ext::emit_task_event;
use crate::run::{TaskEvent, TaskOutcome, WorkflowRun, now_ms};
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
    observer: Arc<dyn Observer>,
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
            observer,
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
    pub async fn run(self: &Arc<Self>, _context: Context) -> WorkflowResult<WorkflowRun> {
        let events = Arc::new(Mutex::new(Vec::new()));
        loop {
            if self.task_list.is_complete()? {
                let final_events = finish_events(events)?;
                return self.collect_run(final_events);
            }

            let ready = self.task_list.get_ready_tasks()?;
            if ready.is_empty() {
                return Err(WorkflowError::Stalled(self.task_list.pending_ids()?));
            }

            let mut handles = Vec::with_capacity(ready.len());
            for task in ready {
                let workflow = Arc::clone(self);
                let events = Arc::clone(&events);
                handles.push(tokio::spawn(async move {
                    workflow.run_one_with_retries(task, &events).await
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

    /// Execute one task without mutating live task status.
    ///
    /// # Errors
    ///
    /// Returns `WorkflowError` when task lookup fails, agent lookup fails, all
    /// provider attempts fail, output validation exhausts retries, or locks fail.
    pub async fn execute_task(
        &self,
        task_id: &str,
        _context: &Context,
    ) -> WorkflowResult<ProviderResponse> {
        let shared = self
            .task_list
            .get_task(task_id)
            .ok_or_else(|| WorkflowError::TaskNotFound(task_id.to_owned()))?;
        let (agent_id, prompt, max_retries) = {
            let guard = shared.read().map_err(|_| WorkflowError::Lock)?;
            (
                guard.agent_id.clone(),
                guard.prompt.clone(),
                guard.max_retries,
            )
        };

        let agent = self
            .agents
            .get(&agent_id)
            .ok_or_else(|| WorkflowError::AgentNotFound(agent_id.clone()))?;

        for attempt in 0..=max_retries {
            let response = match agent.run_prompt(&prompt, &[]).await {
                Ok(run) => run.output,
                Err(_err) if attempt == max_retries => {
                    return Err(WorkflowError::MaxRetriesExceeded(task_id.to_owned()));
                }
                Err(_err) => continue,
            };

            let validation = {
                let guard = shared.read().map_err(|_| WorkflowError::Lock)?;
                guard.validate_response(&response)
            };
            match validation {
                Ok(()) => return Ok(response),
                Err(err) if attempt == max_retries => return Err(err),
                Err(_err) => {}
            }
        }

        Err(WorkflowError::MaxRetriesExceeded(task_id.to_owned()))
    }

    async fn run_one_with_retries(
        &self,
        task: SharedTask,
        events: &Mutex<Vec<TaskEvent>>,
    ) -> WorkflowResult<()> {
        let (agent_id, prompt, max_retries, task_id) = {
            let mut guard = task.write().map_err(|_| WorkflowError::Lock)?;
            guard.status = TaskStatus::Running;
            guard.retry_count = 0;
            (
                guard.agent_id.clone(),
                guard.prompt.clone(),
                guard.max_retries,
                guard.id.clone(),
            )
        };

        let agent = self
            .agents
            .get(&agent_id)
            .ok_or_else(|| WorkflowError::AgentNotFound(agent_id.clone()))?;

        for attempt in 0..=max_retries {
            let started_at = now_ms();
            let response = match agent.run_prompt(&prompt, &[]).await {
                Ok(run) => run.output,
                Err(_err) if attempt == max_retries => {
                    let error = WorkflowError::MaxRetriesExceeded(task_id.clone());
                    self.record_failure(&task, &task_id, attempt, started_at, events, &error)?;
                    return Err(error);
                }
                Err(err) => {
                    let workflow_err = WorkflowError::from(err);
                    self.record_retry(&task, &task_id, attempt, started_at, events, &workflow_err)?;
                    continue;
                }
            };

            let validation = {
                let guard = task.read().map_err(|_| WorkflowError::Lock)?;
                guard.validate_response(&response)
            };
            match validation {
                Ok(()) => {
                    let mut guard = task.write().map_err(|_| WorkflowError::Lock)?;
                    guard.status = TaskStatus::Completed;
                    guard.result = Some(response);
                    drop(guard);
                    self.record_completion(&task_id, attempt, started_at, events)?;
                    return Ok(());
                }
                Err(err) if attempt == max_retries => {
                    self.record_failure(&task, &task_id, attempt, started_at, events, &err)?;
                    return Err(err);
                }
                Err(err) => {
                    self.record_retry(&task, &task_id, attempt, started_at, events, &err)?;
                }
            }
        }

        Err(WorkflowError::MaxRetriesExceeded(task_id))
    }

    fn record_completion(
        &self,
        task_id: &str,
        attempt: u32,
        started_at: i64,
        events: &Mutex<Vec<TaskEvent>>,
    ) -> WorkflowResult<()> {
        let event = TaskEvent {
            task_id: task_id.to_owned(),
            status: TaskStatus::Completed,
            started_at,
            ended_at: now_ms(),
            attempt: attempt + 1,
            error: None,
        };
        emit_task_event(self.observer.as_ref(), &event);
        push_event(events, event)
    }

    fn record_retry(
        &self,
        task: &SharedTask,
        task_id: &str,
        attempt: u32,
        started_at: i64,
        events: &Mutex<Vec<TaskEvent>>,
        err: &WorkflowError,
    ) -> WorkflowResult<()> {
        {
            let mut guard = task.write().map_err(|_| WorkflowError::Lock)?;
            guard.retry_count = guard.retry_count.saturating_add(1);
        }
        let event = TaskEvent {
            task_id: task_id.to_owned(),
            status: TaskStatus::Failed,
            started_at,
            ended_at: now_ms(),
            attempt: attempt + 1,
            error: Some(err.code().to_owned()),
        };
        emit_task_event(self.observer.as_ref(), &event);
        push_event(events, event)
    }

    fn record_failure(
        &self,
        task: &SharedTask,
        task_id: &str,
        attempt: u32,
        started_at: i64,
        events: &Mutex<Vec<TaskEvent>>,
        err: &WorkflowError,
    ) -> WorkflowResult<()> {
        {
            let mut guard = task.write().map_err(|_| WorkflowError::Lock)?;
            guard.status = TaskStatus::Failed;
        }
        let event = TaskEvent {
            task_id: task_id.to_owned(),
            status: TaskStatus::Failed,
            started_at,
            ended_at: now_ms(),
            attempt: attempt + 1,
            error: Some(err.code().to_owned()),
        };
        emit_task_event(self.observer.as_ref(), &event);
        push_event(events, event)
    }

    fn collect_run(&self, events: Vec<TaskEvent>) -> WorkflowResult<WorkflowRun> {
        let mut tasks = HashMap::new();
        for (id, shared) in self.task_list.iter_in_order() {
            let guard = shared.read().map_err(|_| WorkflowError::Lock)?;
            tasks.insert(
                id.clone(),
                TaskOutcome {
                    status: guard.status,
                    result: guard.result.clone(),
                    retries: guard.retry_count,
                },
            );
        }
        Ok(WorkflowRun {
            tasks,
            events,
            last_task_id: self.task_list.get_last_task_id().map(str::to_owned),
        })
    }
}

fn push_event(events: &Mutex<Vec<TaskEvent>>, event: TaskEvent) -> WorkflowResult<()> {
    events.lock().map_err(|_| WorkflowError::Lock)?.push(event);
    Ok(())
}

fn finish_events(events: Arc<Mutex<Vec<TaskEvent>>>) -> WorkflowResult<Vec<TaskEvent>> {
    match Arc::try_unwrap(events) {
        Ok(mutex) => mutex.into_inner().map_err(|_| WorkflowError::Lock),
        Err(shared) => shared
            .lock()
            .map(|events| events.clone())
            .map_err(|_| WorkflowError::Lock),
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
