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
use wyrd_spec::card::common::ParameterValue;
use wyrd_spec::card::prompt::is_valid_parameter_name;
use wyrd_spec::card::workflow::{
    WorkflowAction, WorkflowBinding, WorkflowCard, WorkflowCardError, WorkflowRun, WorkflowSpec,
    WorkflowStep, WorkflowValidationError,
};
use wyrd_spec::error::WyrdError;
use wyrd_spec::ids::SpaceName;
use wyrd_spec::metadata::{Annotations, CardMetadata, Labels};
use wyrd_spec::reference::{CardRef, InlineableRef};
use wyrd_spec::{AgentCard, AgentSpec};

use crate::error::WorkflowResult;
use crate::plan::{ExecutionPlan, ResolvedGraph};
use crate::route::WorkflowExecutionDependencies;
use crate::workflow::{WorkflowExecutor, WorkflowRunOptions};

/// Invocation input accepted by [`Workflow::run`].
///
/// Values are checked against the Workflow's declared inputs: unknown keys
/// and values of the wrong declared type are rejected, and declared defaults
/// fill missing keys.
#[derive(Debug, Clone)]
pub enum WorkflowInput {
    /// Shorthand for the declared string input named `input`.
    Text(String),
    /// Values keyed by declared input name.
    Vars(Map<String, Value>),
}

/// Resolves durable Agent Card references into runtime agents.
pub trait AgentResolver: Send + Sync {
    /// Resolve a referenced Agent Card without performing filesystem or
    /// network work in the workflow surface.
    fn resolve(&self, agent_ref: &InlineableRef<AgentSpec>) -> Result<Agent, WyrdError>;
}

impl<T: AgentResolver + ?Sized> AgentResolver for &T {
    /// Delegate to the referenced resolver.
    fn resolve(&self, agent_ref: &InlineableRef<AgentSpec>) -> Result<Agent, WyrdError> {
        (**self).resolve(agent_ref)
    }
}

impl From<&str> for WorkflowInput {
    /// Text shorthand for the declared `input`.
    fn from(value: &str) -> Self {
        Self::Text(value.to_owned())
    }
}

impl From<String> for WorkflowInput {
    /// Text shorthand for the declared `input`.
    fn from(value: String) -> Self {
        Self::Text(value)
    }
}

impl From<Map<String, Value>> for WorkflowInput {
    /// Values keyed by declared input name.
    fn from(value: Map<String, Value>) -> Self {
        Self::Vars(value)
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
    /// Edges order execution only; bind data with
    /// [`with_step_inputs`](Self::with_step_inputs).
    ///
    /// # Errors
    /// Returns an error when the Agent's Card identity cannot be projected.
    // justification: builder-pattern add() means append-a-workflow-step (returns Self for chaining), not std::ops::Add arithmetic
    #[allow(clippy::should_implement_trait)]
    pub fn add(mut self, agent: Agent) -> WorkflowResult<Self> {
        self.append_agent_step(agent, Vec::new())?;
        Ok(self)
    }

    /// Append `agent` as a new step depending on `deps`.
    ///
    /// Edges order execution only; bind data with
    /// [`with_step_inputs`](Self::with_step_inputs).
    ///
    /// # Errors
    /// Returns an error when the Agent's Card identity cannot be projected.
    pub fn add_after<I, S>(mut self, agent: Agent, deps: I) -> WorkflowResult<Self>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let deps: Vec<String> = deps.into_iter().map(Into::into).collect();
        self.append_agent_step(agent, deps)?;
        Ok(self)
    }

    /// Declare the Workflow inputs and their native defaults, replacing any
    /// previous declaration.
    ///
    /// # Errors
    /// Returns a validation error for a name outside the parameter identifier
    /// grammar.
    pub fn with_inputs(mut self, inputs: BTreeMap<String, ParameterValue>) -> WorkflowResult<Self> {
        for name in inputs.keys() {
            require_identifier(&format!("inputs.{name}"), name)?;
        }
        self.spec.inputs = inputs;
        Ok(self)
    }

    /// Bind the unresolved Prompt variables of step `step_id`, replacing any
    /// previous bindings for that step.
    ///
    /// Completeness against the Prompt is checked when the Workflow is
    /// validated, built, loaded, or run.
    ///
    /// # Errors
    /// Returns a validation error for an unknown step or a variable name
    /// outside the parameter identifier grammar.
    pub fn with_step_inputs(
        mut self,
        step_id: &str,
        bindings: BTreeMap<String, WorkflowBinding>,
    ) -> WorkflowResult<Self> {
        for name in bindings.keys() {
            require_identifier(&format!("steps.{step_id}.inputs.{name}"), name)?;
        }
        let step = self
            .spec
            .steps
            .iter_mut()
            .find(|step| step.id == step_id)
            .ok_or_else(|| {
                WorkflowValidationError::invalid(&format!("steps.{step_id}"), "unknown step id")
            })?;
        step.inputs = bindings;
        Ok(self)
    }

    /// Declare the Workflow outputs, replacing any previous declaration.
    ///
    /// # Errors
    /// Returns a validation error for a name outside the parameter identifier
    /// grammar.
    pub fn with_outputs(
        mut self,
        outputs: BTreeMap<String, WorkflowBinding>,
    ) -> WorkflowResult<Self> {
        for name in outputs.keys() {
            require_identifier(&format!("outputs.{name}"), name)?;
        }
        self.spec.outputs = outputs;
        Ok(self)
    }

    /// Validate the complete Workflow against its resolved Agents.
    ///
    /// # Errors
    /// Returns the pure contract, missing-Agent, Prompt-variable, binding,
    /// output-schema, and route-dialect errors that would otherwise fail a
    /// run before dispatch.
    pub fn validate(&self) -> WorkflowResult<()> {
        ResolvedGraph::resolve(&self.spec, &self.resolved_agents).map(|_| ())
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
    /// Inline agent steps populate the resolved-agents map eagerly. Referenced
    /// agent steps require [`Self::from_card_with_agent_resolver`] before
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
        Self::from_card_with_agent_resolver(card, tool_resolver, prompt_resolver, None)
    }

    /// Reconstruct a workflow and hydrate referenced Agent Card steps through
    /// an explicit runtime resolver.
    ///
    /// `Sibling` is accepted here because the loader and registration engine
    /// have already established its exact identity. The resolver owns the
    /// durable-card lookup; this crate performs no filesystem or network IO.
    ///
    /// # Errors
    /// Returns Workflow contract validation errors, or prompt, tool, or agent
    /// resolver errors.
    pub fn from_card_with_agent_resolver(
        card: WorkflowCard,
        tool_resolver: &dyn skald_tool::ToolResolver,
        prompt_resolver: &dyn skald_agent::PromptResolver,
        agent_resolver: Option<&dyn AgentResolver>,
    ) -> Result<Self, WyrdError> {
        card.spec.validate()?;
        let mut resolved = HashMap::new();
        for step in &card.spec.steps {
            match &step.action {
                WorkflowAction::Agent(InlineableRef::Inline(spec)) => {
                    let card = AgentCard {
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
                WorkflowAction::Agent(
                    agent_ref @ (InlineableRef::Ref(_) | InlineableRef::Sibling { .. }),
                ) => {
                    if let Some(agent_resolver) = agent_resolver {
                        let agent = agent_resolver.resolve(agent_ref)?;
                        resolved.insert(step.id.clone(), Arc::new(agent));
                    }
                }
                WorkflowAction::Agent(InlineableRef::Path(_)) => {}
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

    /// Run against the process-default native provider registry.
    ///
    /// # Errors
    /// Returns the pre-dispatch errors of
    /// [`run_with_options`](Self::run_with_options).
    pub async fn run(&self, input: impl Into<WorkflowInput>) -> WorkflowResult<WorkflowRun> {
        let providers = skald_runtime::default_registry();
        self.run_with(providers.as_ref(), input).await
    }

    /// Run against an explicit native provider registry with local defaults.
    ///
    /// # Errors
    /// Returns the pre-dispatch errors of
    /// [`run_with_options`](Self::run_with_options).
    pub async fn run_with(
        &self,
        providers: &skald_runtime::ProviderRegistry,
        input: impl Into<WorkflowInput>,
    ) -> WorkflowResult<WorkflowRun> {
        let dependencies = WorkflowExecutionDependencies::new(providers.clone());
        self.run_with_options(&dependencies, input, WorkflowRunOptions::default())
            .await
    }

    /// Run with explicit execution dependencies, limits, and cancellation.
    ///
    /// Every validation, input, route, binding-availability, and size check
    /// runs before any step is dispatched. Once execution starts, step
    /// failures, cancellation, deadline expiry, and size limits are reported
    /// in the returned [`WorkflowRun`], never as an error.
    ///
    /// # Errors
    /// Returns the errors of [`validate`](Self::validate), and
    /// `WYRD_WORKFLOW_422_RUN_REQUEST`, `WYRD_WORKFLOW_413_INPUT_TOO_LARGE`,
    /// `WYRD_WORKFLOW_413_GRAPH_TOO_LARGE`,
    /// `WYRD_WORKFLOW_422_ROUTE_UNSUPPORTED`, or
    /// `WYRD_WORKFLOW_503_BINDING_UNAVAILABLE` for invalid input or an
    /// environment that cannot serve the declared routes.
    pub async fn run_with_options(
        &self,
        dependencies: &WorkflowExecutionDependencies,
        input: impl Into<WorkflowInput>,
        options: WorkflowRunOptions,
    ) -> WorkflowResult<WorkflowRun> {
        skald_observer::init();
        let plan = ExecutionPlan::build(
            &self.spec,
            &self.resolved_agents,
            dependencies,
            input.into(),
            options.limits.max_input_bytes,
        )?;
        let workflow = match &self.meta.uid {
            Some(_) => Some(self.to_card()?.card_ref()?),
            None => None,
        };
        let workflow_id = self
            .meta
            .name
            .clone()
            .unwrap_or_else(|| "workflow".to_owned());
        let executor =
            WorkflowExecutor::new(workflow_id, workflow, plan, dependencies.native(), options)?;
        let run = executor.execute();
        Ok(match self.observers.as_slice() {
            [] => run.await,
            [one] => skald_observer::with_observer(Arc::clone(one), run).await,
            many => {
                let composite: Arc<dyn Observer> =
                    Arc::new(skald_observer::CompositeObserver::new(many.to_vec()));
                skald_observer::with_observer(composite, run).await
            }
        })
    }

    fn append_agent_step(&mut self, agent: Agent, deps: Vec<String>) -> WorkflowResult<String> {
        let step_id = self.next_step_id(&agent);
        let agent_arc = Arc::new(agent.clone());
        let action = if let Some(card_ref) = agent.card_ref()? {
            self.cascade_children.push(card_ref.clone());
            WorkflowAction::Agent(InlineableRef::Ref(card_ref))
        } else {
            let spec = agent.to_spec();
            if let Some(prompt_ref) = spec.prompt.as_card_ref() {
                self.cascade_children.push(prompt_ref.clone());
            }
            WorkflowAction::Agent(InlineableRef::from(spec))
        };
        self.spec.steps.push(WorkflowStep {
            id: step_id.clone(),
            action,
            depends_on: deps,
            inputs: BTreeMap::new(),
            llm_route: None,
            fallback: None,
            timeout_seconds: None,
            retry: None,
            display: BTreeMap::new(),
        });
        self.resolved_agents.insert(step_id.clone(), agent_arc);
        self.dedup_cascade();
        Ok(step_id)
    }

    /// Derive a unique step ID from the Agent name.
    ///
    /// Characters outside the parameter identifier grammar become `_`, a
    /// leading digit gains a `_` prefix, unnamed Agents become `step_<n>`, and
    /// collisions gain a `_<n>` suffix.
    fn next_step_id(&self, agent: &Agent) -> String {
        let base = agent.name_str().map_or_else(
            || format!("step_{}", self.spec.steps.len() + 1),
            |name| {
                let mut id: String = name
                    .chars()
                    .map(|ch| {
                        if ch.is_ascii_alphanumeric() || ch == '_' {
                            ch
                        } else {
                            '_'
                        }
                    })
                    .collect();
                if !id.starts_with(|ch: char| ch.is_ascii_alphabetic() || ch == '_') {
                    id.insert(0, '_');
                }
                id
            },
        );
        if !self.resolved_agents.contains_key(&base) {
            return base;
        }
        let mut suffix = 2usize;
        loop {
            let candidate = format!("{base}_{suffix}");
            if !self.resolved_agents.contains_key(&candidate) {
                return candidate;
            }
            suffix += 1;
        }
    }

    /// Sort and deduplicate derived cascade children.
    fn dedup_cascade(&mut self) {
        self.cascade_children.sort_by(|a, b| {
            let a_key = (
                a.kind.wire_name(),
                a.space.as_ref().map(SpaceName::as_str),
                a.name.as_str(),
                a.version.to_string(),
            );
            let b_key = (
                b.kind.wire_name(),
                b.space.as_ref().map(SpaceName::as_str),
                b.name.as_str(),
                b.version.to_string(),
            );
            a_key.cmp(&b_key)
        });
        self.cascade_children.dedup();
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
    // justification: builder-pattern add() means append-a-workflow-step (returns Self for chaining), not std::ops::Add arithmetic
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

    /// Declare Workflow inputs; see [`Workflow::with_inputs`].
    ///
    /// # Errors
    /// Returns the errors of [`Workflow::with_inputs`].
    pub fn with_inputs(self, inputs: BTreeMap<String, ParameterValue>) -> WorkflowResult<Self> {
        Ok(Self {
            wf: self.wf.with_inputs(inputs)?,
        })
    }

    /// Bind one step's Prompt variables; see [`Workflow::with_step_inputs`].
    ///
    /// # Errors
    /// Returns the errors of [`Workflow::with_step_inputs`].
    pub fn with_step_inputs(
        self,
        step_id: &str,
        bindings: BTreeMap<String, WorkflowBinding>,
    ) -> WorkflowResult<Self> {
        Ok(Self {
            wf: self.wf.with_step_inputs(step_id, bindings)?,
        })
    }

    /// Declare Workflow outputs; see [`Workflow::with_outputs`].
    ///
    /// # Errors
    /// Returns the errors of [`Workflow::with_outputs`].
    pub fn with_outputs(self, outputs: BTreeMap<String, WorkflowBinding>) -> WorkflowResult<Self> {
        Ok(Self {
            wf: self.wf.with_outputs(outputs)?,
        })
    }

    /// Finalize the builder after complete resolved validation.
    ///
    /// # Errors
    /// Returns the errors of [`Workflow::validate`].
    pub fn build(self) -> WorkflowResult<Workflow> {
        self.wf.validate()?;
        Ok(self.wf)
    }
}

/// Require the shared parameter identifier grammar for an authored name.
///
/// # Errors
/// Returns a validation error naming `field` when `name` does not match.
fn require_identifier(field: &str, name: &str) -> Result<(), WorkflowValidationError> {
    if is_valid_parameter_name(name) {
        Ok(())
    } else {
        Err(WorkflowValidationError::invalid(
            field,
            "must match [A-Za-z_][A-Za-z0-9_]*",
        ))
    }
}
