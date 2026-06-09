//! Live workflow execution.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use serde_json::Value;
use skald_agent::{Agent, AgentError};
use skald_prompt::Prompt as RuntimePrompt;
use skald_runtime::ProviderRegistry;
use skald_spec::{MessageNum, Prompt, ProviderName, ProviderRequest, ProviderResponse};
use tokio::sync::RwLock;
use wyrd_observe::current;

use crate::context::Context;
use crate::def::WorkflowDef;
use crate::error::{WorkflowError, WorkflowResult};
use crate::run::{TaskEvent, TaskOutcome, WorkflowRun, now_ms};
use crate::schedule::execution_plan;
use crate::task::{TaskStatus, response_schema_name};
use crate::tasklist::{SharedTask, TaskList};

/// Live workflow built from a validated [`WorkflowDef`].
pub struct DagExecutor {
    /// Stable workflow id.
    pub id: String,
    /// Human-readable workflow name.
    pub name: String,
    providers: ProviderRegistry,
    agents: HashMap<String, Arc<Agent>>,
    task_list: TaskList,
}

impl DagExecutor {
    /// Build a live workflow from its declarative definition.
    ///
    /// # Errors
    ///
    /// Returns `WorkflowError` when graph validation fails, agent binding fails,
    /// a task references an unknown agent, or task construction fails.
    pub async fn build(def: WorkflowDef, providers: &ProviderRegistry) -> WorkflowResult<Self> {
        def.validate_graph()?;

        let mut agents = HashMap::with_capacity(def.agents.len());
        for agent_def in def.agents {
            let id = agent_def.id.clone();
            let agent = Agent::new(RuntimePrompt::from_native(agent_def.prompt))
                .with_id(agent_def.id)
                .with_run_config(agent_def.run_config);
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
            providers: providers.clone(),
            agents,
            task_list,
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
    pub async fn run(self: &Arc<Self>, context: Context) -> WorkflowResult<WorkflowRun> {
        let workflow_run_id = ulid::Ulid::new().to_string();
        let observer = current();
        let workflow_started_at = Instant::now();
        let step_count = self.task_list.len();
        observer
            .on_workflow_start(&workflow_run_id, &self.id, step_count)
            .await;

        let context = Arc::new(RwLock::new(context));
        let events = Arc::new(Mutex::new(Vec::new()));
        loop {
            if self.task_list.is_complete()? {
                let final_events = finish_events(events)?;
                let final_context = finish_context(context).await;
                observer
                    .on_workflow_finish(&workflow_run_id, &self.id, workflow_started_at.elapsed())
                    .await;
                return self.collect_run(final_events, final_context.parameters);
            }

            let ready = self.task_list.get_ready_tasks()?;
            if ready.is_empty() {
                return Err(WorkflowError::Stalled(self.task_list.pending_ids()?));
            }

            let mut handles = Vec::with_capacity(ready.len());
            for task in ready {
                let workflow = Arc::clone(self);
                let events = Arc::clone(&events);
                let context = Arc::clone(&context);
                let parent_run_id = workflow_run_id.clone();
                let task_observer = Arc::clone(&observer);
                handles.push(tokio::spawn(async move {
                    let task =
                        workflow.run_one_with_retries(task, &events, context, &parent_run_id);
                    wyrd_observe::with_observer(task_observer, task).await
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

        let native_prompt = prompt;
        let prompt = RuntimePrompt::from_native(native_prompt.clone());
        for attempt in 0..=max_retries {
            let response = match agent
                .run_prompt(
                    agent.effective_providers(&self.providers),
                    &prompt,
                    &[],
                    None,
                )
                .await
            {
                Ok(run) => run
                    .final_response
                    .ok_or_else(|| WorkflowError::AgentMissingFinalResponse(task_id.to_owned()))?,
                Err(err) if attempt == max_retries => {
                    return Err(terminal_agent_error(task_id, &native_prompt, err));
                }
                Err(_err) => continue,
            };

            let validation = {
                let guard = shared.read().map_err(|_| WorkflowError::Lock)?;
                guard.validate_response(&response)
            };
            match validation {
                Ok(()) => return Ok(response.as_ref().clone()),
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
        context: Arc<RwLock<Context>>,
        parent_run_id: &str,
    ) -> WorkflowResult<()> {
        let (agent_id, prompt, max_retries, task_id, dependencies) = {
            let mut guard = task.write().map_err(|_| WorkflowError::Lock)?;
            guard.status = TaskStatus::Running;
            guard.retry_count = 0;
            (
                guard.agent_id.clone(),
                guard.prompt.clone(),
                guard.max_retries,
                guard.id.clone(),
                guard.dependencies().to_vec(),
            )
        };

        let agent = self
            .agents
            .get(&agent_id)
            .ok_or_else(|| WorkflowError::AgentNotFound(agent_id.clone()))?;

        for attempt in 0..=max_retries {
            let started_at = now_ms();
            let bindings = {
                let ctx_guard = context.read().await;
                bindings_from_context(&task_id, &prompt, &ctx_guard)?
            };
            let bound_prompt = {
                let pairs: Vec<(&str, &str)> = bindings
                    .iter()
                    .map(|(key, value)| (key.as_str(), value.as_str()))
                    .collect();
                prompt
                    .bind(&pairs)
                    .map_err(|error| WorkflowError::Other(error.to_string()))?
            };
            let prompt_for_run = self
                .prompt_with_handoff(
                    &bound_prompt,
                    &dependencies,
                    &agent.prompt.native().request.provider(),
                    &context,
                )
                .await?;
            let runtime_prompt = RuntimePrompt::from_native(prompt_for_run);
            let agent_run = match agent
                .run_prompt(
                    agent.effective_providers(&self.providers),
                    &runtime_prompt,
                    &[],
                    Some(parent_run_id),
                )
                .await
            {
                Ok(run) => run,
                Err(err) if attempt == max_retries => {
                    let error = terminal_agent_error(&task_id, &prompt, err);
                    self.record_failure(&task, &task_id, attempt, started_at, events, &error)?;
                    return Err(error);
                }
                Err(err) => {
                    let workflow_err = WorkflowError::from(err);
                    self.record_retry(&task, &task_id, attempt, started_at, events, &workflow_err)?;
                    continue;
                }
            };
            let response = agent_run
                .final_response
                .clone()
                .ok_or_else(|| WorkflowError::AgentMissingFinalResponse(task_id.clone()))?;
            let structured_output = agent_run.structured_output.clone();

            let validation = {
                let guard = task.read().map_err(|_| WorkflowError::Lock)?;
                guard.validate_response(&response)
            };
            match validation {
                Ok(()) => {
                    let task_messages = crate::handoff::extract_messages_for_handoff(
                        &response,
                        &agent.prompt.native().request.provider(),
                    )?;
                    {
                        let mut guard = task.write().map_err(|_| WorkflowError::Lock)?;
                        guard.status = TaskStatus::Completed;
                        guard.result = Some(response.as_ref().clone());
                    }
                    {
                        let mut ctx_guard = context.write().await;
                        ctx_guard.record_task_messages(task_id.clone(), task_messages);
                        if let Some(map) = structured_output {
                            ctx_guard.ingest_structured_output(&map);
                        }
                    }
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

    async fn prompt_with_handoff(
        &self,
        prompt: &Prompt,
        dependencies: &[String],
        dst_provider: &ProviderName,
        context: &RwLock<Context>,
    ) -> WorkflowResult<Prompt> {
        let mut prompt_for_run = prompt.clone();
        let upstream = self
            .translated_upstream_messages(dependencies, dst_provider, context)
            .await?;
        self.prepend_handoff_messages(&mut prompt_for_run.request, dst_provider, &upstream)?;
        Ok(prompt_for_run)
    }

    async fn translated_upstream_messages(
        &self,
        dependencies: &[String],
        dst_provider: &ProviderName,
        context: &RwLock<Context>,
    ) -> WorkflowResult<Vec<MessageNum>> {
        let snapshots = {
            let ctx_guard = context.read().await;
            dependencies
                .iter()
                .filter_map(|dep_id| {
                    ctx_guard
                        .task_messages_for(dep_id)
                        .map(|messages| (dep_id.clone(), messages.to_vec()))
                })
                .collect::<Vec<_>>()
        };

        let mut upstream = Vec::new();
        for (dep_id, messages) in snapshots {
            let src_provider = self.upstream_provider(&dep_id)?;
            let translated =
                crate::handoff::handoff_messages(&src_provider, dst_provider, &messages)?;
            upstream.extend(translated);
        }
        Ok(upstream)
    }

    fn prepend_handoff_messages(
        &self,
        request: &mut ProviderRequest,
        dst_provider: &ProviderName,
        upstream: &[MessageNum],
    ) -> WorkflowResult<()> {
        if upstream.is_empty() {
            return Ok(());
        }

        match request {
            ProviderRequest::OpenAiChatCompletion(req) => {
                let messages = upstream
                    .iter()
                    .map(|msg| match msg {
                        MessageNum::OpenAi(message) => Ok((**message).clone()),
                        _ => Err(unsupported_handoff(&provider_of_message(msg), dst_provider)),
                    })
                    .collect::<WorkflowResult<Vec<_>>>()?;
                req.messages.splice(0..0, messages);
            }
            ProviderRequest::OpenAiChatCompatible { request: req, .. } => {
                let messages = upstream
                    .iter()
                    .map(|msg| match msg {
                        MessageNum::OpenAi(message) => Ok((**message).clone()),
                        _ => Err(unsupported_handoff(&provider_of_message(msg), dst_provider)),
                    })
                    .collect::<WorkflowResult<Vec<_>>>()?;
                req.messages.splice(0..0, messages);
            }
            ProviderRequest::AnthropicMessage(req) => {
                let messages = upstream
                    .iter()
                    .map(|msg| match msg {
                        MessageNum::Anthropic(message) => Ok(message.clone()),
                        _ => Err(unsupported_handoff(&provider_of_message(msg), dst_provider)),
                    })
                    .collect::<WorkflowResult<Vec<_>>>()?;
                req.messages.splice(0..0, messages);
            }
            ProviderRequest::GeminiGenerateContent(req) => {
                let messages = upstream
                    .iter()
                    .map(|msg| match msg {
                        MessageNum::Gemini(message) => Ok(message.clone()),
                        _ => Err(unsupported_handoff(&provider_of_message(msg), dst_provider)),
                    })
                    .collect::<WorkflowResult<Vec<_>>>()?;
                req.contents.splice(0..0, messages);
            }
            ProviderRequest::Vertex(req) => {
                let messages = upstream
                    .iter()
                    .map(|msg| match msg {
                        MessageNum::Gemini(message) => Ok(message.clone()),
                        _ => Err(unsupported_handoff(&provider_of_message(msg), dst_provider)),
                    })
                    .collect::<WorkflowResult<Vec<_>>>()?;
                req.0.contents.splice(0..0, messages);
            }
            ProviderRequest::OpenAiResponses(_)
            | ProviderRequest::OpenAiEmbeddings(_)
            | ProviderRequest::GoogleBatchEmbed(_)
            | ProviderRequest::VertexPredict(_)
            | ProviderRequest::RawV1 { .. } => {
                return Err(unsupported_handoff(dst_provider, dst_provider));
            }
            _ => return Err(unsupported_handoff(dst_provider, dst_provider)),
        }
        Ok(())
    }

    fn upstream_provider(&self, dep_id: &str) -> WorkflowResult<ProviderName> {
        let task = self
            .task_list
            .get_task(dep_id)
            .ok_or_else(|| WorkflowError::TaskNotFound(dep_id.to_owned()))?;
        let agent_id = {
            let guard = task.read().map_err(|_| WorkflowError::Lock)?;
            guard.agent_id.clone()
        };
        let agent = self
            .agents
            .get(&agent_id)
            .ok_or_else(|| WorkflowError::AgentNotFound(agent_id.clone()))?;
        Ok(agent.prompt.native().request.provider())
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
        push_event(events, event)
    }

    fn collect_run(
        &self,
        events: Vec<TaskEvent>,
        parameters: serde_json::Map<String, Value>,
    ) -> WorkflowResult<WorkflowRun> {
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
        let last_task_id = self.task_list.get_last_task_id().map(str::to_owned);
        let final_output = last_task_id
            .as_ref()
            .and_then(|id| tasks.get(id))
            .and_then(|outcome| outcome.result.as_ref())
            .and_then(|response| response.adapter().text())
            .map(std::borrow::Cow::into_owned);
        Ok(WorkflowRun {
            tasks,
            events,
            last_task_id,
            parameters,
            final_output,
        })
    }
}

fn push_event(events: &Mutex<Vec<TaskEvent>>, event: TaskEvent) -> WorkflowResult<()> {
    events.lock().map_err(|_| WorkflowError::Lock)?.push(event);
    Ok(())
}

fn unsupported_handoff(src: &ProviderName, dst: &ProviderName) -> WorkflowError {
    WorkflowError::UnsupportedHandoff {
        src: src.clone(),
        dst: dst.clone(),
    }
}

fn provider_of_message(msg: &MessageNum) -> ProviderName {
    match msg {
        MessageNum::OpenAi(_) => ProviderName::OpenAi,
        MessageNum::Anthropic(_) => ProviderName::Anthropic,
        MessageNum::Gemini(_) => ProviderName::Google,
        _ => ProviderName::Custom("unknown".to_owned()),
    }
}

fn bindings_from_context(
    step_id: &str,
    prompt: &Prompt,
    ctx: &Context,
) -> WorkflowResult<Vec<(String, String)>> {
    let mut out = Vec::with_capacity(prompt.variables.len());
    for name in &prompt.variables {
        let value = ctx
            .lookup_variable(name)
            .ok_or_else(|| WorkflowError::MissingParameter {
                step_id: step_id.to_owned(),
                name: name.clone(),
            })?;
        out.push((name.clone(), value_to_template_string(value)?));
    }
    Ok(out)
}

fn value_to_template_string(value: &Value) -> WorkflowResult<String> {
    match value {
        Value::String(text) => Ok(text.clone()),
        Value::Null => Ok(String::new()),
        other => {
            serde_json::to_string(other).map_err(|error| WorkflowError::Other(error.to_string()))
        }
    }
}

fn terminal_agent_error(task_id: &str, prompt: &Prompt, error: AgentError) -> WorkflowError {
    match error {
        AgentError::StructuredOutputDecode { detail, .. } => {
            WorkflowError::ResponseValidationFailed {
                task_id: task_id.to_owned(),
                expected_schema: response_schema_name(&prompt.response_type),
                received: detail,
            }
        }
        _ => WorkflowError::MaxRetriesExceeded(task_id.to_owned()),
    }
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

async fn finish_context(context: Arc<RwLock<Context>>) -> Context {
    match Arc::try_unwrap(context) {
        Ok(lock) => lock.into_inner(),
        Err(shared) => shared.read().await.clone(),
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
