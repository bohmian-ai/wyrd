//! Typed in-memory Card authoring over the canonical `wyrd-spec` contracts.
//!
//! A definition is an ordinary value: [`Service::new`], [`Verifier::new`],
//! [`Agent::new`], and [`CardDefinition::new`] create one, and fallible
//! inherent methods such as [`Verifier::add_step`] edit it in place. Every
//! value is backed by the canonical spec type it lowers to, so nothing here
//! is a second contract. Constructors and edits never touch the network;
//! [`CodeCall::from_file`] reads one local source file.
//!
//! Adding a child captures its value: editing the original afterwards
//! changes nothing already added. A definition remembers the Cards and local
//! Artifacts it references so the containing graph can be declared as one
//! value. [`CardDefinition::declaration`] snapshots that graph and passes it
//! through the loader's ordinary validation and ordering; registration then
//! goes through [`crate::cards::Cards::declare_authored`] or
//! [`crate::state::WyrdState::declare_authored`], which publish each local
//! Artifact first through [`crate::cards::Cards::upload_artifact`] and then
//! declare the graph through [`crate::cards::Cards::declare`].
//!
//! Every authored Card carries an exact version pin, so references between
//! authored Cards are exact sibling references and references to a local or
//! registered Artifact are exact external references.

#![deny(missing_docs)]

mod verifier;

use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use wyrd_loader::{AuthoredCard, DeclarationInput};
use wyrd_semver::VersionBlock;
use wyrd_spec::api_version::ApiVersion;
use wyrd_spec::card::agent::{AgentRunConfigSpec, AgentSpec, ExecutionRoute};
use wyrd_spec::card::artifact::ArtifactSpec;
use wyrd_spec::card::operator::OperatorSpec;
use wyrd_spec::card::prompt::PromptSpec;
use wyrd_spec::card::service::{ServiceComponent, ServiceSpec};
use wyrd_spec::card::trigger::TriggerSpec;
use wyrd_spec::card::verifier::{ArtifactSelection, VerificationBinding, VerifierSpec};
use wyrd_spec::envelope::{CardKind, Metadata, Spec};
use wyrd_spec::error::WyrdError;
use wyrd_spec::ids::{CardName, ProviderDeploymentName};
use wyrd_spec::reference::{CardRef, InlineableRef, Ref};
use wyrd_spec::registry::RelativeArtifactPath;

pub use skald_spec::MediaFormat;
pub use verifier::{Action, AgentCall, Check, CodeCall, Requirement, Step};
pub use wyrd_spec::card::agent::BuiltinTool;
pub use wyrd_spec::card::drift::DriftSpec;
pub use wyrd_spec::card::verifier::{
    CodeRuntime, CodeSource, Condition, Decide, InputSlot, OrderBy, Reference, ReferenceError,
    VerdictField, WorkspaceSource,
};
pub use wyrd_spec::verification::{ComparisonOperator, ContextCapture, ExecutionPlan};

/// The Cards and local Artifacts an authored value references, carried with
/// it until its containing graph is declared.
///
/// Durable specs never hold local source metadata, so this list is how a
/// reference to a sibling Card or a local Artifact keeps its definition.
#[derive(Debug, Clone, Default, PartialEq)]
struct Dependencies {
    /// Referenced Card definitions, each with its own dependencies.
    cards: Vec<CardDefinition>,
    /// Referenced Artifacts that publish from a local source.
    artifacts: Vec<Artifact>,
}

impl Dependencies {
    /// Take over every dependency of `other`.
    fn absorb(&mut self, other: Self) {
        self.cards.extend(other.cards);
        self.artifacts.extend(other.artifacts);
    }
}

/// One editable Card definition: exact identity, a typed spec, and the
/// definitions it references.
///
/// [`Agent`], [`Verifier`], and [`Service`] are this type over their spec;
/// [`CardDefinition`] holds any registrable kind's [`Spec`], so every other
/// kind is authored through its canonical spec without a wire envelope.
#[derive(Debug, Clone, PartialEq)]
pub struct Definition<S> {
    /// The Card kind, which `spec` determines.
    kind: CardKind,
    /// Exact-pin metadata; never carries a bump or UID.
    metadata: Metadata,
    /// The canonical spec this definition declares.
    spec: S,
    /// Definitions and local Artifacts the spec references.
    dependencies: Dependencies,
}

/// A Card of any registrable kind, authored through its canonical spec.
pub type CardDefinition = Definition<Spec>;
/// An Agent Card definition.
pub type Agent = Definition<AgentSpec>;
/// A Verifier Card definition.
pub type Verifier = Definition<VerifierSpec>;
/// A Service Card definition.
pub type Service = Definition<ServiceSpec>;

impl<S> Definition<S> {
    /// The exact reference this definition registers as, without a UID.
    #[must_use]
    pub fn card_ref(&self) -> CardRef {
        CardRef {
            kind: self.kind.clone(),
            name: self.metadata.name.clone(),
            version: self
                .metadata
                .resolved_pin()
                .expect("invariant: authored definitions hold an exact version pin")
                .clone(),
            uid: None,
        }
    }

    /// The canonical metadata this definition declares.
    #[must_use]
    pub fn metadata(&self) -> &Metadata {
        &self.metadata
    }

    /// The canonical spec this definition declares.
    #[must_use]
    pub fn spec(&self) -> &S {
        &self.spec
    }

    /// Build a definition of `kind` over `spec` with exact `metadata`.
    fn with_metadata(kind: CardKind, metadata: Metadata, spec: S) -> Self {
        Self {
            kind,
            metadata,
            spec,
            dependencies: Dependencies::default(),
        }
    }
}

impl CardDefinition {
    /// Define a Card of `spec`'s kind named `name` at exact `version`.
    ///
    /// # Errors
    /// Returns [`WyrdError::Validation`] for an invalid name and
    /// [`WyrdError::RegistryInvalidVersionBlock`] for a version that is not an
    /// exact semantic version.
    pub fn new(name: &str, version: &str, spec: Spec) -> Result<Self, WyrdError> {
        Ok(Self::with_metadata(
            spec.kind(),
            exact_metadata(name, version)?,
            spec,
        ))
    }

    /// Define a Card from complete canonical `metadata`, including labels and
    /// annotations.
    ///
    /// # Errors
    /// Returns [`WyrdError::RegistryInvalidVersionBlock`] unless `metadata`
    /// holds an exact version pin, and [`WyrdError::Validation`] when it
    /// carries a bump or a server-managed UID, hash, or origin.
    pub fn from_metadata(metadata: Metadata, spec: Spec) -> Result<Self, WyrdError> {
        if metadata.resolved_pin().is_none() || metadata.bump.is_some() {
            return Err(WyrdError::RegistryInvalidVersionBlock {
                message: "metadata.version: an authored definition needs an exact version pin"
                    .to_owned(),
                details: json!({ "path": "metadata.version" }),
            });
        }
        if metadata.uid.is_some()
            || metadata.spec_hash.is_some()
            || metadata.artifact_hash.is_some()
            || metadata.origin.is_some()
        {
            return Err(invalid(
                "server_managed_metadata",
                "metadata",
                "uid, spec_hash, artifact_hash, and origin are server-managed",
            ));
        }
        Ok(Self::with_metadata(spec.kind(), metadata, spec))
    }

    /// Snapshot this Card's graph as its declaration: the local Artifacts to
    /// publish first, in reference order, and the containing graph's input.
    ///
    /// Every referenced definition joins the graph once; an equal definition
    /// referenced twice is one submission. The graph passes the same
    /// validation and ordering a file-authored tree does
    /// ([`wyrd_loader::declaration_input_from_cards`]), and references each
    /// local Artifact by its exact identity, so the declaration does not
    /// depend on publication. No local path enters a submission.
    ///
    /// # Errors
    /// Returns [`WyrdError::SpecDuplicateSubmission`] when two different
    /// definitions, or two different local Artifacts, share one identity,
    /// and `WYRD_LOADER_400_INVALID_ENVELOPE` carrying the shared validation
    /// diagnostics for an invalid graph.
    pub fn declaration(&self) -> Result<Declaration, WyrdError> {
        let mut graph = Graph::default();
        graph.add(self.clone())?;
        let authored: Vec<AuthoredCard> = graph
            .cards
            .into_iter()
            .map(|card| AuthoredCard {
                source_path: PathBuf::from(card.card_ref().to_string()),
                api_version: ApiVersion::v1(),
                kind: card.kind,
                metadata: card.metadata,
                spec: card.spec,
                artifacts: Vec::new(),
            })
            .collect();
        Ok(Declaration {
            artifacts: graph.artifacts,
            input: wyrd_loader::declaration_input_from_cards(&authored)?,
        })
    }
}

/// A Card graph's declaration: local Artifacts published first, each as its
/// own sole submission, then the containing graph.
#[derive(Debug, Clone, PartialEq)]
pub struct Declaration {
    /// Local Artifacts, in reference order, each published before the graph.
    pub artifacts: Vec<Artifact>,
    /// The containing graph, in dependency-first order with no local sources.
    pub input: DeclarationInput,
}

/// The distinct definitions and local Artifacts reachable from one root.
#[derive(Default)]
struct Graph {
    /// Definitions without their dependencies, dependencies first.
    cards: Vec<CardDefinition>,
    /// Distinct local Artifacts, in reference order.
    artifacts: Vec<Artifact>,
}

impl Graph {
    /// Add `card` and, before it, everything it references.
    ///
    /// # Errors
    /// Returns [`WyrdError::SpecDuplicateSubmission`] for a different
    /// definition of an identity already added.
    fn add(&mut self, mut card: CardDefinition) -> Result<(), WyrdError> {
        let dependencies = std::mem::take(&mut card.dependencies);
        for artifact in dependencies.artifacts {
            match self
                .artifacts
                .iter()
                .find(|known| known.card_ref() == artifact.card_ref())
            {
                Some(known) if *known == artifact => {}
                Some(_) => return Err(conflict(&artifact.card_ref())),
                None => self.artifacts.push(artifact),
            }
        }
        for child in dependencies.cards {
            self.add(child)?;
        }
        match self
            .cards
            .iter()
            .find(|known| known.card_ref() == card.card_ref())
        {
            Some(known) if *known == card => Ok(()),
            Some(_) => Err(conflict(&card.card_ref())),
            None => {
                self.cards.push(card);
                Ok(())
            }
        }
    }
}

/// The refusal of two different definitions of `card_ref`.
fn conflict(card_ref: &CardRef) -> WyrdError {
    WyrdError::SpecDuplicateSubmission {
        message: format!("{card_ref} is defined twice with different content"),
        details: json!({ "card_ref": card_ref.to_string() }),
    }
}

impl From<Agent> for CardDefinition {
    fn from(agent: Agent) -> Self {
        Self {
            kind: agent.kind,
            metadata: agent.metadata,
            spec: Spec::Agent(agent.spec),
            dependencies: agent.dependencies,
        }
    }
}

impl From<Verifier> for CardDefinition {
    fn from(verifier: Verifier) -> Self {
        Self {
            kind: verifier.kind,
            metadata: verifier.metadata,
            spec: Spec::Verifier(verifier.spec),
            dependencies: verifier.dependencies,
        }
    }
}

impl From<Service> for CardDefinition {
    fn from(service: Service) -> Self {
        Self {
            kind: service.kind,
            metadata: service.metadata,
            spec: Spec::Service(service.spec),
            dependencies: service.dependencies,
        }
    }
}

/// One inlineable reference position, as one of its three authoring intents:
/// an embedded spec, a sibling definition declared in the same graph, or an
/// already registered Card.
#[derive(Debug, Clone, PartialEq)]
pub struct Linked<T> {
    /// The canonical reference value.
    reference: InlineableRef<T>,
    /// The sibling definition, when there is one.
    dependencies: Dependencies,
}

impl<T> Linked<T> {
    /// Embed `spec` in the referencing Card.
    #[must_use]
    pub fn inline(spec: T) -> Self {
        Self {
            reference: InlineableRef::Inline(Box::new(spec)),
            dependencies: Dependencies::default(),
        }
    }

    /// Reference `card` as a sibling declared in the same graph.
    #[must_use]
    pub fn sibling(card: impl Into<CardDefinition>) -> Self {
        let card = card.into();
        Self {
            reference: InlineableRef::Sibling {
                sibling: card.card_ref(),
            },
            dependencies: Dependencies {
                cards: vec![card],
                artifacts: Vec::new(),
            },
        }
    }

    /// Reference an already registered Card by its exact identity.
    #[must_use]
    pub fn registered(card_ref: CardRef) -> Self {
        Self {
            reference: InlineableRef::Ref(identity(card_ref)),
            dependencies: Dependencies::default(),
        }
    }
}

impl<T> From<CardRef> for Linked<T> {
    fn from(card_ref: CardRef) -> Self {
        Self::registered(card_ref)
    }
}

impl<T> From<CardDefinition> for Linked<T> {
    fn from(card: CardDefinition) -> Self {
        Self::sibling(card)
    }
}

impl From<Agent> for Linked<AgentSpec> {
    fn from(agent: Agent) -> Self {
        Self::sibling(agent)
    }
}

impl From<AgentSpec> for Linked<AgentSpec> {
    fn from(spec: AgentSpec) -> Self {
        Self::inline(spec)
    }
}

impl From<Prompt> for Linked<skald_spec::Prompt> {
    fn from(prompt: Prompt) -> Self {
        Self::inline(prompt.0)
    }
}

impl From<TriggerSpec> for Linked<TriggerSpec> {
    fn from(spec: TriggerSpec) -> Self {
        Self::inline(spec)
    }
}

impl From<OperatorSpec> for Linked<OperatorSpec> {
    fn from(spec: OperatorSpec) -> Self {
        Self::inline(spec)
    }
}

/// One `verified_by` binding of a subject to a Verifier, with its optional
/// Trigger and Operators.
#[derive(Debug, Clone, PartialEq)]
pub struct Binding {
    /// The canonical binding.
    binding: VerificationBinding,
    /// The bound Verifier, Trigger, and Operator definitions.
    dependencies: Dependencies,
}

impl Binding {
    /// Bind `verifier`, declared in the same graph; nothing activates it
    /// until a Trigger is set.
    #[must_use]
    pub fn new(verifier: Verifier) -> Self {
        let verifier_ref = verifier.card_ref();
        Self {
            binding: binding(Ref::Sibling {
                sibling: verifier_ref,
            }),
            dependencies: Dependencies {
                cards: vec![verifier.into()],
                artifacts: Vec::new(),
            },
        }
    }

    /// Bind an already registered Verifier by its exact identity.
    #[must_use]
    pub fn registered(verifier: CardRef) -> Self {
        Self {
            binding: binding(Ref::Ref(identity(verifier))),
            dependencies: Dependencies::default(),
        }
    }

    /// Run the bound Verifier when `trigger` fires.
    pub fn set_runs_on(&mut self, trigger: impl Into<Linked<TriggerSpec>>) {
        let trigger = trigger.into();
        self.binding.runs_on = Some(trigger.reference);
        self.dependencies.absorb(trigger.dependencies);
    }

    /// Dispatch `operator` after a failed verdict.
    pub fn add_on_failure(&mut self, operator: impl Into<Linked<OperatorSpec>>) {
        let operator = operator.into();
        self.binding.on_failure.push(operator.reference);
        self.dependencies.absorb(operator.dependencies);
    }

    /// Dispatch `operator` when a settled group verdict changes.
    pub fn add_on_verdict_change(&mut self, operator: impl Into<Linked<OperatorSpec>>) {
        let operator = operator.into();
        self.binding.on_verdict_change.push(operator.reference);
        self.dependencies.absorb(operator.dependencies);
    }

    /// The canonical binding.
    #[must_use]
    pub fn binding(&self) -> &VerificationBinding {
        &self.binding
    }
}

impl From<Verifier> for Binding {
    fn from(verifier: Verifier) -> Self {
        Self::new(verifier)
    }
}

/// An explicit-only binding to `verifier`.
fn binding(verifier: Ref) -> VerificationBinding {
    VerificationBinding {
        verifier,
        runs_on: None,
        on_failure: Vec::new(),
        on_verdict_change: Vec::new(),
    }
}

/// A Prompt, compiled from its authored form exactly as a Prompt Card is.
#[derive(Debug, Clone, PartialEq)]
pub struct Prompt(skald_spec::Prompt);

impl Prompt {
    /// A portable Prompt sending `text` as its one user message to `model`
    /// of `provider`; `{{name}}` placeholders become its variables.
    ///
    /// # Errors
    /// Returns the Prompt compiler's refusal, such as an unknown provider.
    pub fn new(provider: &str, model: &str, text: &str) -> Result<Self, WyrdError> {
        Self::from_authored(json!({
            "provider": provider,
            "model": model,
            "messages": [{ "role": "user", "content": text }],
        }))
    }

    /// Compile any authored Prompt form: portable or native, with system
    /// text, settings, and a response format.
    ///
    /// # Errors
    /// Returns [`WyrdError::Validation`] for a value that is not an authored
    /// Prompt, and the Prompt compiler's refusal for one that does not
    /// compile.
    pub fn from_authored(authored: Value) -> Result<Self, WyrdError> {
        let authored: skald_spec::AuthoredPrompt =
            serde_json::from_value(authored).map_err(|error| invalid("prompt", "prompt", error))?;
        Ok(Self(wyrd_spec::card::prompt::compile_prompt(&authored)?))
    }

    /// Replace the response format with `format`: a named format such as
    /// `verdict` or a JSON Schema object.
    ///
    /// # Errors
    /// Returns the Prompt compiler's refusal for a format it cannot resolve.
    pub fn set_response_format(&mut self, format: Value) -> Result<(), WyrdError> {
        let mut authored = serde_json::to_value(self.0.authored()?.as_ref())
            .map_err(|error| invalid("prompt", "prompt", error))?;
        authored["response_format"] = format;
        *self = Self::from_authored(authored)?;
        Ok(())
    }

    /// Define this Prompt as a Prompt Card named `name` at exact `version`.
    ///
    /// # Errors
    /// Returns Prompt Card validation and identity errors.
    pub fn to_card(&self, name: &str, version: &str) -> Result<CardDefinition, WyrdError> {
        CardDefinition::new(
            name,
            version,
            Spec::Prompt(PromptSpec::new(self.0.clone())?),
        )
    }

    /// The compiled canonical Prompt.
    #[must_use]
    pub fn prompt(&self) -> &skald_spec::Prompt {
        &self.0
    }
}

impl Agent {
    /// Define Agent `name` at exact `version` that runs `prompt` through the
    /// executing host's native provider until a route is set.
    ///
    /// # Errors
    /// Returns identity errors as [`CardDefinition::new`] does.
    pub fn new(
        name: &str,
        version: &str,
        prompt: impl Into<Linked<skald_spec::Prompt>>,
    ) -> Result<Self, WyrdError> {
        let prompt = prompt.into();
        let mut agent = Self::with_metadata(
            CardKind::Agent,
            exact_metadata(name, version)?,
            AgentSpec {
                prompt: prompt.reference,
                execution_route: ExecutionRoute::Native {},
                tool_names: Vec::new(),
                run_config: AgentRunConfigSpec::default(),
                verified_by: Vec::new(),
            },
        );
        agent.dependencies = prompt.dependencies;
        Ok(agent)
    }

    /// Route model calls through the tenant gateway's `deployment`.
    ///
    /// # Errors
    /// Returns [`WyrdError::Validation`] for an invalid deployment name.
    pub fn set_gateway_route(&mut self, deployment: &str) -> Result<(), WyrdError> {
        let deployment = ProviderDeploymentName::new(deployment)
            .map_err(|error| invalid("deployment", "spec.execution_route.deployment", error))?;
        self.spec.execution_route = ExecutionRoute::WyrdGateway { deployment };
        Ok(())
    }

    /// Route model calls through the executing host's native provider.
    pub fn set_native_route(&mut self) {
        self.spec.execution_route = ExecutionRoute::Native {};
    }

    /// Offer exactly `tools`, such as [`BuiltinTool::FsRead`]'s `fs.read`.
    pub fn set_tools<I, T>(&mut self, tools: I)
    where
        I: IntoIterator<Item = T>,
        T: Into<String>,
    {
        self.spec.tool_names = tools.into_iter().map(Into::into).collect();
    }

    /// Stop each run after `iterations` model turns.
    pub fn set_max_iterations(&mut self, iterations: u32) {
        self.spec.run_config.max_iterations = Some(iterations);
    }

    /// Replace the whole run configuration.
    pub fn set_run_config(&mut self, config: AgentRunConfigSpec) {
        self.spec.run_config = config;
    }

    /// Bind a Verifier to this standalone Agent.
    pub fn add_verifier(&mut self, binding: impl Into<Binding>) {
        let binding = binding.into();
        self.spec.verified_by.push(binding.binding);
        self.dependencies.absorb(binding.dependencies);
    }
}

impl Service {
    /// Define Service `name` at exact `version` with no components.
    ///
    /// # Errors
    /// Returns identity errors as [`CardDefinition::new`] does.
    pub fn new(name: &str, version: &str) -> Result<Self, WyrdError> {
        Ok(Self::with_metadata(
            CardKind::Service,
            exact_metadata(name, version)?,
            ServiceSpec::default(),
        ))
    }

    /// Set the Service's human-readable description.
    pub fn set_description(&mut self, description: &str) {
        self.spec.description = Some(description.to_owned());
    }

    /// Set the Service type, such as `api`.
    pub fn set_service_type(&mut self, service_type: &str) {
        self.spec.service_type = Some(service_type.to_owned());
    }

    /// Compose `card`, declared in the same graph, under `alias`, verified by
    /// `verified_by`.
    ///
    /// # Errors
    /// Returns [`WyrdError::Validation`] with rule `duplicate_alias` when
    /// `alias` is already composed.
    pub fn add_component<B: Into<Binding>>(
        &mut self,
        alias: &str,
        card: impl Into<CardDefinition>,
        verified_by: impl IntoIterator<Item = B>,
    ) -> Result<(), WyrdError> {
        let card = card.into();
        let sibling = Ref::Sibling {
            sibling: card.card_ref(),
        };
        self.compose(alias, sibling, verified_by)?;
        self.dependencies.cards.push(card);
        Ok(())
    }

    /// Compose an already registered Card under `alias`, verified by
    /// `verified_by`.
    ///
    /// # Errors
    /// As [`Self::add_component`].
    pub fn add_registered_component<B: Into<Binding>>(
        &mut self,
        alias: &str,
        card: CardRef,
        verified_by: impl IntoIterator<Item = B>,
    ) -> Result<(), WyrdError> {
        self.compose(alias, Ref::Ref(identity(card)), verified_by)
    }

    /// Bind a Verifier to the Service as a whole.
    pub fn add_verifier(&mut self, binding: impl Into<Binding>) {
        let binding = binding.into();
        self.spec.verified_by.push(binding.binding);
        self.dependencies.absorb(binding.dependencies);
    }

    /// Append the component `alias` referencing `card_ref`.
    ///
    /// # Errors
    /// Returns rule `duplicate_alias` for an alias already composed.
    fn compose<B: Into<Binding>>(
        &mut self,
        alias: &str,
        card_ref: Ref,
        verified_by: impl IntoIterator<Item = B>,
    ) -> Result<(), WyrdError> {
        if self
            .spec
            .components
            .iter()
            .any(|component| component.alias == alias)
        {
            return Err(invalid(
                "duplicate_alias",
                &format!("spec.components.{alias}"),
                "component aliases must be unique within a Service",
            ));
        }
        let mut component = ServiceComponent {
            alias: alias.to_owned(),
            card_ref,
            verified_by: Vec::new(),
            source: None,
            config: std::collections::BTreeMap::new(),
            credential_refs: Vec::new(),
        };
        for binding in verified_by {
            let binding = binding.into();
            component.verified_by.push(binding.binding);
            self.dependencies.absorb(binding.dependencies);
        }
        self.spec.components.push(component);
        Ok(())
    }
}

/// An Artifact Card: a local file or directory published before the graph
/// that references it, or an already registered Artifact used as is.
#[derive(Debug, Clone, PartialEq)]
pub struct Artifact {
    /// Exact-pin metadata.
    metadata: Metadata,
    /// The Artifact spec published with a local source.
    spec: ArtifactSpec,
    /// The local source, or `None` for a registered Artifact.
    source: Option<LocalSource>,
}

/// The local source of an Artifact still to be published.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalSource {
    /// The regular file or directory to publish.
    pub path: PathBuf,
    /// Whether supported documents are extracted beside their originals.
    pub extract: bool,
}

impl Artifact {
    /// Define Artifact `name` at exact `version` published from the file or
    /// directory at `path`, extracting supported documents when `extract`
    /// is set. Nothing is read until the Artifact is published.
    ///
    /// # Errors
    /// Returns identity errors as [`CardDefinition::new`] does.
    pub fn from_path(
        name: &str,
        version: &str,
        path: impl AsRef<Path>,
        extract: bool,
    ) -> Result<Self, WyrdError> {
        Ok(Self {
            metadata: exact_metadata(name, version)?,
            spec: ArtifactSpec {
                artifact_kind: "file".to_owned(),
                ..ArtifactSpec::default()
            },
            source: Some(LocalSource {
                path: path.as_ref().to_path_buf(),
                extract,
            }),
        })
    }

    /// Use the registered Artifact `card_ref` without preparing or
    /// uploading anything.
    ///
    /// The reference keeps its exact identity; registration pins the UID.
    ///
    /// # Errors
    /// Returns [`WyrdError::Validation`] when `card_ref` is not an Artifact.
    pub fn registered(card_ref: CardRef) -> Result<Self, WyrdError> {
        if card_ref.kind != CardKind::Artifact {
            return Err(invalid(
                "artifact_kind",
                "card",
                format!("{card_ref} is not an Artifact"),
            ));
        }
        let mut metadata = exact_metadata(card_ref.name.as_str(), "0.0.0")?;
        metadata.version = Some(card_ref.version.into());
        Ok(Self {
            metadata,
            spec: ArtifactSpec::default(),
            source: None,
        })
    }

    /// Replace the spec published with a local source.
    ///
    /// # Errors
    /// Returns [`ArtifactSpec::validate`]'s refusal.
    pub fn set_spec(&mut self, spec: ArtifactSpec) -> Result<(), WyrdError> {
        spec.validate()?;
        self.spec = spec;
        Ok(())
    }

    /// The exact reference this Artifact registers as, without a UID.
    #[must_use]
    pub fn card_ref(&self) -> CardRef {
        CardRef {
            kind: CardKind::Artifact,
            name: self.metadata.name.clone(),
            version: self
                .metadata
                .resolved_pin()
                .expect("invariant: authored Artifacts hold an exact version pin")
                .clone(),
            uid: None,
        }
    }

    /// The metadata a local source is published with.
    #[must_use]
    pub fn metadata(&self) -> &Metadata {
        &self.metadata
    }

    /// The spec a local source is published with.
    #[must_use]
    pub fn spec(&self) -> &ArtifactSpec {
        &self.spec
    }

    /// The local source, or `None` for a registered Artifact.
    #[must_use]
    pub fn source(&self) -> Option<&LocalSource> {
        self.source.as_ref()
    }

    /// Select the one fixed file or directory prefix `path` of this exact
    /// Artifact version.
    ///
    /// # Errors
    /// Returns [`WyrdError::Validation`] for a path that is not a relative
    /// Artifact path.
    pub fn file(&self, path: &str) -> Result<ArtifactFile, WyrdError> {
        RelativeArtifactPath::new(path).map_err(|error| invalid("artifact_path", "path", error))?;
        Ok(ArtifactFile {
            artifact: self.clone(),
            path: Some(path.to_owned()),
        })
    }

    /// Select every file of this exact Artifact version.
    #[must_use]
    pub fn files(&self) -> ArtifactFile {
        ArtifactFile {
            artifact: self.clone(),
            path: None,
        }
    }
}

/// Fixed files of one exact Artifact version, mounted into a workspace or
/// run as code; callers cannot replace them through execution input.
#[derive(Debug, Clone, PartialEq)]
pub struct ArtifactFile {
    /// The selected Artifact, kept for publication when it is local.
    artifact: Artifact,
    /// The selected file or directory prefix; `None` is every file.
    path: Option<String>,
}

impl ArtifactFile {
    /// Select `path` of the registered Artifact `card_ref`.
    ///
    /// # Errors
    /// Returns [`Artifact::registered`]'s and [`Artifact::file`]'s refusals.
    pub fn new(card_ref: CardRef, path: &str) -> Result<Self, WyrdError> {
        Artifact::registered(card_ref)?.file(path)
    }

    /// The canonical selection, referencing the Artifact externally by its
    /// exact identity.
    #[must_use]
    pub fn selection(&self) -> ArtifactSelection {
        ArtifactSelection {
            card: Ref::Ref(self.artifact.card_ref()),
            path: self.path.clone(),
        }
    }

    /// The selected Artifact.
    #[must_use]
    pub fn artifact(&self) -> &Artifact {
        &self.artifact
    }

    /// The local Artifact this selection needs published, if any.
    fn dependencies(&self) -> Dependencies {
        Dependencies {
            cards: Vec::new(),
            artifacts: self
                .artifact
                .source
                .is_some()
                .then(|| self.artifact.clone())
                .into_iter()
                .collect(),
        }
    }
}

/// `card_ref` without its UID: authored references carry identity only and
/// registration pins the UID, as for file-authored Cards.
fn identity(mut card_ref: CardRef) -> CardRef {
    card_ref.uid = None;
    card_ref
}

/// Metadata pinning `name` at exact `version`.
///
/// # Errors
/// Returns [`WyrdError::Validation`] for an invalid name and
/// [`WyrdError::RegistryInvalidVersionBlock`] for a non-exact version.
fn exact_metadata(name: &str, version: &str) -> Result<Metadata, WyrdError> {
    let name = CardName::new(name).map_err(|error| invalid("card_name", "metadata.name", error))?;
    let version =
        VersionBlock::parse(version).map_err(|error| WyrdError::RegistryInvalidVersionBlock {
            message: format!("metadata.version: {error}"),
            details: json!({ "path": "metadata.version", "version": version }),
        })?;
    Ok(Metadata {
        name,
        version: Some(version.into()),
        bump: None,
        uid: None,
        labels: std::collections::BTreeMap::new(),
        annotations: std::collections::BTreeMap::new(),
        spec_hash: None,
        artifact_hash: None,
        origin: None,
    })
}

/// The structured refusal of one authoring rule at `path`, in the shape the
/// shared Verifier validation reports.
fn invalid(rule: &str, path: &str, message: impl std::fmt::Display) -> WyrdError {
    WyrdError::Validation {
        message: format!("{path}: {message}"),
        details: json!({ "rule": rule, "path": path }),
    }
}

#[cfg(test)]
mod tests;
