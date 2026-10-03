//! User-facing Workflow authoring + run surface.
//!
//! One struct holds envelope metadata, the durable spec body, derived cascade
//! children, and the resolved per-step agents used at run time. It is the Rust
//! user surface; the Python SDK wraps it as the `wyrd.agent.Workflow` class.

use std::collections::{BTreeMap, HashMap};
use std::path::Path;
use std::sync::Arc;

use chrono::Utc;
use serde_json::{Map, Value};
use skald_agent::Agent;
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
/// The Python SDK wraps this type as `wyrd.agent.Workflow`. Construction
/// flows through `Workflow::new`, the sugar constructors `Workflow::sequential`
/// / `Workflow::parallel`, or the `Workflow::builder` DAG primitive. `.run()`
/// validates the resolved graph and drives the Agents through the internal
/// Workflow executor.
#[derive(Clone)]
pub struct Workflow {
    pub(crate) meta: CardMetadata,
    pub(crate) spec: WorkflowSpec,
    pub(crate) cascade_children: Vec<CardRef>,
    pub(crate) resolved_agents: HashMap<String, Arc<Agent>>,
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

    /// Replace the workflow's queryable labels.
    #[must_use]
    pub fn with_labels(mut self, labels: Labels) -> Self {
        self.meta.labels = labels;
        self
    }

    /// Replace the workflow's free-form annotations.
    #[must_use]
    pub fn with_annotations(mut self, annotations: Annotations) -> Self {
        self.meta.annotations = annotations;
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
    /// A referenced step without a resolver, or an unresolved `Path`, stays
    /// unhydrated and fails [`Self::validate`] and every run.
    /// [`Self::from_card_bodies`] supplies already-fetched exact bodies
    /// through this seam.
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

    /// Parse a workflow from one canonical wire-form envelope YAML document.
    ///
    /// This parses a single document only: inline Agent steps hydrate, but
    /// local `path` dependencies, sibling bundles, and Card
    /// references are not resolved here. Load a bundle or a registered
    /// Workflow through the shared client, which feeds
    /// [`Self::from_card_bodies`].
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

    /// Load this workflow from one wire-form YAML envelope on disk.
    ///
    /// Same single-document role as [`Self::from_yaml_str`]; it does not
    /// resolve dependency paths or Card references.
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
    /// Equivalent to [`prepare`](Self::prepare) followed by
    /// [`PreparedWorkflowRun::execute`] with no transition observer. Once
    /// execution starts, step failures, cancellation, deadline expiry, and
    /// size limits are reported in the returned [`WorkflowRun`], never as an
    /// error.
    ///
    /// # Errors
    /// Returns the errors of [`prepare`](Self::prepare).
    pub async fn run_with_options(
        &self,
        dependencies: &WorkflowExecutionDependencies,
        input: impl Into<WorkflowInput>,
        options: WorkflowRunOptions,
    ) -> WorkflowResult<WorkflowRun> {
        Ok(self
            .prepare(dependencies, input, options)?
            .execute(|_| {})
            .await)
    }

    /// Prepare one run without dispatching any step.
    ///
    /// Every validation, input, route, binding-availability, and size check
    /// runs here; the run ID is minted and the absolute total deadline is
    /// fixed from now. The returned run holds the queued snapshot and starts
    /// only when [`PreparedWorkflowRun::execute`] is awaited; dropping it
    /// dispatches nothing. Cancellation flows only through
    /// `options.cancellation`.
    ///
    /// # Errors
    /// Returns the errors of [`validate`](Self::validate), and
    /// `WYRD_WORKFLOW_422_RUN_REQUEST`, `WYRD_WORKFLOW_413_INPUT_TOO_LARGE`,
    /// `WYRD_WORKFLOW_413_GRAPH_TOO_LARGE`,
    /// `WYRD_WORKFLOW_422_ROUTE_UNSUPPORTED`, or
    /// `WYRD_WORKFLOW_503_BINDING_UNAVAILABLE` for invalid input or an
    /// environment that cannot serve the declared routes.
    pub fn prepare(
        &self,
        dependencies: &WorkflowExecutionDependencies,
        input: impl Into<WorkflowInput>,
        options: WorkflowRunOptions,
    ) -> WorkflowResult<PreparedWorkflowRun> {
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
        Ok(PreparedWorkflowRun { executor })
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
            step_id_for_name,
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

/// One prepared Workflow run that has passed every pre-dispatch check.
///
/// Created by [`Workflow::prepare`]. It owns the run's plan, minted run ID,
/// and queued snapshot; nothing is dispatched until
/// [`execute`](Self::execute) is awaited.
pub struct PreparedWorkflowRun {
    /// Executor holding the plan, ledger, limits, and cancellation.
    executor: WorkflowExecutor,
}

impl PreparedWorkflowRun {
    /// The queued snapshot, carrying the run ID the terminal snapshot keeps.
    #[must_use]
    pub fn snapshot(&self) -> &WorkflowRun {
        self.executor.snapshot()
    }

    /// Execute the run to its terminal snapshot.
    ///
    /// `on_transition` is called synchronously on the scheduling task with
    /// the complete snapshot after the run starts and after every step starts
    /// or settles; it must not block or await. The terminal snapshot is
    /// returned, not observed. Never fails: step, cancellation, deadline, and
    /// size outcomes are recorded in the returned snapshot. Dropping the
    /// future aborts the in-flight step tasks.
    pub async fn execute<F>(self, on_transition: F) -> WorkflowRun
    where
        F: FnMut(&WorkflowRun) + Send,
    {
        self.executor.execute(on_transition).await
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

/// Map an Agent name onto the step-ID grammar.
///
/// Characters outside `[A-Za-z0-9_]` become `_`, and a name that does not
/// start with a letter or `_` gains a `_` prefix. Builders use this for the
/// base step ID of a named Agent, so callers can name a predecessor step by
/// its Agent. The Python SDK uses it to resolve an Agent passed as `after`.
pub fn step_id_for_name(name: &str) -> String {
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

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    use serde_json::{Value, json};
    use wyrd_spec::auth::AbsoluteUrl;
    use wyrd_spec::card::common::ParameterValue;
    use wyrd_spec::card::workflow::{
        ExternalGatewayProtocol, LlmRoute, WorkflowRetryPolicy, WorkflowRun, WorkflowRunStatus,
    };
    use wyrd_spec::ids::CredentialBindingName;

    use super::{Workflow, WorkflowInput};
    use crate::error::{WorkflowError, WorkflowResult};
    use crate::route::WorkflowExecutionDependencies;
    use crate::test_support::{
        RecordingTool, Reply, ScriptedProvider, agent, bindings, string_schema,
    };
    use crate::workflow::{WorkflowExecutionLimits, WorkflowRunOptions};

    /// Run `workflow` against `provider` with default local options.
    async fn run_local(
        workflow: &Workflow,
        provider: &Arc<ScriptedProvider>,
        input: impl Into<WorkflowInput>,
    ) -> WorkflowResult<WorkflowRun> {
        workflow
            .run_with_options(
                &WorkflowExecutionDependencies::new(provider.registry()),
                input,
                WorkflowRunOptions::default(),
            )
            .await
    }

    /// Stable code and offending field of a pre-dispatch failure.
    fn code_and_field(error: &WorkflowError) -> (&'static str, String) {
        let field = match error {
            WorkflowError::Spec(error) => error.field().to_owned(),
            WorkflowError::Wyrd(error) => {
                let details = &error.as_problem_json()["details"];
                details["field"]
                    .as_str()
                    .or_else(|| details["step"].as_str())
                    .unwrap_or_default()
                    .to_owned()
            }
            WorkflowError::AgentNotFound(step) => step.clone(),
        };
        (error.code(), field)
    }

    /// Two-step text Workflow `first -> second` without bindings or outputs.
    fn pair(first: &str, second: &str, structured_first: bool) -> Workflow {
        let schema = structured_first.then(|| string_schema(&["summary"]));
        Workflow::new("pair")
            .add(agent("first", first, schema))
            .and_then(|w| w.add_after(agent("second", second, None), ["first"]))
            .expect("fixture steps append")
    }

    /// Scenario 1: every resolved-graph defect fails with a safe,
    /// field-specific stable error before any provider call: unbound and extra
    /// Prompt variables, payload-kind mismatches in either direction, an
    /// unresolved Agent, a hidden (undeclared) step reference, a cycle, an
    /// external route whose protocol differs from the Prompt dialect, a retry
    /// count whose final attempt cannot fit `u32`, and step timeouts or a run
    /// deadline that cannot be represented as an instant.
    #[tokio::test]
    async fn resolved_bindings_reject_before_dispatch() {
        const VALIDATION: &str = "WYRD_WORKFLOW_422_VALIDATION";
        let with_outputs = |workflow: Workflow| {
            workflow
                .with_outputs(bindings(&[("out", "steps.second.output.text")]))
                .expect("outputs declare")
        };
        let mut missing_agent = with_outputs(pair("first", "second", false));
        missing_agent.resolved_agents.remove("second");
        let mut external = with_outputs(pair("first", "second", false));
        external.spec.steps[1].llm_route = Some(LlmRoute::ExtGateway {
            protocol: ExternalGatewayProtocol::AnthropicMessages,
            base_url: AbsoluteUrl::new("https://llm.example.com/v1".to_owned())
                .expect("absolute url"),
            headers: BTreeMap::new(),
            credential_binding: CredentialBindingName::new("corp").expect("binding name"),
        });
        let retry = |max_retries: u32| {
            let mut workflow = with_outputs(pair("first", "second", false));
            workflow.spec.steps[1].retry = Some(WorkflowRetryPolicy {
                max_retries,
                initial_backoff_ms: None,
            });
            workflow
        };
        let mut timeout = with_outputs(pair("first", "second", false));
        timeout.spec.steps[1].timeout_seconds = Some(u64::MAX);
        let cases: Vec<(&str, Workflow, &str, &str)> = vec![
            (
                "unbound variable",
                with_outputs(pair("first ${topic}", "second", false)),
                VALIDATION,
                "steps[0].inputs.topic",
            ),
            (
                "extra binding",
                with_outputs(
                    pair("first", "second", false)
                        .with_step_inputs("first", bindings(&[("extra", "input.topic")]))
                        .expect("binding names are identifiers"),
                ),
                VALIDATION,
                "steps[0].inputs.extra",
            ),
            (
                "text binding to structured step",
                with_outputs(
                    pair("first", "second ${v}", true)
                        .with_step_inputs("second", bindings(&[("v", "steps.first.output.text")]))
                        .expect("binding names are identifiers"),
                ),
                VALIDATION,
                "steps[1].inputs.v",
            ),
            (
                "structured binding to text step",
                with_outputs(
                    pair("first", "second ${v}", false)
                        .with_step_inputs(
                            "second",
                            bindings(&[("v", "steps.first.output.structured.summary")]),
                        )
                        .expect("binding names are identifiers"),
                ),
                VALIDATION,
                "steps[1].inputs.v",
            ),
            (
                "missing agent",
                missing_agent,
                "WYRD_WORKFLOW_404_AGENT",
                "second",
            ),
            (
                "hidden reference",
                pair("first", "second", false)
                    .with_outputs(bindings(&[("out", "steps.first.output.text")]))
                    .and_then(|w| w.add(agent("third", "third ${v}", None)))
                    .and_then(|w| {
                        w.with_step_inputs("third", bindings(&[("v", "steps.first.output.text")]))
                    })
                    .expect("fixture builds"),
                VALIDATION,
                "steps[2].inputs.v",
            ),
            (
                "cycle",
                with_outputs(
                    Workflow::new("cycle")
                        .add_after(agent("first", "first", None), ["second"])
                        .and_then(|w| w.add_after(agent("second", "second", None), ["first"]))
                        .expect("fixture steps append"),
                ),
                "WYRD_WORKFLOW_422_CYCLE",
                "",
            ),
            (
                "external protocol mismatch",
                external,
                "WYRD_WORKFLOW_422_ROUTE_UNSUPPORTED",
                "steps[1].llm_route",
            ),
            (
                "unrepresentable retry count",
                retry(u32::MAX),
                VALIDATION,
                "steps[1].retry.max_retries",
            ),
            (
                "unrepresentable step timeout",
                timeout,
                VALIDATION,
                "steps[1].timeout_seconds",
            ),
        ];

        for (case, workflow, code, field) in cases {
            let provider = ScriptedProvider::new();
            let validated = workflow.validate().expect_err(case);
            let error = run_local(&workflow, &provider, serde_json::Map::new())
                .await
                .expect_err(case);
            let (actual_code, actual_field) = code_and_field(&error);
            assert_eq!(actual_code, code, "{case}: {error}");
            assert_eq!(validated.code(), code, "{case}");
            if !field.is_empty() {
                assert_eq!(actual_field, field, "{case}: {error}");
            }
            assert!(provider.requests().is_empty(), "{case} dispatched");
        }

        // The largest representable retry count is accepted.
        retry(u32::MAX - 1)
            .validate()
            .expect("u32::MAX - 1 retries validate");

        // An unrepresentable local run deadline is refused before dispatch.
        let provider = ScriptedProvider::new();
        let error = with_outputs(pair("first", "second", false))
            .run_with_options(
                &WorkflowExecutionDependencies::new(provider.registry()),
                serde_json::Map::new(),
                WorkflowRunOptions {
                    limits: WorkflowExecutionLimits {
                        deadline: Some(Duration::MAX),
                        ..WorkflowExecutionLimits::default()
                    },
                    ..WorkflowRunOptions::default()
                },
            )
            .await
            .expect_err("deadline is not representable");
        assert_eq!(
            code_and_field(&error),
            ("WYRD_WORKFLOW_422_RUN_REQUEST", "deadline".to_owned())
        );
        assert!(provider.requests().is_empty());
    }

    /// Scenario 7: the Rust builder declares inputs, step bindings, and
    /// outputs; `build` refuses an incomplete graph; edges inject no data;
    /// text shorthand needs a declared string `input`; unknown or mistyped
    /// input fails as a run request while defaults fill missing keys; and a
    /// local tool runs only when declared on the Agent, and a model request
    /// for an undeclared tool fails that step without executing anything.
    #[tokio::test]
    async fn explicit_builder_contract() {
        let declared = Workflow::builder("explicit")
            .add(agent("draft", "draft ${input} at ${level}", None))
            .and_then(|b| b.add_after(agent("review", "review only", None), ["draft"]));
        let incomplete = declared
            .and_then(|b| b.with_outputs(bindings(&[("out", "steps.review.output.text")])))
            .expect("fixture builds");
        let error = incomplete.build().expect_err("draft variables are unbound");
        assert_eq!(code_and_field(&error).1, "steps[0].inputs.input");

        let unknown_step = Workflow::builder("explicit")
            .add(agent("draft", "draft", None))
            .and_then(|b| b.with_step_inputs("nope", bindings(&[("x", "input.x")])))
            .err()
            .expect("unknown step is refused");
        assert_eq!(code_and_field(&unknown_step).1, "steps.nope");

        let inputs: BTreeMap<String, ParameterValue> = [
            ("input".to_owned(), ParameterValue::Str(String::new())),
            ("level".to_owned(), ParameterValue::Int(2)),
        ]
        .into();
        let workflow = Workflow::builder("explicit")
            .add(agent("draft", "draft ${input} at ${level}", None))
            .and_then(|b| b.add_after(agent("review", "review only", None), ["draft"]))
            .and_then(|b| b.with_inputs(inputs))
            .and_then(|b| {
                b.with_step_inputs(
                    "draft",
                    bindings(&[("input", "input.input"), ("level", "input.level")]),
                )
            })
            .and_then(|b| {
                b.with_outputs(bindings(&[
                    ("review", "steps.review.output.text"),
                    ("level", "input.level"),
                ]))
            })
            .and_then(|b| b.build())
            .expect("explicit workflow builds");

        let provider = ScriptedProvider::new();
        provider.on("draft", vec![Reply::Text("drafted".into())]);
        provider.on("review", vec![Reply::Text("approved".into())]);
        let run = run_local(&workflow, &provider, "essay")
            .await
            .expect("text shorthand binds the declared input");
        assert_eq!(run.status, WorkflowRunStatus::Succeeded, "{run:?}");
        assert_eq!(run.outputs["review"], json!("approved"));
        assert_eq!(run.outputs["level"], json!(2));
        assert_eq!(
            provider.requests(),
            vec!["draft essay at 2".to_owned(), "review only".to_owned()],
            "the edge injects nothing into review"
        );

        let refusals: [(&str, Value, &str); 2] = [
            ("unknown input", json!({ "other": "x" }), "input.other"),
            ("mistyped input", json!({ "level": "two" }), "input.level"),
        ];
        for (case, input, field) in refusals {
            let Value::Object(input) = input else {
                panic!("fixture input must be an object");
            };
            let provider = ScriptedProvider::new();
            let error = run_local(&workflow, &provider, input)
                .await
                .expect_err(case);
            assert_eq!(
                code_and_field(&error),
                ("WYRD_WORKFLOW_422_RUN_REQUEST", field.to_owned()),
                "{case}"
            );
            assert!(provider.requests().is_empty(), "{case} dispatched");
        }

        let undeclared_text = Workflow::builder("no_input")
            .add(agent("only", "static", None))
            .and_then(|b| b.with_outputs(bindings(&[("out", "steps.only.output.text")])))
            .and_then(|b| b.build())
            .expect("static workflow builds");
        let provider = ScriptedProvider::new();
        let error = run_local(&undeclared_text, &provider, "loose text")
            .await
            .expect_err("text shorthand needs a declared string input");
        assert_eq!(
            code_and_field(&error),
            ("WYRD_WORKFLOW_422_RUN_REQUEST", "input".to_owned())
        );

        let declared_tool = Arc::new(RecordingTool {
            name: "lookup".to_owned(),
            calls: AtomicUsize::new(0),
        });
        let tooled = Workflow::builder("tooled")
            .add(agent("lookup_step", "lookup call", None).with_tool(declared_tool.clone()))
            .and_then(|b| b.with_outputs(bindings(&[("found", "steps.lookup_step.output.text")])))
            .and_then(|b| b.build())
            .expect("tooled workflow builds");
        let provider = ScriptedProvider::new();
        provider.on(
            "lookup call",
            vec![
                Reply::ToolCall("lookup".into(), json!({})),
                Reply::Text("found it".into()),
            ],
        );
        let run = run_local(&tooled, &provider, serde_json::Map::new())
            .await
            .expect("tooled workflow starts");
        assert_eq!(
            run.outputs.get("found"),
            Some(&json!("found it")),
            "{run:?}"
        );
        assert_eq!(declared_tool.calls.load(Ordering::SeqCst), 1);
        assert!(
            provider
                .requests()
                .iter()
                .any(|text| text.contains("answered")),
            "the tool result is returned to the model"
        );

        let undeclared = Workflow::builder("undeclared")
            .add(agent("plain_step", "plain call", None))
            .and_then(|b| b.with_outputs(bindings(&[("plain", "steps.plain_step.output.text")])))
            .and_then(|b| b.build())
            .expect("plain workflow builds");
        let provider = ScriptedProvider::new();
        provider.on(
            "plain call",
            vec![
                Reply::ToolCall("lookup".into(), json!({})),
                Reply::Text("no tool".into()),
            ],
        );
        let run = run_local(&undeclared, &provider, serde_json::Map::new())
            .await
            .expect("plain workflow starts");
        let refused = run.steps["plain_step"]
            .error
            .as_ref()
            .map(|error| error.code.as_str());
        assert_eq!(refused, Some("WYRD_AGENT_404_TOOL_NOT_IN_AGENT"));
        assert_eq!(run.status, WorkflowRunStatus::Failed);
        assert_eq!(
            declared_tool.calls.load(Ordering::SeqCst),
            1,
            "an Agent that does not declare the tool never executes it"
        );
    }
}
