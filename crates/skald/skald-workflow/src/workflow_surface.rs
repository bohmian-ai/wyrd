//! User-facing Workflow authoring + run surface.
//!
//! Mirrors the [`skald_agent::Agent`] pyclass-is-the-class pattern: one struct
//! holds envelope metadata, the durable spec body, derived cascade children,
//! and the resolved per-step agents used at run time. The same struct serves
//! both the Rust user surface and the Python `wyrd.agent.Workflow` class.

use std::collections::{BTreeMap, HashMap};
use std::path::Path;
use std::sync::Arc;

use chrono::Utc;
use serde_json::{Map, Value};
use skald_agent::{Agent, Observer};
use wyrd_spec::card::workflow::{
    WorkflowAction, WorkflowCard, WorkflowCardError, WorkflowSpec, WorkflowStep,
};
use wyrd_spec::error::WyrdError;
use wyrd_spec::metadata::{Annotations, CardMetadata, Labels};
use wyrd_spec::reference::{AgentRef, CardRef, PromptRef};

use crate::context::Context;
use crate::def::{TaskDef, WorkflowAgent, WorkflowDef, default_max_retries};
use crate::error::{WorkflowError, WorkflowResult};
use crate::run::WorkflowRun;
use crate::workflow::DagExecutor;

/// Workflow-level input accepted by [`Workflow::run`].
#[derive(Debug, Clone)]
pub enum WorkflowInput {
    /// Single text input exposed as the `input` template variable.
    Text(String),
    /// Pre-shaped variable bindings.
    Vars(Map<String, Value>),
}

impl From<&str> for WorkflowInput {
    fn from(value: &str) -> Self {
        Self::Text(value.to_owned())
    }
}

impl From<String> for WorkflowInput {
    fn from(value: String) -> Self {
        Self::Text(value)
    }
}

impl From<Map<String, Value>> for WorkflowInput {
    fn from(value: Map<String, Value>) -> Self {
        Self::Vars(value)
    }
}

impl From<HashMap<String, String>> for WorkflowInput {
    fn from(value: HashMap<String, String>) -> Self {
        let mut map = Map::new();
        for (key, item) in value {
            map.insert(key, Value::String(item));
        }
        Self::Vars(map)
    }
}

impl From<Value> for WorkflowInput {
    fn from(value: Value) -> Self {
        match value {
            Value::Object(map) => Self::Vars(map),
            other => {
                let mut map = Map::new();
                map.insert("input".to_owned(), other);
                Self::Vars(map)
            }
        }
    }
}

impl WorkflowInput {
    pub(crate) fn into_context_input(self) -> Map<String, Value> {
        match self {
            Self::Text(text) => {
                let mut map = Map::new();
                map.insert("input".to_owned(), Value::String(text));
                map
            }
            Self::Vars(map) => map,
        }
    }
}

/// User-facing workflow surface: meta + spec + resolved agents + cascade.
///
/// The pyclass IS the Python class (per the Wyrd S12C doctrine). Construction
/// flows through `Workflow::new`, the sugar constructors `Workflow::sequential`
/// / `Workflow::parallel`, or the `Workflow::builder` DAG primitive. `.run()`
/// drives the resolved agents through the internal [`DagExecutor`].
#[derive(Clone)]
#[cfg_attr(
    feature = "python",
    pyo3::pyclass(module = "wyrd.agent", name = "Workflow", skip_from_py_object)
)]
pub struct Workflow {
    pub(crate) meta: CardMetadata,
    pub(crate) spec: WorkflowSpec,
    pub(crate) cascade_children: Vec<CardRef>,
    pub(crate) resolved_agents: HashMap<String, Arc<Agent>>,
    /// Runtime-only observers attached to this workflow instance.
    pub(crate) observers: Vec<Arc<dyn Observer>>,
}

impl std::fmt::Debug for Workflow {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Workflow")
            .field("name", &self.meta.name)
            .field("version", &self.meta.version)
            .field("steps", &self.spec.steps.len())
            .field("cascade_children", &self.cascade_children.len())
            .finish()
    }
}

impl Workflow {
    /// Build an empty workflow with the given name.
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            meta: CardMetadata {
                name: Some(name.into()),
                version: None,
                space: None,
                uid: None,
                labels: Labels::default(),
                annotations: Annotations::default(),
            },
            spec: WorkflowSpec::default(),
            cascade_children: Vec::new(),
            resolved_agents: HashMap::new(),
            observers: Vec::new(),
        }
    }

    /// Sugar constructor: link the supplied agents in a linear chain.
    pub fn sequential<I>(name: impl Into<String>, agents: I) -> WorkflowResult<Self>
    where
        I: IntoIterator<Item = Agent>,
    {
        let mut wf = Self::new(name);
        let mut previous: Option<String> = None;
        for agent in agents {
            let deps: Vec<String> = previous.iter().cloned().collect();
            let step_id = wf.append_agent_step(agent, deps)?;
            previous = Some(step_id);
        }
        wf.spec.validate_dag()?;
        Ok(wf)
    }

    /// Sugar constructor: every agent runs in parallel, no dependencies.
    pub fn parallel<I>(name: impl Into<String>, agents: I) -> WorkflowResult<Self>
    where
        I: IntoIterator<Item = Agent>,
    {
        let mut wf = Self::new(name);
        for agent in agents {
            wf.append_agent_step(agent, Vec::new())?;
        }
        wf.spec.validate_dag()?;
        Ok(wf)
    }

    /// Return a copy with runtime observers attached.
    ///
    /// Observers are runtime-only state. They are not serialized into the
    /// workflow card and must be reattached after loading from YAML.
    #[must_use]
    pub fn with_observers(mut self, observers: Vec<Arc<dyn Observer>>) -> Self {
        self.observers = observers;
        self
    }

    /// Borrow the runtime observers attached to this workflow.
    #[must_use]
    pub fn observers(&self) -> &[Arc<dyn Observer>] {
        &self.observers
    }

    /// Open a fluent builder for explicit DAG construction.
    #[must_use]
    pub fn builder(name: impl Into<String>) -> WorkflowBuilder {
        WorkflowBuilder {
            wf: Self::new(name),
        }
    }

    /// Set the workflow's semantic version.
    #[must_use]
    pub fn with_version(mut self, version: impl Into<String>) -> Self {
        self.meta.version = Some(version.into());
        self
    }

    /// Set the workflow's space.
    #[must_use]
    pub fn with_space(mut self, space: impl Into<String>) -> Self {
        self.meta.space = Some(space.into());
        self
    }

    /// Append `agent` as a new step with no dependencies.
    ///
    /// # Errors
    /// Returns DAG validation errors when the resulting graph is invalid.
    #[allow(clippy::should_implement_trait)]
    pub fn add(mut self, agent: Agent) -> WorkflowResult<Self> {
        self.append_agent_step(agent, Vec::new())?;
        self.spec.validate_dag()?;
        Ok(self)
    }

    /// Append `agent` as a new step depending on `deps`.
    ///
    /// # Errors
    /// Returns DAG validation errors when the resulting graph is invalid.
    pub fn add_after<I, S>(mut self, agent: Agent, deps: I) -> WorkflowResult<Self>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let deps: Vec<String> = deps.into_iter().map(Into::into).collect();
        self.append_agent_step(agent, deps)?;
        self.spec.validate_dag()?;
        Ok(self)
    }

    /// Borrow this workflow's envelope metadata.
    #[must_use]
    pub fn meta(&self) -> &CardMetadata {
        &self.meta
    }

    /// Borrow this workflow's durable spec body.
    #[must_use]
    pub fn spec(&self) -> &WorkflowSpec {
        &self.spec
    }

    /// Optional card name.
    #[must_use]
    pub fn name_str(&self) -> Option<&str> {
        self.meta.name.as_deref()
    }

    /// Optional card version.
    #[must_use]
    pub fn version_str(&self) -> Option<&str> {
        self.meta.version.as_deref()
    }

    /// Optional card space.
    #[must_use]
    pub fn space_str(&self) -> Option<&str> {
        self.meta.space.as_deref()
    }

    /// Ordered step ids.
    #[must_use]
    pub fn step_ids(&self) -> Vec<String> {
        self.spec.steps.iter().map(|step| step.id.clone()).collect()
    }

    /// Derived cascade children.
    #[must_use]
    pub fn cascade_children(&self) -> &[CardRef] {
        &self.cascade_children
    }

    /// Project this workflow into a durable [`WorkflowCard`] envelope holder.
    ///
    /// # Errors
    /// Returns missing-name / missing-version errors when identity fields are
    /// not set.
    pub fn to_card(&self) -> Result<WorkflowCard, WyrdError> {
        let name = self
            .meta
            .name
            .clone()
            .ok_or(WorkflowCardError::MissingName)?;
        let version = self
            .meta
            .version
            .clone()
            .ok_or(WorkflowCardError::MissingVersion)?;
        let space = self.meta.space.clone().unwrap_or_else(|| "default".into());
        Ok(WorkflowCard {
            space,
            name,
            version,
            uid: self.meta.uid.clone().unwrap_or_default(),
            labels: self.meta.labels.clone(),
            annotations: self.meta.annotations.clone(),
            spec: self.spec.clone(),
            cascade_children: self.cascade_children.clone(),
            created_at: Utc::now(),
        })
    }

    /// Reconstruct a workflow from a `WorkflowCard` envelope.
    ///
    /// Inline `AgentRef::Inline` steps populate the resolved-agents map
    /// eagerly; `AgentRef::Card` steps require runtime resolution before
    /// `.run()` can drive them.
    ///
    /// # Errors
    /// Returns prompt or agent resolution errors when an inline agent's
    /// prompt cannot be resolved.
    pub fn from_card(
        card: WorkflowCard,
        tool_resolver: &dyn skald_tool::ToolResolver,
        prompt_resolver: &dyn skald_agent::PromptResolver,
    ) -> Result<Self, WyrdError> {
        let mut resolved = HashMap::new();
        for step in &card.spec.steps {
            if let WorkflowAction::Agent(AgentRef::Inline(spec)) = &step.action {
                let card = wyrd_spec::AgentCard {
                    space: card.space.clone(),
                    name: format!("{}-{}", card.name, step.id),
                    version: "0.0.0".to_owned(),
                    uid: String::new(),
                    labels: Labels::default(),
                    annotations: Annotations::default(),
                    spec: (**spec).clone(),
                    cascade_children: Vec::new(),
                    created_at: Utc::now(),
                };
                let agent = Agent::from_card(card, tool_resolver, prompt_resolver)?;
                resolved.insert(step.id.clone(), Arc::new(agent));
            }
        }
        Ok(Self {
            meta: CardMetadata {
                name: Some(card.name),
                version: Some(card.version),
                space: Some(card.space),
                uid: (!card.uid.is_empty()).then_some(card.uid),
                labels: card.labels,
                annotations: card.annotations,
            },
            spec: card.spec,
            cascade_children: card.cascade_children,
            resolved_agents: resolved,
            observers: Vec::new(),
        })
    }

    /// Serialize this workflow as canonical envelope YAML.
    ///
    /// # Errors
    /// Returns identity, validation, or YAML codec errors.
    pub fn to_yaml_string(&self) -> Result<String, WyrdError> {
        let card = self.to_card()?;
        serde_yaml::to_string(&card).map_err(|error| WorkflowCardError::yaml(&error).into())
    }

    /// Parse a workflow from canonical envelope YAML.
    ///
    /// # Errors
    /// Returns parse, prompt resolution, or agent resolution errors.
    pub fn from_yaml_str(
        yaml: &str,
        tool_resolver: &dyn skald_tool::ToolResolver,
        prompt_resolver: &dyn skald_agent::PromptResolver,
    ) -> Result<Self, WyrdError> {
        let card: WorkflowCard =
            serde_yaml::from_str(yaml).map_err(|error| WorkflowCardError::yaml(&error))?;
        Self::from_card(card, tool_resolver, prompt_resolver)
    }

    /// Write this workflow's envelope YAML to disk.
    ///
    /// # Errors
    /// Returns identity, IO, or YAML codec errors.
    pub fn save(&self, path: impl AsRef<Path>) -> Result<(), WyrdError> {
        let path = path.as_ref();
        let yaml = self.to_yaml_string()?;
        if let Some(parent) = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            std::fs::create_dir_all(parent)
                .map_err(|error| WorkflowCardError::io(parent.display().to_string(), &error))?;
        }
        std::fs::write(path, yaml)
            .map_err(|error| WorkflowCardError::io(path.display().to_string(), &error))?;
        Ok(())
    }

    /// Load this workflow from a YAML envelope on disk.
    ///
    /// # Errors
    /// Returns IO, parse, prompt resolution, or agent resolution errors.
    pub fn load(
        path: impl AsRef<Path>,
        tool_resolver: &dyn skald_tool::ToolResolver,
        prompt_resolver: &dyn skald_agent::PromptResolver,
    ) -> Result<Self, WyrdError> {
        let path = path.as_ref();
        let yaml = std::fs::read_to_string(path)
            .map_err(|error| WorkflowCardError::io(path.display().to_string(), &error))?;
        Self::from_yaml_str(&yaml, tool_resolver, prompt_resolver)
    }

    /// Drive this workflow's resolved agents through the internal DAG executor.
    ///
    /// # Errors
    /// Returns runtime errors when an agent is missing, the DAG cannot run, or
    /// any per-step retries are exhausted.
    pub async fn run(&self, input: impl Into<WorkflowInput>) -> WorkflowResult<WorkflowRun> {
        let providers = skald_runtime::default_registry();
        self.run_with(providers.as_ref(), input).await
    }

    /// Run the DAG against an explicit provider registry.
    ///
    /// Prefer [`run`](Self::run) for the common case. Use this variant when
    /// injecting a test registry or a non-default provider configuration.
    ///
    /// # Errors
    /// Returns runtime errors when an agent is missing, the DAG cannot run, or
    /// any per-step retries are exhausted.
    pub async fn run_with(
        &self,
        providers: &skald_runtime::ProviderRegistry,
        input: impl Into<WorkflowInput>,
    ) -> WorkflowResult<WorkflowRun> {
        wyrd_observe::init();
        let input = input.into();
        let inner = self.run_with_inner(providers, input);
        match self.observers.as_slice() {
            [] => inner.await,
            [one] => wyrd_observe::with_observer(Arc::clone(one), inner).await,
            many => {
                let composite: Arc<dyn Observer> =
                    Arc::new(wyrd_observe::CompositeObserver::new(many.to_vec()));
                wyrd_observe::with_observer(composite, inner).await
            }
        }
    }

    async fn run_with_inner(
        &self,
        providers: &skald_runtime::ProviderRegistry,
        input: WorkflowInput,
    ) -> WorkflowResult<WorkflowRun> {
        let def = self.to_workflow_def()?;
        let executor = DagExecutor::build(def, providers).await?;
        let mut ctx = Context::new();
        ctx.input = input.into_context_input();
        Arc::new(executor).run(ctx).await
    }

    fn append_agent_step(&mut self, agent: Agent, deps: Vec<String>) -> WorkflowResult<String> {
        let step_id = self.next_step_id(&agent);
        let agent_arc = Arc::new(agent.clone());
        let action = if let Some(card_ref) = agent
            .card_ref()
            .map_err(|error| WorkflowError::Other(error.to_string()))?
        {
            self.cascade_children.push(card_ref.clone());
            WorkflowAction::Agent(AgentRef::Card(card_ref))
        } else {
            let spec = agent.to_spec();
            if let PromptRef::Card(prompt_ref) = &spec.prompt {
                self.cascade_children.push(prompt_ref.clone());
            }
            WorkflowAction::Agent(AgentRef::from(spec))
        };
        self.spec.steps.push(WorkflowStep {
            id: step_id.clone(),
            action,
            depends_on: deps,
            inputs: BTreeMap::new(),
            condition: None,
            timeout_seconds: None,
            retry: None,
            display: BTreeMap::new(),
        });
        self.resolved_agents.insert(step_id.clone(), agent_arc);
        self.dedup_cascade();
        Ok(step_id)
    }

    fn next_step_id(&self, agent: &Agent) -> String {
        let base = agent
            .name_str()
            .map(str::to_owned)
            .unwrap_or_else(|| format!("step-{}", self.spec.steps.len() + 1));
        if !self.resolved_agents.contains_key(&base) {
            return base;
        }
        let mut suffix = 2usize;
        loop {
            let candidate = format!("{base}-{suffix}");
            if !self.resolved_agents.contains_key(&candidate) {
                return candidate;
            }
            suffix += 1;
        }
    }

    fn dedup_cascade(&mut self) {
        self.cascade_children.sort_by(|a, b| {
            let a_key = (
                a.kind.wire_name(),
                a.space.as_str(),
                a.name.as_str(),
                a.version.to_string(),
            );
            let b_key = (
                b.kind.wire_name(),
                b.space.as_str(),
                b.name.as_str(),
                b.version.to_string(),
            );
            a_key.cmp(&b_key)
        });
        self.cascade_children.dedup();
    }

    fn to_workflow_def(&self) -> WorkflowResult<WorkflowDef> {
        let mut agents = Vec::with_capacity(self.spec.steps.len());
        let mut tasks = Vec::with_capacity(self.spec.steps.len());
        for step in &self.spec.steps {
            let agent = self
                .resolved_agents
                .get(&step.id)
                .ok_or_else(|| WorkflowError::AgentNotFound(step.id.clone()))?;
            let prompt = agent.prompt.native().clone();
            agents.push(WorkflowAgent {
                id: step.id.clone(),
                prompt: prompt.clone(),
                run_config: agent.run_config.clone(),
            });
            tasks.push(TaskDef {
                id: step.id.clone(),
                agent_id: step.id.clone(),
                prompt,
                dependencies: step.depends_on.clone(),
                max_retries: step
                    .retry
                    .as_ref()
                    .map_or_else(default_max_retries, |r| r.max_retries),
            });
        }
        Ok(WorkflowDef {
            id: self.meta.name.clone().unwrap_or_else(|| "workflow".into()),
            name: self.meta.name.clone().unwrap_or_else(|| "workflow".into()),
            agents,
            tasks,
        })
    }
}

/// Fluent DAG builder for [`Workflow`].
pub struct WorkflowBuilder {
    wf: Workflow,
}

impl WorkflowBuilder {
    /// Set the workflow's semantic version.
    #[must_use]
    pub fn version(mut self, version: impl Into<String>) -> Self {
        self.wf.meta.version = Some(version.into());
        self
    }

    /// Set the workflow's space.
    #[must_use]
    pub fn space(mut self, space: impl Into<String>) -> Self {
        self.wf.meta.space = Some(space.into());
        self
    }

    /// Append `agent` as a step with no dependencies.
    ///
    /// # Errors
    /// Returns when the resulting workflow cannot be appended to.
    #[allow(clippy::should_implement_trait)]
    pub fn add(mut self, agent: Agent) -> WorkflowResult<Self> {
        self.wf.append_agent_step(agent, Vec::new())?;
        Ok(self)
    }

    /// Append `agent` as a step depending on `deps`.
    ///
    /// # Errors
    /// Returns when the resulting workflow cannot be appended to.
    pub fn add_after<I, S>(mut self, agent: Agent, deps: I) -> WorkflowResult<Self>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let deps: Vec<String> = deps.into_iter().map(Into::into).collect();
        self.wf.append_agent_step(agent, deps)?;
        Ok(self)
    }

    /// Finalize the builder, validating the DAG.
    ///
    /// # Errors
    /// Returns DAG validation errors.
    pub fn build(self) -> WorkflowResult<Workflow> {
        let WorkflowBuilder { wf } = self;
        wf.spec.validate_dag()?;
        Ok(wf)
    }
}
