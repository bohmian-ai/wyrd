//! User-facing Skald agent with card-authoring lifecycle projection.

use std::collections::HashMap;
use std::fmt;
use std::path::Path;
use std::sync::{Arc, OnceLock, RwLock};
use std::time::Duration;

use skald_prompt::Prompt;
use skald_tool::{AgentTool, ToolError, ToolResolver};
use wyrd_spec::{
    AgentCard, AgentCardError, AgentRunConfigSpec, AgentSpec, CardMetadata,
    envelope::CardKind,
    error::WyrdError,
    metadata::{Annotations, Labels},
    reference::{CardRef, PromptRef},
};

use crate::callbacks::{
    AfterAgentFn, AfterModelFn, AfterToolFn, BeforeAgentFn, BeforeModelFn, BeforeToolFn,
};
use crate::error::AgentResult;
use crate::journal::{Journal, NoopJournal};
use crate::run::RunConfig;
use crate::session::{NoSession, SessionId, SessionMemory};

/// Wire form for a durable Agent Card.
pub type AgentWire = AgentCard;

/// Resolves durable prompt references into runtime prompts.
pub trait PromptResolver: Send + Sync {
    /// Resolve a prompt reference into a runtime prompt.
    ///
    /// # Errors
    /// Returns a Wyrd error when a referenced Prompt Card cannot be resolved.
    fn resolve(&self, prompt_ref: &PromptRef) -> Result<Prompt, WyrdError>;
}

impl<T: PromptResolver + ?Sized> PromptResolver for &T {
    fn resolve(&self, prompt_ref: &PromptRef) -> Result<Prompt, WyrdError> {
        (**self).resolve(prompt_ref)
    }
}

/// Process-local prompt resolver used by tests and local YAML loading.
#[derive(Debug, Default)]
pub struct LocalPromptResolver;

impl PromptResolver for LocalPromptResolver {
    fn resolve(&self, prompt_ref: &PromptRef) -> Result<Prompt, WyrdError> {
        match prompt_ref {
            PromptRef::Inline(prompt) => Ok(Prompt::from_native((**prompt).clone())),
            PromptRef::Card(card_ref) => prompt_registry()
                .read()
                .map_err(|error| {
                    WyrdError::from(AgentCardError::validation(format!(
                        "Agent prompt registry lock poisoned: {error}"
                    )))
                })?
                .get(&prompt_key(card_ref))
                .map(|prompt| prompt.as_ref().clone())
                .ok_or_else(|| {
                    AgentCardError::PromptCardNotFound {
                        card_ref: card_ref.clone(),
                    }
                    .into()
                }),
        }
    }
}

/// Return the process-local prompt resolver.
#[must_use]
pub fn default_prompt_resolver() -> &'static LocalPromptResolver {
    static RESOLVER: LocalPromptResolver = LocalPromptResolver;
    &RESOLVER
}

/// Register a prompt in the local Agent prompt resolver.
///
/// # Errors
/// Returns validation errors when the reference is not a Prompt Card reference.
/// Register a prompt under a `CardRef` for resolution by `Agent::try_from_ref`.
///
/// This registry is process-global and is not tenant-isolated. It is intended
/// for single-process, single-tenant use only. In multi-tenant or multi-user
/// processes, callers are responsible for ensuring no cross-tenant interference.
pub fn register_prompt_card(card_ref: &CardRef, prompt: Prompt) -> Result<(), WyrdError> {
    if card_ref.kind != CardKind::Prompt {
        return Err(
            AgentCardError::validation("prompt registry accepts only Prompt Card refs").into(),
        );
    }
    let mut registry = prompt_registry().write().map_err(|error| {
        WyrdError::from(AgentCardError::validation(format!(
            "Agent prompt registry lock poisoned: {error}"
        )))
    })?;
    registry.insert(prompt_key(card_ref), Arc::new(prompt));
    Ok(())
}

/// Remove all entries from the process-global Agent prompt resolver registry.
///
/// See [`register_prompt_card`] for isolation constraints.
pub fn clear_prompt_card_registry() {
    if let Ok(mut registry) = prompt_registry().write() {
        registry.clear();
    }
}

/// Six-callback bundle for a live agent.
#[derive(Clone, Default)]
pub struct AgentCallbacks {
    /// Callbacks fired before the agent run begins.
    pub before_agent: Vec<BeforeAgentFn>,
    /// Callbacks fired after the agent run completes.
    pub after_agent: Vec<AfterAgentFn>,
    /// Callbacks fired before provider model calls.
    pub before_model: Vec<BeforeModelFn>,
    /// Callbacks fired after provider model calls.
    pub after_model: Vec<AfterModelFn>,
    /// Callbacks fired before tool invocations.
    pub before_tool: Vec<BeforeToolFn>,
    /// Callbacks fired after tool invocations.
    pub after_tool: Vec<AfterToolFn>,
}

/// Runnable Wyrd Agent with local card projection and Skald loop execution.
///
/// Use `Agent::new(prompt)` for an already-resolved prompt and
/// `Agent::try_from_ref(prompt_ref, resolver)` when a durable Prompt Card must
/// be resolved before execution.
///
/// Provider, model, and system-prompt identity live on [`Prompt`]; retarget the
/// agent with [`Agent::with_prompt`] or [`Agent::try_with_prompt`].
///
/// # Example
///
/// ```no_run
/// use skald_agent::Agent;
/// use skald_prompt::{OpenAiChatOptions, openai_chat};
///
/// # async fn run_example() -> Result<(), Box<dyn std::error::Error>> {
/// let prompt = openai_chat(
///     "gpt-4o-mini",
///     OpenAiChatOptions {
///         messages: vec!["be helpful".to_owned()],
///         ..Default::default()
///     },
/// )?;
///
/// let agent = Agent::new(prompt)
///     .name("planner")
///     .version("0.3.0");
///
/// let _run = agent.run("draft the doc").await?;
/// # Ok(())
/// # }
/// ```
#[derive(Clone)]
#[cfg_attr(
    feature = "python",
    pyo3::pyclass(module = "wyrd.agent", name = "Agent", skip_from_py_object)
)]
pub struct Agent {
    /// Local Card metadata used for envelope projection.
    pub(crate) meta: CardMetadata,
    /// Preserved durable prompt reference.
    pub(crate) prompt_ref: PromptRef,
    /// Runtime-local tool names preserved for YAML round trips.
    pub(crate) tool_names: Vec<String>,
    /// Stable id used for tracing and diagnostics.
    pub id: String,
    /// Resolved native prompt.
    pub prompt: Arc<Prompt>,
    /// Loop execution settings.
    pub run_config: RunConfig,
    /// Per-agent runtime-local tool cache.
    pub(crate) tools: Vec<Arc<dyn AgentTool>>,
    /// Session memory backend for this agent.
    pub(crate) session: Arc<dyn SessionMemory>,
    /// Run journal backend for this agent.
    pub(crate) journal: Arc<dyn Journal>,
    /// Runtime callback chains.
    pub(crate) callbacks: AgentCallbacks,
    /// Per-agent provider registry override. When set, overrides the global
    /// default registry for this agent's runs only. Not serialized.
    pub(crate) provider_override: Option<Arc<skald_runtime::ProviderRegistry>>,
    /// Python class retained for structured-output instantiation.
    ///
    /// Set via `Agent(output_type=...)` or `agent.run(output_type=...)`.
    /// Overrides `Prompt.py_output_cls` when both are set.
    /// Only present under the `python` feature.
    #[cfg(feature = "python")]
    pub(crate) py_output_cls: Option<Arc<pyo3::Py<pyo3::PyAny>>>,
}

impl fmt::Debug for Agent {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Agent")
            .field("id", &self.id)
            .field("name", &self.meta.name)
            .field("version", &self.meta.version)
            .field("prompt_model", &self.prompt.native().model)
            .field("run_config", &self.run_config)
            .field("tool_names", &self.tool_names)
            .finish()
    }
}

impl Agent {
    /// Build an Agent from an already-resolved Prompt.
    ///
    /// Use this infallible constructor when the prompt is inline and ready to
    /// run.
    #[must_use]
    pub fn new(prompt: Prompt) -> Self {
        let prompt_ref = PromptRef::from(prompt.clone().into_native());
        Self::from_resolved_parts(generate_agent_id(), prompt_ref, prompt)
    }

    /// Build an Agent from an already-shared resolved Prompt.
    ///
    /// Use this when a caller already owns the stable runtime id and shared
    /// prompt handle.
    #[must_use]
    pub fn from_resolved(id: impl Into<String>, prompt: Arc<Prompt>) -> Self {
        let prompt_ref = PromptRef::from(prompt.as_ref().clone().into_native());
        Self::from_resolved_arc_parts(id.into(), prompt_ref, prompt)
    }

    /// Build an Agent from a PromptRef and resolver.
    ///
    /// Use this fallible constructor when the prompt may be a durable Prompt
    /// Card reference.
    ///
    /// # Errors
    /// Returns resolver errors for card-backed prompt references.
    pub fn try_from_ref(
        prompt_ref: PromptRef,
        resolver: &dyn PromptResolver,
    ) -> Result<Self, WyrdError> {
        let prompt = resolve_prompt_ref(&prompt_ref, resolver)?;
        Ok(Self::from_resolved_parts(
            generate_agent_id(),
            prompt_ref,
            prompt,
        ))
    }

    /// Return a copy with a stable runtime id.
    #[must_use]
    pub fn with_id(mut self, id: impl Into<String>) -> Self {
        self.id = id.into();
        self
    }

    /// Return a copy retargeted to a resolved Prompt.
    #[must_use]
    pub fn with_prompt(mut self, prompt: impl Into<Arc<Prompt>>) -> Self {
        let prompt = prompt.into();
        self.prompt_ref = PromptRef::from(prompt.as_ref().clone().into_native());
        self.prompt = prompt;
        self
    }

    /// Return a copy retargeted through a PromptRef and resolver.
    ///
    /// # Errors
    /// Returns resolver errors for card-backed prompt references.
    pub fn try_with_prompt(
        mut self,
        prompt_ref: PromptRef,
        resolver: &dyn PromptResolver,
    ) -> Result<Self, WyrdError> {
        self.prompt = Arc::new(resolve_prompt_ref(&prompt_ref, resolver)?);
        self.prompt_ref = prompt_ref;
        Ok(self)
    }

    /// Return a copy with a per-agent provider registry override.
    ///
    /// When set, this registry is used instead of the process-global default
    /// for every run driven by this agent. The override is purely runtime
    /// state — it is not serialized into the `AgentCard` or `AgentSpec`.
    #[must_use]
    pub fn with_provider_registry(
        mut self,
        registry: Arc<skald_runtime::ProviderRegistry>,
    ) -> Self {
        self.provider_override = Some(registry);
        self
    }

    /// Return the effective provider registry for this agent.
    ///
    /// Returns the per-agent override when set, otherwise `fallback`.
    pub fn effective_providers<'a>(
        &'a self,
        fallback: &'a skald_runtime::ProviderRegistry,
    ) -> &'a skald_runtime::ProviderRegistry {
        self.provider_override.as_deref().unwrap_or(fallback)
    }

    /// Return a copy with one runtime-local tool appended.
    #[must_use]
    pub fn with_tool(mut self, tool: Arc<dyn AgentTool>) -> Self {
        self.tool_names.push(tool.name().to_owned());
        self.tools.push(tool);
        self
    }

    /// Alias for [`with_tool`](Self::with_tool).
    #[must_use]
    pub fn add_tool(self, tool: Arc<dyn AgentTool>) -> Self {
        self.with_tool(tool)
    }

    /// Return a copy with the full runtime-local tool cache replaced.
    #[must_use]
    pub fn with_tools<I>(mut self, tools: I) -> Self
    where
        I: IntoIterator<Item = Arc<dyn AgentTool>>,
    {
        self.tools = tools.into_iter().collect();
        self.tool_names = self
            .tools
            .iter()
            .map(|tool| tool.name().to_owned())
            .collect();
        self
    }

    /// Alias for [`with_tools`](Self::with_tools).
    #[must_use]
    pub fn set_tools(self, tools: Vec<Arc<dyn AgentTool>>) -> Self {
        self.with_tools(tools)
    }

    /// Return a copy with a new run configuration.
    #[must_use]
    pub fn with_run_config(mut self, run_config: RunConfig) -> Self {
        self.run_config = run_config;
        self
    }

    /// Return a copy with a session memory backend.
    #[must_use]
    pub fn with_session(mut self, session: Arc<dyn SessionMemory>) -> Self {
        self.session = session;
        self
    }

    /// Return a copy with a run journal backend.
    #[must_use]
    pub fn with_journal(mut self, journal: Arc<dyn Journal>) -> Self {
        self.journal = journal;
        self
    }

    /// Return a copy with a card name.
    #[must_use]
    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.meta.name = Some(name.into());
        self
    }

    /// Return a copy with a card version.
    #[must_use]
    pub fn version(mut self, version: impl Into<String>) -> Self {
        self.meta.version = Some(version.into());
        self
    }

    /// Return a copy with a card space.
    #[must_use]
    pub fn space(mut self, space: impl Into<String>) -> Self {
        self.meta.space = Some(space.into());
        self
    }

    /// Return a copy with labels.
    #[must_use]
    pub fn labels(mut self, labels: Labels) -> Self {
        self.meta.labels = labels;
        self
    }

    /// Return a copy with annotations.
    #[must_use]
    pub fn annotations(mut self, annotations: Annotations) -> Self {
        self.meta.annotations = annotations;
        self
    }

    /// Registers a callback fired before the agent run begins.
    #[must_use]
    pub fn before_agent(mut self, callback: BeforeAgentFn) -> Self {
        self.callbacks.before_agent.push(callback);
        self
    }

    /// Registers a callback fired after the agent run completes.
    #[must_use]
    pub fn after_agent(mut self, callback: AfterAgentFn) -> Self {
        self.callbacks.after_agent.push(callback);
        self
    }

    /// Registers a callback fired before one provider request is sent.
    #[must_use]
    pub fn before_model(mut self, callback: BeforeModelFn) -> Self {
        self.callbacks.before_model.push(callback);
        self
    }

    /// Registers a callback fired after one provider response is received.
    #[must_use]
    pub fn after_model(mut self, callback: AfterModelFn) -> Self {
        self.callbacks.after_model.push(callback);
        self
    }

    /// Registers a callback fired before one tool invocation.
    #[must_use]
    pub fn before_tool(mut self, callback: BeforeToolFn) -> Self {
        self.callbacks.before_tool.push(callback);
        self
    }

    /// Registers a callback fired after one tool invocation.
    #[must_use]
    pub fn after_tool(mut self, callback: AfterToolFn) -> Self {
        self.callbacks.after_tool.push(callback);
        self
    }

    /// Borrow the runtime id.
    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Borrow the resolved prompt.
    #[must_use]
    pub fn prompt(&self) -> &Arc<Prompt> {
        &self.prompt
    }

    /// Borrow the run configuration.
    #[must_use]
    pub fn run_config(&self) -> &RunConfig {
        &self.run_config
    }

    /// Borrow resolved runtime-local tools.
    #[must_use]
    pub fn tools(&self) -> &[Arc<dyn AgentTool>] {
        &self.tools
    }

    /// Borrow local Card metadata.
    #[must_use]
    pub fn meta(&self) -> &CardMetadata {
        &self.meta
    }

    /// Return the optional card name.
    #[must_use]
    pub fn name_str(&self) -> Option<&str> {
        self.meta.name.as_deref()
    }

    /// Return the optional card version.
    #[must_use]
    pub fn version_str(&self) -> Option<&str> {
        self.meta.version.as_deref()
    }

    /// Return the optional card space.
    #[must_use]
    pub fn space_str(&self) -> Option<&str> {
        self.meta.space.as_deref()
    }

    /// Return the preserved prompt reference.
    #[must_use]
    pub fn prompt_ref(&self) -> &PromptRef {
        &self.prompt_ref
    }

    /// Return preserved runtime-local tool names.
    #[must_use]
    pub fn tool_names(&self) -> &[String] {
        &self.tool_names
    }

    /// Return this agent's card reference when name and version are set.
    ///
    /// # Errors
    /// Returns validation errors when identity fields are malformed.
    pub fn card_ref(&self) -> Result<Option<CardRef>, WyrdError> {
        if self.meta.name.is_none() || self.meta.version.is_none() {
            return Ok(None);
        }
        let Some(card) = self.identity_card()? else {
            return Ok(None);
        };
        card.card_ref().map(Some)
    }

    /// Save this Agent Card YAML envelope to local disk.
    ///
    /// # Errors
    /// Returns identity, IO, YAML, or validation errors.
    pub fn save(&self, path: impl AsRef<Path>) -> Result<(), WyrdError> {
        let path = path.as_ref();
        let yaml = self.to_yaml_string()?;
        if let Some(parent) = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            std::fs::create_dir_all(parent)
                .map_err(|error| AgentCardError::io(parent.display().to_string(), &error))?;
        }
        std::fs::write(path, yaml)
            .map_err(|error| AgentCardError::io(path.display().to_string(), &error))?;
        Ok(())
    }

    /// Load an Agent Card from YAML using explicit tool and prompt resolvers.
    ///
    /// # Errors
    /// Returns card load, prompt resolution, or tool resolution errors.
    pub fn from_yaml_path(
        path: impl AsRef<Path>,
        tool_resolver: &dyn ToolResolver,
        prompt_resolver: &dyn PromptResolver,
    ) -> Result<Self, WyrdError> {
        let path = path.as_ref();
        let yaml = std::fs::read_to_string(path)
            .map_err(|error| AgentCardError::io(path.display().to_string(), &error))?;
        Self::from_yaml_str(&yaml, tool_resolver, prompt_resolver)
    }

    /// Load an Agent Card from a YAML string using explicit resolvers.
    ///
    /// # Errors
    /// Returns YAML, prompt resolution, or tool resolution errors.
    pub fn from_yaml_str(
        input: &str,
        tool_resolver: &dyn ToolResolver,
        prompt_resolver: &dyn PromptResolver,
    ) -> Result<Self, WyrdError> {
        let card: AgentCard =
            serde_yaml::from_str(input).map_err(|error| AgentCardError::yaml(&error))?;
        Self::from_card(card, tool_resolver, prompt_resolver)
    }

    /// Convert this Agent Card to a YAML string.
    ///
    /// # Errors
    /// Returns identity, validation, or YAML errors.
    pub fn to_yaml_string(&self) -> Result<String, WyrdError> {
        serde_yaml::to_string(&self.to_card()?).map_err(|error| AgentCardError::yaml(&error).into())
    }

    /// Convert this agent into a typed Agent Card envelope.
    ///
    /// # Errors
    /// Returns missing identity or invalid identity errors.
    pub fn to_card(&self) -> Result<AgentCard, WyrdError> {
        let mut card = self.identity_card()?.ok_or(AgentCardError::MissingName)?;
        if self.meta.version.is_none() {
            return Err(AgentCardError::MissingVersion.into());
        }
        card.spec = self.to_spec();
        card.cascade_children = derive_cascade_children(&card.spec);
        Ok(card)
    }

    /// Alias for projecting this agent into wire envelope form.
    ///
    /// # Errors
    /// Returns missing identity or invalid identity errors.
    pub fn to_wire(&self) -> Result<AgentWire, WyrdError> {
        self.to_card()
    }

    /// Reconstruct an agent from a typed Agent Card envelope.
    ///
    /// # Errors
    /// Returns prompt resolution or tool resolution errors.
    pub fn from_card(
        card: AgentCard,
        tool_resolver: &dyn ToolResolver,
        prompt_resolver: &dyn PromptResolver,
    ) -> Result<Self, WyrdError> {
        let mut tools = Vec::with_capacity(card.spec.tool_names.len());
        for name in &card.spec.tool_names {
            tools.push(tool_resolver.resolve(name).map_err(tool_resolution_error)?);
        }

        let mut agent = Self::try_from_ref(card.spec.prompt.clone(), prompt_resolver)?
            .with_id(card.name.clone())
            .with_run_config(run_config_from_agent_run_config_spec(&card.spec.run_config))
            .with_tools(tools);
        agent.meta = CardMetadata {
            name: Some(card.name),
            version: Some(card.version),
            space: Some(card.space),
            uid: (!card.uid.is_empty()).then_some(card.uid),
            labels: card.labels,
            annotations: card.annotations,
        };
        agent.tool_names = card.spec.tool_names;
        Ok(agent)
    }

    /// Alias for reconstructing this agent from wire envelope form.
    ///
    /// # Errors
    /// Returns prompt resolution or tool resolution errors.
    pub fn from_wire(
        wire: AgentWire,
        tool_resolver: &dyn ToolResolver,
        prompt_resolver: &dyn PromptResolver,
    ) -> Result<Self, WyrdError> {
        Self::from_card(wire, tool_resolver, prompt_resolver)
    }

    /// Validate whether this local Agent Card can be durably registered.
    ///
    /// # Errors
    /// Returns `WYRD_AGENT_422_RUNTIME_LOCAL_TOOLS_NOT_REGISTRABLE` when
    /// runtime-local tool names are present.
    pub fn validate_registrable(&self) -> Result<(), WyrdError> {
        if self.tool_names.is_empty() {
            Ok(())
        } else {
            Err(AgentCardError::RuntimeLocalToolsNotRegistrable {
                tool_names: self.tool_names.clone(),
            }
            .into())
        }
    }

    /// Derive cascade children from this agent's durable spec.
    #[must_use]
    pub fn cascade_children(&self) -> Vec<CardRef> {
        derive_cascade_children(&self.to_spec())
    }

    /// Run the bounded tool loop using the stored or default provider registry.
    ///
    /// # Errors
    /// Returns Skald agent runtime errors.
    pub async fn run(&self, input: &str) -> AgentResult<crate::run::AgentRun> {
        let providers = skald_runtime::default_registry();
        crate::loop_runtime::run(self, providers.as_ref(), None, input).await
    }

    /// Run the bounded tool loop against a live provider registry.
    ///
    /// # Errors
    /// Returns Skald agent runtime errors.
    pub async fn run_with(
        &self,
        providers: &skald_runtime::ProviderRegistry,
        session_id: Option<SessionId>,
        input: &str,
    ) -> AgentResult<crate::run::AgentRun> {
        crate::loop_runtime::run(self, providers, session_id, input).await
    }

    /// Run the bounded tool loop driven by a rendered prompt with variable
    /// substitution.
    ///
    /// # Errors
    /// Returns Skald agent runtime errors.
    pub async fn run_prompt(
        &self,
        providers: &skald_runtime::ProviderRegistry,
        prompt: &Prompt,
        vars: &[(&str, &str)],
        parent_run_id: Option<&str>,
    ) -> AgentResult<crate::run::AgentRun> {
        crate::loop_runtime::run_prompt(self, providers, prompt, vars, parent_run_id).await
    }

    fn from_resolved_parts(id: String, prompt_ref: PromptRef, prompt: Prompt) -> Self {
        Self::from_resolved_arc_parts(id, prompt_ref, Arc::new(prompt))
    }

    fn from_resolved_arc_parts(id: String, prompt_ref: PromptRef, prompt: Arc<Prompt>) -> Self {
        Self {
            meta: CardMetadata::default(),
            prompt_ref,
            tool_names: Vec::new(),
            id,
            prompt,
            run_config: RunConfig::default(),
            tools: Vec::new(),
            session: Arc::new(NoSession),
            journal: Arc::new(NoopJournal),
            callbacks: AgentCallbacks::default(),
            provider_override: None,
            #[cfg(feature = "python")]
            py_output_cls: None,
        }
    }

    /// Project this agent into its pure durable [`AgentSpec`] body.
    #[must_use]
    pub fn to_spec(&self) -> AgentSpec {
        AgentSpec {
            prompt: self.prompt_ref.clone(),
            tool_names: self.tool_names.clone(),
            run_config: agent_run_config_spec_from_run_config(&self.run_config),
        }
    }

    fn identity_card(&self) -> Result<Option<AgentCard>, WyrdError> {
        let Some(name) = self.meta.name.clone() else {
            return Ok(None);
        };
        let Some(version) = self.meta.version.clone() else {
            return Ok(Some(AgentCard {
                space: self
                    .meta
                    .space
                    .clone()
                    .unwrap_or_else(|| "default".to_owned()),
                name,
                version: String::new(),
                uid: self.meta.uid.clone().unwrap_or_default(),
                labels: self.meta.labels.clone(),
                annotations: self.meta.annotations.clone(),
                spec: self.to_spec(),
                cascade_children: self.cascade_children(),
                created_at: chrono::Utc::now(),
            }));
        };
        Ok(Some(AgentCard {
            space: self
                .meta
                .space
                .clone()
                .unwrap_or_else(|| "default".to_owned()),
            name,
            version,
            uid: self.meta.uid.clone().unwrap_or_default(),
            labels: self.meta.labels.clone(),
            annotations: self.meta.annotations.clone(),
            spec: self.to_spec(),
            cascade_children: self.cascade_children(),
            created_at: chrono::Utc::now(),
        }))
    }
}

/// Convert Skald `RunConfig` into the pure Agent Card spec mirror.
#[must_use]
pub fn agent_run_config_spec_from_run_config(run_config: &RunConfig) -> AgentRunConfigSpec {
    AgentRunConfigSpec {
        max_iterations: Some(run_config.max_iterations),
        tool_concurrency_cap: run_config.tool_concurrency_cap,
        session_recent_limit: run_config.session_recent_limit,
        timeout_ms: run_config.timeout.map(duration_to_millis),
    }
}

/// Convert the pure Agent Card run config mirror into Skald `RunConfig`.
#[must_use]
pub fn run_config_from_agent_run_config_spec(spec: &AgentRunConfigSpec) -> RunConfig {
    let mut run_config = RunConfig::default();
    if let Some(max_iterations) = spec.max_iterations {
        run_config.max_iterations = max_iterations;
    }
    if let Some(tool_concurrency_cap) = spec.tool_concurrency_cap {
        run_config.tool_concurrency_cap = Some(tool_concurrency_cap);
    }
    if let Some(session_recent_limit) = spec.session_recent_limit {
        run_config.session_recent_limit = Some(session_recent_limit);
    }
    if let Some(timeout_ms) = spec.timeout_ms {
        run_config.timeout = Some(Duration::from_millis(timeout_ms));
    }
    run_config
}

/// Derive `CardRef` cascade children from an Agent spec.
#[must_use]
pub fn derive_cascade_children(spec: &AgentSpec) -> Vec<CardRef> {
    match &spec.prompt {
        PromptRef::Card(card_ref) => vec![card_ref.clone()],
        PromptRef::Inline(_) => Vec::new(),
    }
}

fn resolve_prompt_ref(
    prompt_ref: &PromptRef,
    resolver: &dyn PromptResolver,
) -> Result<Prompt, WyrdError> {
    match prompt_ref {
        PromptRef::Inline(prompt) => Ok(Prompt::from_native((**prompt).clone())),
        PromptRef::Card(_) => resolver.resolve(prompt_ref),
    }
}

fn prompt_registry() -> &'static RwLock<HashMap<String, Arc<Prompt>>> {
    static REGISTRY: OnceLock<RwLock<HashMap<String, Arc<Prompt>>>> = OnceLock::new();
    REGISTRY.get_or_init(|| RwLock::new(HashMap::new()))
}

fn prompt_key(card_ref: &CardRef) -> String {
    let space = card_ref
        .space
        .as_ref()
        .map_or_else(|| "default".to_owned(), ToString::to_string);
    format!(
        "{space}/{}:{}@{}",
        card_ref.kind.wire_name(),
        card_ref.name,
        card_ref.version
    )
}

fn tool_resolution_error(error: ToolError) -> AgentCardError {
    match error {
        ToolError::NotRegistered { name, available } => {
            AgentCardError::RuntimeLocalToolNotFound { name, available }
        }
        other => AgentCardError::RuntimeLocalToolNotFound {
            name: "unknown".to_owned(),
            available: vec![other.to_string()],
        },
    }
}

fn duration_to_millis(duration: Duration) -> u64 {
    let millis = duration.as_millis();
    u64::try_from(millis).unwrap_or(u64::MAX)
}

fn generate_agent_id() -> String {
    ulid::Ulid::new().to_string()
}
