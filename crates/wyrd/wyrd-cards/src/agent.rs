//! Agent Card user-facing holder, builder, lifecycle, and runtime resolution.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, OnceLock, RwLock};
use std::time::Duration;

use skald_agent::{
    AfterAgentFn, AfterModelFn, AfterToolFn, Agent, AgentError, AgentRun, BeforeAgentFn,
    BeforeModelFn, BeforeToolFn, Journal, RunConfig, SessionId, SessionMemory,
};
use skald_prompt::Prompt;
use skald_runtime::ProviderRegistry;
use skald_tool::{AgentTool, ToolError, ToolResolver, default_registry};
use wyrd_spec::card::agent::{AgentRunConfigSpec, AgentSpec};
use wyrd_spec::envelope::CardKind;
use wyrd_spec::error::WyrdError;
use wyrd_spec::metadata::{Annotations, Labels};
use wyrd_spec::reference::{CardRef, PromptRef};

use crate::envelope::AgentCard;
use crate::error::AgentCardError;

/// User-authored metadata carried beside a live Skald agent.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AgentMetadata {
    /// Optional card name. Required only for save/register projection.
    pub name: Option<String>,
    /// Optional card version. Required only for save/register projection.
    pub version: Option<String>,
    /// Optional card space. Defaults to `default` when projected.
    pub space: Option<String>,
    /// Queryable labels.
    pub labels: Labels,
    /// Free-form annotations.
    pub annotations: Annotations,
}

/// User-facing Wyrd Agent holder with card metadata and a resolved Skald agent.
#[derive(Debug, Clone)]
pub struct AgentWithMeta {
    /// Engine-resolved Skald agent.
    pub agent: Agent,
    /// Local card metadata.
    pub meta: AgentMetadata,
    prompt_ref: PromptRef,
    tool_names: Vec<String>,
}

impl std::ops::Deref for AgentWithMeta {
    type Target = Agent;

    fn deref(&self) -> &Self::Target {
        &self.agent
    }
}

impl AgentWithMeta {
    /// Start a fluent Agent builder.
    pub fn builder() -> AgentBuilder {
        AgentBuilder::default()
    }

    /// Return the optional card name.
    #[must_use]
    pub fn name(&self) -> Option<&str> {
        self.meta.name.as_deref()
    }

    /// Return the optional card version.
    #[must_use]
    pub fn version(&self) -> Option<&str> {
        self.meta.version.as_deref()
    }

    /// Return the optional card space.
    #[must_use]
    pub fn space(&self) -> Option<&str> {
        self.meta.space.as_deref()
    }

    /// Return the preserved prompt reference.
    #[must_use]
    pub const fn prompt_ref(&self) -> &PromptRef {
        &self.prompt_ref
    }

    /// Return the preserved runtime-local tool names.
    #[must_use]
    pub fn tool_names(&self) -> &[String] {
        &self.tool_names
    }

    /// Return this agent's card reference when name and version are set.
    ///
    /// # Errors
    /// Returns validation errors when identity fields are malformed.
    pub fn card_ref(&self) -> Result<Option<CardRef>, WyrdError> {
        let (Some(name), Some(version)) = (&self.meta.name, &self.meta.version) else {
            return Ok(None);
        };
        AgentCard {
            space: self
                .meta
                .space
                .clone()
                .unwrap_or_else(|| "default".to_owned()),
            name: name.clone(),
            version: version.clone(),
            uid: String::new(),
            labels: self.meta.labels.clone(),
            annotations: self.meta.annotations.clone(),
            spec: self.to_spec(),
            cascade_children: Vec::new(),
            created_at: chrono::Utc::now(),
        }
        .card_ref()
        .map(Some)
    }

    /// Save this Agent Card YAML envelope to local disk.
    ///
    /// # Errors
    /// Returns `WYRD_AGENT_422_MISSING_NAME` or
    /// `WYRD_AGENT_422_MISSING_VERSION` when card identity is incomplete, plus
    /// IO/YAML validation errors.
    pub fn save(&self, path: impl AsRef<Path>) -> Result<(), WyrdError> {
        self.to_card()?.save(path)
    }

    /// Load an Agent Card from YAML using the process-wide tool registry.
    ///
    /// # Errors
    /// Returns card load, prompt resolution, or tool resolution errors.
    pub fn from_yaml_path(path: impl AsRef<Path>) -> Result<Self, WyrdError> {
        Self::from_yaml_path_with_resolver(path, default_registry())
    }

    /// Load an Agent Card from YAML using an explicit tool resolver.
    ///
    /// # Errors
    /// Returns card load, prompt resolution, or tool resolution errors.
    pub fn from_yaml_path_with_resolver<R: ToolResolver>(
        path: impl AsRef<Path>,
        resolver: &R,
    ) -> Result<Self, WyrdError> {
        Self::from_card(AgentCard::load(path)?, resolver)
    }

    /// Load an Agent Card from a YAML string using the process-wide tool registry.
    ///
    /// # Errors
    /// Returns YAML, prompt resolution, or tool resolution errors.
    pub fn from_yaml_str(input: &str) -> Result<Self, WyrdError> {
        Self::from_yaml_str_with_resolver(input, default_registry())
    }

    /// Load an Agent Card from a YAML string using an explicit tool resolver.
    ///
    /// # Errors
    /// Returns YAML, prompt resolution, or tool resolution errors.
    pub fn from_yaml_str_with_resolver<R: ToolResolver>(
        input: &str,
        resolver: &R,
    ) -> Result<Self, WyrdError> {
        let card: AgentCard = serde_yaml::from_str(input)
            .map_err(|error| WyrdError::from(AgentCardError::yaml(&error)))?;
        Self::from_card(card, resolver)
    }

    /// Convert this Agent Card to a YAML string.
    ///
    /// # Errors
    /// Returns identity, validation, or YAML errors.
    pub fn to_yaml_string(&self) -> Result<String, WyrdError> {
        serde_yaml::to_string(&self.to_card()?)
            .map_err(|error| WyrdError::from(AgentCardError::yaml(&error)))
    }

    /// Convert this holder into a typed Agent Card envelope.
    ///
    /// # Errors
    /// Returns missing identity or invalid identity errors.
    pub fn to_card(&self) -> Result<AgentCard, WyrdError> {
        let name = self.meta.name.clone().ok_or(AgentCardError::MissingName)?;
        let version = self
            .meta
            .version
            .clone()
            .ok_or(AgentCardError::MissingVersion)?;
        let space = self
            .meta
            .space
            .clone()
            .unwrap_or_else(|| "default".to_owned());
        let spec = self.to_spec();
        Ok(AgentCard {
            space,
            name,
            version,
            uid: String::new(),
            labels: self.meta.labels.clone(),
            annotations: self.meta.annotations.clone(),
            cascade_children: derive_cascade_children(&spec),
            spec,
            created_at: chrono::Utc::now(),
        })
    }

    /// Alias for projecting this holder into wire envelope form.
    ///
    /// # Errors
    /// Returns missing identity or invalid identity errors.
    pub fn to_wire(&self) -> Result<AgentCard, WyrdError> {
        self.to_card()
    }

    /// Reconstruct an agent holder from a typed Agent Card envelope.
    ///
    /// # Errors
    /// Returns prompt resolution or tool resolution errors.
    pub fn from_card<R: ToolResolver>(card: AgentCard, resolver: &R) -> Result<Self, WyrdError> {
        let resolved_prompt = resolve_prompt(&card.spec.prompt)?;
        let mut tools = Vec::with_capacity(card.spec.tool_names.len());
        for name in &card.spec.tool_names {
            let tool = resolver.resolve(name).map_err(tool_resolution_error)?;
            tools.push(tool);
        }

        let agent = Agent::new(card.name.clone(), resolved_prompt)
            .with_run_config(run_config_from_agent_run_config_spec(&card.spec.run_config))
            .set_tools(tools);
        Ok(Self {
            agent,
            meta: AgentMetadata {
                name: Some(card.name),
                version: Some(card.version),
                space: Some(card.space),
                labels: card.labels,
                annotations: card.annotations,
            },
            prompt_ref: card.spec.prompt,
            tool_names: card.spec.tool_names,
        })
    }

    /// Alias for reconstructing this holder from wire envelope form.
    ///
    /// # Errors
    /// Returns prompt resolution or tool resolution errors.
    pub fn from_wire<R: ToolResolver>(card: AgentCard, resolver: &R) -> Result<Self, WyrdError> {
        Self::from_card(card, resolver)
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

    /// Return a copy with a card name.
    #[must_use]
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.meta.name = Some(name.into());
        self
    }

    /// Return a copy with a card version.
    #[must_use]
    pub fn with_version(mut self, version: impl Into<String>) -> Self {
        self.meta.version = Some(version.into());
        self
    }

    /// Return a copy with a card space.
    #[must_use]
    pub fn with_space(mut self, space: impl Into<String>) -> Self {
        self.meta.space = Some(space.into());
        self
    }

    /// Return a copy with labels.
    #[must_use]
    pub fn with_labels(mut self, labels: Labels) -> Self {
        self.meta.labels = labels;
        self
    }

    /// Return a copy with annotations.
    #[must_use]
    pub fn with_annotations(mut self, annotations: Annotations) -> Self {
        self.meta.annotations = annotations;
        self
    }

    /// Return a copy with a run configuration.
    #[must_use]
    pub fn with_run_config(mut self, run_config: RunConfig) -> Self {
        self.agent = self.agent.with_run_config(run_config);
        self
    }

    /// Set the run configuration in place.
    pub fn set_run_config(&mut self, run_config: RunConfig) {
        self.agent = self.agent.clone().with_run_config(run_config);
    }

    /// Return a copy with a session memory backend.
    #[must_use]
    pub fn with_session(mut self, session: Arc<dyn SessionMemory>) -> Self {
        self.agent = self.agent.with_session(session);
        self
    }

    /// Set the session memory backend in place.
    pub fn set_session(&mut self, session: Arc<dyn SessionMemory>) {
        self.agent = self.agent.clone().with_session(session);
    }

    /// Return a copy with a run journal backend.
    #[must_use]
    pub fn with_journal(mut self, journal: Arc<dyn Journal>) -> Self {
        self.agent = self.agent.with_journal(journal);
        self
    }

    /// Set the run journal backend in place.
    pub fn set_journal(&mut self, journal: Arc<dyn Journal>) {
        self.agent = self.agent.clone().with_journal(journal);
    }

    /// Add a before-agent callback in place.
    pub fn add_before_agent_in_place(&mut self, callback: BeforeAgentFn) {
        self.agent = self.agent.clone().before_agent(callback);
    }

    /// Add an after-agent callback in place.
    pub fn add_after_agent_in_place(&mut self, callback: AfterAgentFn) {
        self.agent = self.agent.clone().after_agent(callback);
    }

    /// Add a before-model callback in place.
    pub fn add_before_model_in_place(&mut self, callback: BeforeModelFn) {
        self.agent = self.agent.clone().before_model(callback);
    }

    /// Add an after-model callback in place.
    pub fn add_after_model_in_place(&mut self, callback: AfterModelFn) {
        self.agent = self.agent.clone().after_model(callback);
    }

    /// Add a before-tool callback in place.
    pub fn add_before_tool_in_place(&mut self, callback: BeforeToolFn) {
        self.agent = self.agent.clone().before_tool(callback);
    }

    /// Add an after-tool callback in place.
    pub fn add_after_tool_in_place(&mut self, callback: AfterToolFn) {
        self.agent = self.agent.clone().after_tool(callback);
    }

    /// Return a copy with an inline prompt.
    #[must_use]
    pub fn with_prompt(mut self, prompt: Prompt) -> Self {
        self.with_prompt_in_place(prompt);
        self
    }

    /// Set an inline prompt in place.
    pub fn with_prompt_in_place(&mut self, prompt: Prompt) {
        self.agent = self.agent.clone().with_prompt(Arc::new(prompt.clone()));
        self.prompt_ref = PromptRef::from(prompt.into_native());
    }

    /// Resolve and set an inline or card prompt.
    ///
    /// # Errors
    /// Returns `WYRD_AGENT_404_PROMPT_CARD` for missing carded prompts.
    pub fn try_with_prompt(mut self, prompt: impl Into<PromptRef>) -> Result<Self, WyrdError> {
        let prompt_ref = prompt.into();
        self.agent = self.agent.with_prompt(resolve_prompt(&prompt_ref)?);
        self.prompt_ref = prompt_ref;
        Ok(self)
    }

    /// Return a copy with an appended runtime-local tool.
    #[must_use]
    pub fn add_tool(mut self, tool: Arc<dyn AgentTool>) -> Self {
        self.add_tool_in_place(tool);
        self
    }

    /// Append a runtime-local tool in place.
    pub fn add_tool_in_place(&mut self, tool: Arc<dyn AgentTool>) {
        self.tool_names.push(tool.name().to_owned());
        self.agent = self.agent.clone().add_tool(tool);
    }

    /// Replace all runtime-local tools in place.
    pub fn set_tools_in_place(&mut self, tools: Vec<Arc<dyn AgentTool>>) {
        self.tool_names = tools.iter().map(|tool| tool.name().to_owned()).collect();
        self.agent = self.agent.clone().set_tools(tools);
    }

    /// Return the inner agent in an `Arc`.
    #[must_use]
    pub fn agent_arc(&self) -> Arc<Agent> {
        Arc::new(self.agent.clone())
    }

    /// Run this agent through the resolved Skald engine.
    ///
    /// # Errors
    /// Returns Skald agent runtime errors.
    pub async fn run(
        &self,
        providers: &ProviderRegistry,
        session_id: Option<SessionId>,
        input: &str,
    ) -> Result<AgentRun, AgentError> {
        self.agent.run(providers, session_id, input).await
    }

    fn to_spec(&self) -> AgentSpec {
        AgentSpec {
            prompt: self.prompt_ref.clone(),
            tool_names: self.tool_names.clone(),
            run_config: agent_run_config_spec_from_run_config(&self.agent.run_config),
        }
    }
}

/// Fluent builder for `AgentWithMeta`.
#[derive(Default)]
pub struct AgentBuilder {
    id: Option<String>,
    meta: AgentMetadata,
    prompt: Option<PromptRef>,
    tool_names: Vec<String>,
    tools_resolved: Vec<Arc<dyn AgentTool>>,
    run_config: RunConfig,
    session: Option<Arc<dyn SessionMemory>>,
    journal: Option<Arc<dyn Journal>>,
    before_agent: Vec<BeforeAgentFn>,
    after_agent: Vec<AfterAgentFn>,
    before_model: Vec<BeforeModelFn>,
    after_model: Vec<AfterModelFn>,
    before_tool: Vec<BeforeToolFn>,
    after_tool: Vec<AfterToolFn>,
}

impl AgentBuilder {
    /// Set the Skald runtime id.
    #[must_use]
    pub fn id(mut self, id: impl Into<String>) -> Self {
        self.id = Some(id.into());
        self
    }

    /// Set the card name.
    #[must_use]
    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.meta.name = Some(name.into());
        self
    }

    /// Set the card version.
    #[must_use]
    pub fn version(mut self, version: impl Into<String>) -> Self {
        self.meta.version = Some(version.into());
        self
    }

    /// Set the card space.
    #[must_use]
    pub fn space(mut self, space: impl Into<String>) -> Self {
        self.meta.space = Some(space.into());
        self
    }

    /// Set labels.
    #[must_use]
    pub fn labels(mut self, labels: Labels) -> Self {
        self.meta.labels = labels;
        self
    }

    /// Set annotations.
    #[must_use]
    pub fn annotations(mut self, annotations: Annotations) -> Self {
        self.meta.annotations = annotations;
        self
    }

    /// Set the prompt reference.
    #[must_use]
    pub fn prompt(mut self, prompt: impl Into<PromptRef>) -> Self {
        self.prompt = Some(prompt.into());
        self
    }

    /// Append a runtime-local tool.
    #[must_use]
    pub fn tool(mut self, tool: Arc<dyn AgentTool>) -> Self {
        self.tool_names.push(tool.name().to_owned());
        self.tools_resolved.push(tool);
        self
    }

    /// Append multiple runtime-local tools.
    #[must_use]
    pub fn tools<I>(mut self, tools: I) -> Self
    where
        I: IntoIterator<Item = Arc<dyn AgentTool>>,
    {
        for tool in tools {
            self.tool_names.push(tool.name().to_owned());
            self.tools_resolved.push(tool);
        }
        self
    }

    /// Set the run configuration.
    #[must_use]
    pub fn run_config(mut self, run_config: RunConfig) -> Self {
        self.run_config = run_config;
        self
    }

    /// Set the session memory backend.
    #[must_use]
    pub fn session(mut self, session: Arc<dyn SessionMemory>) -> Self {
        self.session = Some(session);
        self
    }

    /// Set the run journal backend.
    #[must_use]
    pub fn journal(mut self, journal: Arc<dyn Journal>) -> Self {
        self.journal = Some(journal);
        self
    }

    /// Append a before-agent callback.
    #[must_use]
    pub fn before_agent(mut self, callback: BeforeAgentFn) -> Self {
        self.before_agent.push(callback);
        self
    }

    /// Append an after-agent callback.
    #[must_use]
    pub fn after_agent(mut self, callback: AfterAgentFn) -> Self {
        self.after_agent.push(callback);
        self
    }

    /// Append a before-model callback.
    #[must_use]
    pub fn before_model(mut self, callback: BeforeModelFn) -> Self {
        self.before_model.push(callback);
        self
    }

    /// Append an after-model callback.
    #[must_use]
    pub fn after_model(mut self, callback: AfterModelFn) -> Self {
        self.after_model.push(callback);
        self
    }

    /// Append a before-tool callback.
    #[must_use]
    pub fn before_tool(mut self, callback: BeforeToolFn) -> Self {
        self.before_tool.push(callback);
        self
    }

    /// Append an after-tool callback.
    #[must_use]
    pub fn after_tool(mut self, callback: AfterToolFn) -> Self {
        self.after_tool.push(callback);
        self
    }

    /// Build an Agent holder.
    ///
    /// # Errors
    /// Returns missing prompt or prompt-card resolution errors.
    pub fn build(self) -> Result<AgentWithMeta, WyrdError> {
        let prompt_ref = self.prompt.ok_or(AgentCardError::MissingPrompt)?;
        let resolved_prompt = resolve_prompt(&prompt_ref)?;
        let id = self
            .id
            .or_else(|| self.meta.name.clone())
            .unwrap_or_else(generate_agent_id);
        let mut agent = Agent::new(id, resolved_prompt).with_run_config(self.run_config);
        if let Some(session) = self.session {
            agent = agent.with_session(session);
        }
        if let Some(journal) = self.journal {
            agent = agent.with_journal(journal);
        }
        for callback in self.before_agent {
            agent = agent.before_agent(callback);
        }
        for callback in self.after_agent {
            agent = agent.after_agent(callback);
        }
        for callback in self.before_model {
            agent = agent.before_model(callback);
        }
        for callback in self.after_model {
            agent = agent.after_model(callback);
        }
        for callback in self.before_tool {
            agent = agent.before_tool(callback);
        }
        for callback in self.after_tool {
            agent = agent.after_tool(callback);
        }
        agent = agent.set_tools(self.tools_resolved);
        Ok(AgentWithMeta {
            agent,
            meta: self.meta,
            prompt_ref,
            tool_names: self.tool_names,
        })
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

/// Register a prompt in the local Agent Card prompt resolver.
///
/// # Errors
/// Returns validation errors if the reference is not a Prompt Card reference.
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

/// Remove all local Agent Card prompt registry entries.
pub fn clear_prompt_card_registry() {
    if let Ok(mut registry) = prompt_registry().write() {
        registry.clear();
    }
}

/// Derive `CardRef` cascade children from an Agent spec.
#[must_use]
pub fn derive_cascade_children(spec: &AgentSpec) -> Vec<CardRef> {
    match &spec.prompt {
        PromptRef::Card(card_ref) => vec![card_ref.clone()],
        PromptRef::Inline(_) => Vec::new(),
    }
}

fn resolve_prompt(prompt_ref: &PromptRef) -> Result<Arc<Prompt>, WyrdError> {
    match prompt_ref {
        PromptRef::Inline(prompt) => Ok(Arc::new(Prompt::from_native((**prompt).clone()))),
        PromptRef::Card(card_ref) => prompt_registry()
            .read()
            .expect("Agent prompt registry lock poisoned")
            .get(&prompt_key(card_ref))
            .map(Arc::clone)
            .ok_or_else(|| {
                AgentCardError::PromptCardNotFound {
                    card_ref: card_ref.clone(),
                }
                .into()
            }),
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
