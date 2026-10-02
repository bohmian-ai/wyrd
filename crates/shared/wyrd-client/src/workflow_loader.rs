//! Workflow loading: one composition from an authored bundle or a registered
//! Workflow Card to the Skald runtime.
//!
//! [`WorkflowLoader`] owns the optional registry client and the caller's tool
//! registry. It gathers the exact Agent and Prompt bodies a Workflow names —
//! loader siblings from disk, external and registered references through exact
//! Cards reads — into a [`WorkflowGraph`]. The graph keeps each body under its
//! authored provenance, so a sibling body never satisfies an external `ref`
//! with the same identity. It feeds Skald's existing synchronous resolver seam
//! and runs pure and resolved Workflow validation. Loading never registers,
//! executes, or resolves secrets.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use chrono::Utc;
use skald_agent::{Agent, PromptResolver};
use skald_prompt::Prompt;
use skald_spec::Prompt as NativePrompt;
use skald_tool::{ToolRegistry, ToolResolver};
use skald_workflow::{AgentResolver, Workflow};
use wyrd_spec::api_version::ApiVersion;
use wyrd_spec::card::workflow::{WorkflowAction, WorkflowCard};
use wyrd_spec::envelope::{Card, CardKind, Relationships, Spec};
use wyrd_spec::error::WyrdError;
use wyrd_spec::metadata::{Annotations, Labels};
use wyrd_spec::reference::{CardRef, CardRefIdentity, InlineableRef, Ref};
use wyrd_spec::registry::CardSubmission;
use wyrd_spec::{AgentCard, AgentSpec};

use crate::WyrdClient;
use crate::cards::{CardSelector, Cards};

/// Loads runnable Skald Workflows from authored bundles or the registry.
///
/// The loader owns composition only: it never registers, executes, or reads
/// secrets. Tools are the caller's existing registry and are bound to each
/// Agent's declared `tool_names` during hydration.
#[derive(Clone)]
pub struct WorkflowLoader {
    /// Registry handle for exact reads; absent for offline loading.
    cards: Option<Cards>,
    /// Caller-supplied tools resolved for every Agent's declared names.
    tools: Arc<dyn ToolResolver>,
}

impl WorkflowLoader {
    /// Build an offline loader that binds Agent tools from `tools`.
    #[must_use]
    pub fn new(tools: Arc<dyn ToolResolver>) -> Self {
        Self { cards: None, tools }
    }

    /// Attach a registry client used for external `ref` dependencies and
    /// registered Workflow loading.
    #[must_use]
    pub fn with_client(mut self, client: WyrdClient) -> Self {
        self.cards = Some(Cards::with_client(client));
        self
    }

    /// Load the Workflow Card at `path` and its local dependency bundle.
    ///
    /// The shared loader resolves `path`, `inline`, and sibling dependencies
    /// relative to their containing files and runs pure contract validation;
    /// every loaded Agent and Prompt is recorded as a sibling body. External
    /// `ref` dependencies — even ones naming the same identity as a loaded
    /// sibling — are read exactly from the registry, which requires
    /// [`with_client`](Self::with_client). Nothing is registered.
    ///
    /// Cancellation may stop after completed filesystem or registry reads; no
    /// registration or durable Card write happens on this path.
    ///
    /// # Errors
    /// Returns `WYRD_REGISTRY_400_INVALID_CARD_SPEC` carrying the loader
    /// diagnostics when the bundle fails to load or does not contain exactly
    /// one Workflow Card in `path`; `WYRD_REGISTRY_422_UNRESOLVED_DEPENDENCY`
    /// for an external dependency without a client or one that is not active;
    /// the Cards read error for a missing or mismatched dependency; and the
    /// Workflow validation and hydration errors of [`WorkflowGraph::hydrate`].
    pub async fn load_file(&self, path: &Path) -> Result<Workflow, WyrdError> {
        let tree = wyrd_loader::load(path).map_err(|error| WyrdError::RegistryInvalidCardSpec {
            message: format!("workflow bundle failed to load: {error}"),
            details: serde_json::json!({ "path": path, "diagnostics": error.diagnostics }),
        })?;
        let entry = path.canonicalize().map_err(|error| {
            WyrdError::registry_invalid_card_spec(format!(
                "workflow path {} cannot be resolved: {error}",
                path.display()
            ))
        })?;
        let mut workflows = tree
            .cards
            .iter()
            .filter(|card| card.submission.kind == CardKind::Workflow && card.source_path == entry);
        let (Some(workflow), None) = (workflows.next(), workflows.next()) else {
            return Err(WyrdError::registry_invalid_card_spec(format!(
                "{} must contain exactly one Workflow Card",
                path.display()
            )));
        };
        let mut graph = WorkflowGraph::from_submission(&workflow.submission)?;
        for card in &tree.cards {
            if matches!(card.submission.kind, CardKind::Agent | CardKind::Prompt) {
                let card = submission_card(&card.submission)?;
                let sibling = exact_ref(&card)?;
                graph.insert(Ref::Sibling { sibling }, card.spec)?;
            }
        }
        self.fetch_missing(&mut graph).await?;
        graph.hydrate(self.tools.as_ref())
    }

    /// Fetch a registered Workflow and its locked dependencies for local
    /// execution.
    ///
    /// `workflow` must name a Workflow in an explicit space; its version is
    /// always an exact pin, which the exact read validates. Every Card is
    /// read by its exact identity — the stored dependency references carry
    /// their registered UIDs — so later versions never float in. The returned
    /// Workflow keeps its registered identity for the run snapshot.
    ///
    /// Cancellation may stop after completed registry reads; no registration
    /// or durable Card write happens on this path.
    ///
    /// # Errors
    /// Returns `WYRD_REGISTRY_400_INVALID_CARD_SPEC` for a non-Workflow or
    /// spaceless reference or an invalid version, and
    /// `WYRD_REGISTRY_422_UNRESOLVED_DEPENDENCY` without a client;
    /// `WYRD_REGISTRY_422_UNRESOLVED_DEPENDENCY` for a Card that is not
    /// active; the Cards read error for a missing, foreign, or mismatched
    /// Card; and the errors of [`WorkflowGraph::hydrate`].
    pub async fn load_registered(&self, workflow: &CardRef) -> Result<Workflow, WyrdError> {
        if workflow.kind != CardKind::Workflow || workflow.space.is_none() {
            return Err(WyrdError::registry_invalid_card_spec(
                "registered Workflow loading requires a Workflow reference with a space",
            ));
        }
        let card = self.read(workflow).await?;
        let mut graph = WorkflowGraph::new(WorkflowCard::from_envelope(card)?)?;
        self.fetch_missing(&mut graph).await?;
        graph.hydrate(self.tools.as_ref())
    }

    /// Read every external dependency `graph` still lacks until it is closed.
    ///
    /// Each pass reads the current missing set; a fetched Agent may add its
    /// Prompt reference, and Prompts add nothing, so the loop terminates.
    /// Only external `ref` slots are read from the registry; a sibling the
    /// bundle did not load is refused rather than substituted.
    ///
    /// Cancellation may stop between reads, leaving `graph` partially filled;
    /// nothing durable is written.
    ///
    /// # Errors
    /// Returns `WYRD_REGISTRY_422_UNRESOLVED_DEPENDENCY` for a missing
    /// sibling, and the errors of [`read`](Self::read) and
    /// [`WorkflowGraph::insert`].
    async fn fetch_missing(&self, graph: &mut WorkflowGraph) -> Result<(), WyrdError> {
        loop {
            let missing = graph.missing();
            if missing.is_empty() {
                return Ok(());
            }
            for dependency in missing {
                let card = match &dependency {
                    Ref::Ref(card_ref) => self.read(card_ref).await?,
                    Ref::Sibling { sibling } => {
                        return Err(unresolved(sibling, "is not a loaded sibling"));
                    }
                    Ref::Path(path) => {
                        return Err(WyrdError::registry_invalid_card_spec(format!(
                            "card dependency path {} was not loaded",
                            path.display()
                        )));
                    }
                };
                graph.insert(dependency, card.spec)?;
            }
        }
    }

    /// Read one active Card by its exact identity.
    ///
    /// Cancellation may drop the in-flight Cards read; nothing is written.
    ///
    /// # Errors
    /// Returns `WYRD_REGISTRY_422_UNRESOLVED_DEPENDENCY` without a client or
    /// when the Card is not active, and the Cards exact-read error otherwise.
    async fn read(&self, card_ref: &CardRef) -> Result<Card, WyrdError> {
        let cards = self.cards.as_ref().ok_or_else(|| {
            unresolved(
                card_ref,
                "requires a registry client; attach one with WorkflowLoader::with_client",
            )
        })?;
        let card = cards.get(CardSelector::exact(card_ref.clone())).await?;
        let active = card
            .status
            .as_ref()
            .is_some_and(|status| status.phase == "active");
        if active {
            Ok(card)
        } else {
            Err(unresolved(card_ref, "is not active"))
        }
    }
}

/// One Workflow Card with the exact Agent and Prompt bodies it references.
///
/// The graph is the pure, synchronous hand-off from whichever environment
/// fetched the bodies — local disk, the registry client, or server
/// registration — to Skald's existing resolver traits. It performs no IO.
/// Bodies are keyed by the authored reference form as well as identity: a
/// `Sibling` slot consumes only a submitted or path-loaded body, and a `Ref`
/// slot only the body its exact registry read returned, even when both name
/// the same `(kind, space, name, version)`.
pub struct WorkflowGraph {
    /// The Workflow Card being hydrated.
    workflow: WorkflowCard,
    /// Referenced Agent and Prompt bodies keyed by provenance and identity.
    bodies: HashMap<BodyKey, Spec>,
}

/// Provenance-qualified body key: whether the slot is a loader sibling, and
/// the exact identity it names.
type BodyKey = (bool, CardRefIdentity);

/// Return the body key for a durable dependency, or `None` for a path.
fn body_key(dependency: &Ref) -> Option<BodyKey> {
    match dependency {
        Ref::Ref(card_ref) => Some((false, card_ref.identity_key())),
        Ref::Sibling { sibling } => Some((true, sibling.identity_key())),
        Ref::Path(_) => None,
    }
}

impl WorkflowGraph {
    /// Start a graph for `workflow` after its pure contract validation.
    ///
    /// # Errors
    /// Returns the field-specific `WYRD_WORKFLOW_*` contract error.
    pub fn new(workflow: WorkflowCard) -> Result<Self, WyrdError> {
        workflow.spec.validate()?;
        Ok(Self {
            workflow,
            bodies: HashMap::new(),
        })
    }

    /// Start a graph from a Workflow registration submission.
    ///
    /// # Errors
    /// Returns `WYRD_REGISTRY_400_INVALID_CARD_SPEC` when the submission is
    /// not a decodable Workflow with an exact identity, and the errors of
    /// [`new`](Self::new).
    pub fn from_submission(submission: &CardSubmission) -> Result<Self, WyrdError> {
        Self::new(WorkflowCard::from_envelope(submission_card(submission)?)?)
    }

    /// Record the body supplied for one Agent or Prompt dependency.
    ///
    /// `dependency` carries the authored form: pass `Ref::Sibling` for a
    /// submitted or path-loaded body and `Ref::Ref` for the body an exact
    /// registry read returned. Re-inserting the same key replaces the body.
    ///
    /// # Errors
    /// Returns `WYRD_REGISTRY_400_INVALID_CARD_SPEC` for an unresolved path or
    /// when `spec` is not the kind `dependency` names.
    pub fn insert(&mut self, dependency: Ref, spec: Spec) -> Result<(), WyrdError> {
        let (Some(card_ref), Some(key)) = (dependency.as_card_ref(), body_key(&dependency)) else {
            return Err(WyrdError::registry_invalid_card_spec(
                "card dependency is an unresolved path",
            ));
        };
        if spec.kind() != card_ref.kind {
            return Err(WyrdError::registry_invalid_card_spec(format!(
                "card dependency {card_ref} resolved to a {} body",
                spec.kind().wire_name()
            )));
        }
        self.bodies.insert(key, spec);
        Ok(())
    }

    /// Return the Agent and Prompt dependencies whose bodies are still absent.
    ///
    /// Each dependency keeps its authored `Ref` or `Sibling` form, so the
    /// caller supplies it from the matching source. Prompt references of
    /// known Agents — inline step Agents and recorded Agent bodies — are
    /// included, so repeated fetching closes the graph. Paths are never
    /// returned; they stay unresolved and fail hydration.
    #[must_use]
    pub fn missing(&self) -> Vec<Ref> {
        let mut missing: Vec<Ref> = Vec::new();
        let mut want = |dependency: Option<Ref>| {
            if let Some(dependency) = dependency
                && let Some(key) = body_key(&dependency)
                && !self.bodies.contains_key(&key)
                && !missing
                    .iter()
                    .any(|known| body_key(known).as_ref() == Some(&key))
            {
                missing.push(dependency);
            }
        };
        for step in &self.workflow.spec.steps {
            let WorkflowAction::Agent(agent) = &step.action;
            let agent = match agent {
                InlineableRef::Inline(agent) => Some(agent.as_ref()),
                reference => {
                    let dependency = reference.to_durable();
                    let agent = dependency
                        .as_ref()
                        .and_then(|dependency| self.agent(dependency));
                    want(dependency);
                    agent
                }
            };
            if let Some(agent) = agent {
                want(agent.prompt.to_durable());
            }
        }
        missing
    }

    /// Run declarative registration validation without binding tools.
    ///
    /// Declared tool names are execution-environment suitability, not
    /// registration validity, so they are left unresolved here; every other
    /// pure and resolved Workflow check runs exactly as for execution.
    ///
    /// # Errors
    /// Returns the errors of [`hydrate`](Self::hydrate) other than tool
    /// resolution.
    pub fn validate(&self) -> Result<(), WyrdError> {
        let mut workflow = self.workflow.clone();
        for step in &mut workflow.spec.steps {
            let WorkflowAction::Agent(agent) = &mut step.action;
            if let InlineableRef::Inline(agent) = agent {
                agent.tool_names.clear();
            }
        }
        let tools = ToolRegistry::new();
        let resolver = GraphResolver {
            graph: self,
            tools: &tools,
            bind_tools: false,
        };
        Workflow::from_card_with_agent_resolver(workflow, &tools, &resolver, Some(&resolver))?
            .validate()
            .map_err(Into::into)
    }

    /// Hydrate the runnable Skald Workflow, binding Agent tools from `tools`.
    ///
    /// # Errors
    /// Returns the pure Workflow contract error,
    /// `WYRD_REGISTRY_422_UNRESOLVED_DEPENDENCY` for a body that was never
    /// recorded, the Agent tool or Prompt resolution error, and the resolved
    /// validation errors of [`Workflow::validate`] (Prompt-variable bindings,
    /// payload kinds, output schemas, and route dialect).
    pub fn hydrate(&self, tools: &dyn ToolResolver) -> Result<Workflow, WyrdError> {
        let resolver = GraphResolver {
            graph: self,
            tools,
            bind_tools: true,
        };
        let workflow = Workflow::from_card_with_agent_resolver(
            self.workflow.clone(),
            tools,
            &resolver,
            Some(&resolver),
        )?;
        workflow.validate()?;
        Ok(workflow)
    }

    /// Return the Agent body recorded for `dependency`'s provenance and
    /// identity.
    fn agent(&self, dependency: &Ref) -> Option<&AgentSpec> {
        match body_key(dependency).and_then(|key| self.bodies.get(&key)) {
            Some(Spec::Agent(agent)) => Some(agent),
            _ => None,
        }
    }
}

/// Serves a [`WorkflowGraph`]'s recorded bodies through Skald's resolvers.
struct GraphResolver<'a> {
    /// Graph holding the recorded bodies.
    graph: &'a WorkflowGraph,
    /// Tools bound to referenced Agents.
    tools: &'a dyn ToolResolver,
    /// Whether referenced Agents bind their declared tool names.
    bind_tools: bool,
}

impl AgentResolver for GraphResolver<'_> {
    /// Build the runtime Agent for a referenced step from the body recorded
    /// under the step's authored provenance, keeping its exact identity.
    ///
    /// # Errors
    /// Returns `WYRD_REGISTRY_400_INVALID_CARD_SPEC` for a step that is not a
    /// Card reference, `WYRD_REGISTRY_422_UNRESOLVED_DEPENDENCY` when no body
    /// was recorded for that provenance, and the Agent tool and Prompt
    /// resolution errors of `Agent::from_card`.
    fn resolve(&self, agent_ref: &InlineableRef<AgentSpec>) -> Result<Agent, WyrdError> {
        let dependency = agent_ref.to_durable();
        let (Some(dependency), Some(card_ref)) = (
            dependency.as_ref(),
            dependency.as_ref().and_then(Ref::as_card_ref),
        ) else {
            return Err(WyrdError::registry_invalid_card_spec(
                "workflow step Agent is not a card reference",
            ));
        };
        let mut spec = self
            .graph
            .agent(dependency)
            .cloned()
            .ok_or_else(|| unresolved(card_ref, "was not loaded"))?;
        if !self.bind_tools {
            spec.tool_names.clear();
        }
        let card = AgentCard {
            space: card_ref
                .space
                .as_ref()
                .map_or_else(|| "default".to_owned(), ToString::to_string),
            name: card_ref.name.to_string(),
            version: card_ref.version.to_string(),
            uid: card_ref
                .uid
                .as_ref()
                .map(ToString::to_string)
                .unwrap_or_default(),
            labels: Labels::default(),
            annotations: Annotations::default(),
            spec,
            cascade_children: Vec::new(),
            created_at: Utc::now(),
        };
        Agent::from_card(card, self.tools, self)
    }
}

impl PromptResolver for GraphResolver<'_> {
    /// Return an inline native Prompt or the Prompt Card body recorded under
    /// the slot's authored provenance.
    ///
    /// # Errors
    /// Returns `WYRD_REGISTRY_400_INVALID_CARD_SPEC` for an unresolved path
    /// and `WYRD_REGISTRY_422_UNRESOLVED_DEPENDENCY` when no Prompt body was
    /// recorded for that provenance and identity.
    fn resolve(&self, prompt_ref: &InlineableRef<NativePrompt>) -> Result<Prompt, WyrdError> {
        if let InlineableRef::Inline(prompt) = prompt_ref {
            return Ok(Prompt::from_native((**prompt).clone()));
        }
        let dependency = prompt_ref.to_durable();
        let (Some(key), Some(card_ref)) = (
            dependency.as_ref().and_then(body_key),
            dependency.as_ref().and_then(Ref::as_card_ref),
        ) else {
            return Err(WyrdError::registry_invalid_card_spec(
                "Agent prompt is an unresolved path",
            ));
        };
        match self.graph.bodies.get(&key) {
            Some(Spec::Prompt(prompt)) => Ok(Prompt::from_native(prompt.prompt.clone())),
            _ => Err(unresolved(card_ref, "was not loaded")),
        }
    }
}

/// Convert a registration submission into a Card envelope.
///
/// # Errors
/// Returns `WYRD_REGISTRY_400_INVALID_CARD_SPEC` for an undecodable spec.
fn submission_card(submission: &CardSubmission) -> Result<Card, WyrdError> {
    let spec = Spec::from_kind_and_value(&submission.kind, submission.spec.clone())
        .map_err(|error| WyrdError::registry_invalid_card_spec(error.to_string()))?;
    Ok(Card {
        api_version: ApiVersion::v1(),
        kind: submission.kind.clone(),
        metadata: submission.metadata.clone(),
        spec,
        relationships: Relationships::default(),
        status: None,
    })
}

/// Return the exact reference a loaded Card's identity names.
///
/// # Errors
/// Returns `WYRD_REGISTRY_400_INVALID_CARD_SPEC` when the Card lacks a
/// resolved space or exact version.
fn exact_ref(card: &Card) -> Result<CardRef, WyrdError> {
    match (&card.metadata.space, card.metadata.resolved_pin()) {
        (Some(space), Some(version)) => Ok(CardRef {
            kind: card.kind.clone(),
            name: card.metadata.name.clone(),
            version: version.clone(),
            space: Some(space.clone()),
            uid: card.metadata.uid.clone(),
        }),
        _ => Err(WyrdError::registry_invalid_card_spec(format!(
            "{} {} has no exact space and version",
            card.kind.wire_name(),
            card.metadata.name
        ))),
    }
}

/// Build the refusal for a dependency this environment cannot supply.
fn unresolved(card_ref: &CardRef, reason: &str) -> WyrdError {
    WyrdError::RegistryUnresolvedDependency {
        message: format!("card dependency {card_ref} {reason}"),
        details: serde_json::json!({ "card_ref": card_ref.to_string() }),
    }
}

/// Loader composition, provenance, and hydration tests over the checked-in
/// code-review bundle and a real client/server boundary.
#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::Mutex;

    use async_trait::async_trait;
    use serde_json::json;
    use skald_providers::ProviderError;
    use skald_spec::ProviderResponse;
    use skald_spec::wire::openai_chat::OpenAiChatResponse;
    use skald_workflow::{
        WorkflowExecutionDependencies, WorkflowRunOptions, WorkflowRunStatus, WyrdGatewayCall,
        WyrdGatewayCaller,
    };
    use tempfile::TempDir;
    use tokio_util::sync::CancellationToken;

    use super::*;

    /// Deterministic gateway answering each reviewer by its system role.
    #[derive(Default)]
    struct ReviewGateway {
        /// Serialized requests in arrival order.
        requests: Mutex<Vec<String>>,
    }

    #[async_trait]
    impl WyrdGatewayCaller for ReviewGateway {
        /// Answer with a fixed review per reviewer, recording the request.
        ///
        /// # Errors
        /// Never returns an error; every request receives its fixed review.
        async fn call(
            &self,
            call: WyrdGatewayCall,
            _cancellation: &CancellationToken,
        ) -> Result<ProviderResponse, ProviderError> {
            let request = serde_json::to_string(&call.request).unwrap_or_default();
            let text = if request.contains("security reviewer") {
                "SECURITY-FINDINGS"
            } else if request.contains("correctness reviewer") {
                "CORRECTNESS-FINDINGS"
            } else {
                "FINAL-REVIEW"
            };
            self.requests
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .push(request);
            Ok(chat_text(text))
        }
    }

    /// One OpenAI Chat completion carrying `text`.
    ///
    /// # Panics
    /// Panics if the static completion fixture stops decoding, which is a
    /// test-fixture invariant.
    fn chat_text(text: &str) -> ProviderResponse {
        let response: OpenAiChatResponse = serde_json::from_value(json!({
            "id": "resp",
            "object": "chat.completion",
            "created": 0,
            "model": "gpt-5-5",
            "choices": [{
                "index": 0,
                "message": { "role": "assistant", "content": text },
                "finish_reason": "stop"
            }]
        }))
        .expect("static completion decodes");
        ProviderResponse::OpenAiChatCompletion(response)
    }

    /// The checked-in code-review bundle directory.
    fn bundle() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../examples/workflows/code-review")
    }

    /// Copy the bundle's Agent and Prompt Cards into a temp directory with
    /// `edit` applied to the Workflow YAML.
    ///
    /// # Panics
    /// Panics when a bundle file cannot be read, copied, or written.
    fn edited_bundle(edit: impl Fn(String) -> String) -> TempDir {
        let temp = TempDir::new().expect("temp directory creates");
        for dir in ["agents", "prompts"] {
            std::fs::create_dir(temp.path().join(dir)).expect("bundle directory creates");
            for file in ["security", "correctness", "final-reviewer"] {
                std::fs::copy(
                    bundle().join(format!("{dir}/{file}.yaml")),
                    temp.path().join(format!("{dir}/{file}.yaml")),
                )
                .expect("bundle file copies");
            }
        }
        let workflow =
            std::fs::read_to_string(bundle().join("workflow.yaml")).expect("workflow reads");
        std::fs::write(temp.path().join("workflow.yaml"), edit(workflow)).expect("workflow writes");
        temp
    }

    /// Hydrate and run the checked-in bundle offline: its path-loaded Prompt
    /// Cards back each Agent, and both reviewers feed the final reviewer's
    /// declared Prompt variables through the existing binder. External refs
    /// without a client — including one naming the same identity as a loaded
    /// sibling — extra Prompt bindings, and a route/dialect mismatch are
    /// refused at load before any call.
    ///
    /// # Panics
    /// Panics when the bundle stops hydrating or running as asserted, or a
    /// refusal is missing or carries the wrong code.
    #[tokio::test]
    async fn hydrate_local_workflow_graph() {
        let loader = WorkflowLoader::new(Arc::new(ToolRegistry::new()));
        let workflow = loader
            .load_file(&bundle().join("workflow.yaml"))
            .await
            .expect("bundle hydrates");
        assert_eq!(
            workflow.step_ids(),
            vec!["security", "correctness", "final_review"]
        );

        let gateway = Arc::new(ReviewGateway::default());
        let dependencies =
            WorkflowExecutionDependencies::new(skald_runtime::ProviderRegistry::new())
                .with_wyrd_gateway(Arc::clone(&gateway) as Arc<dyn WyrdGatewayCaller>);
        let input = serde_json::Map::from_iter([(
            "code".to_owned(),
            json!("diff --git a/src/auth.rs b/src/auth.rs"),
        )]);
        let run = workflow
            .run_with_options(&dependencies, input, WorkflowRunOptions::default())
            .await
            .expect("run starts");
        assert_eq!(run.status, WorkflowRunStatus::Succeeded);
        assert_eq!(run.outputs["review"], json!("FINAL-REVIEW"));
        assert!(run.workflow.is_none());
        let requests = gateway
            .requests
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone();
        assert_eq!(requests.len(), 3);
        let last = requests.last().expect("final request recorded");
        assert!(last.contains("SECURITY-FINDINGS") && last.contains("CORRECTNESS-FINDINGS"));
        assert!(last.contains("diff --git a/src/auth.rs"));

        let external = edited_bundle(|yaml| {
            yaml.replacen(
                "          path: ./agents/security.yaml",
                "          ref:\n            kind: Agent\n            name: security-reviewer\n            version: \"1.0.0\"",
                1,
            )
        });
        let error = loader
            .load_file(&external.path().join("workflow.yaml"))
            .await
            .expect_err("a ref needs a registry client");
        assert_eq!(error.code(), "WYRD_REGISTRY_422_UNRESOLVED_DEPENDENCY");

        let shadowed = edited_bundle(|yaml| {
            yaml.replacen(
                "    - id: correctness",
                "    - id: registered_security\n      action:\n        type: agent\n        target:\n          ref:\n            kind: Agent\n            name: security-reviewer\n            version: \"1.0.0\"\n      inputs:\n        code: input.code\n\n    - id: correctness",
                1,
            )
        });
        let error = loader
            .load_file(&shadowed.path().join("workflow.yaml"))
            .await
            .expect_err("a loaded sibling never satisfies an external ref");
        assert_eq!(error.code(), "WYRD_REGISTRY_422_UNRESOLVED_DEPENDENCY");

        let extra = edited_bundle(|yaml| {
            yaml.replacen(
                "        code: input.code\n      timeout_seconds",
                "        code: input.code\n        extra: input.code\n      timeout_seconds",
                1,
            )
        });
        let error = loader
            .load_file(&extra.path().join("workflow.yaml"))
            .await
            .expect_err("extra binding is refused");
        assert_eq!(error.code(), "WYRD_WORKFLOW_422_VALIDATION");
        assert!(error.to_string().contains("steps[0].inputs.extra"));

        let dialect = edited_bundle(|yaml| {
            yaml.replacen(
                "    kind: wyrd_gateway",
                "    kind: ext_gateway\n    protocol: anthropic_messages\n    base_url: https://gateway.example.com\n    credential_binding: review-gateway",
                1,
            )
        });
        let error = loader
            .load_file(&dialect.path().join("workflow.yaml"))
            .await
            .expect_err("route dialect mismatch is refused");
        assert_eq!(error.code(), "WYRD_WORKFLOW_422_ROUTE_UNSUPPORTED");
        assert_eq!(
            gateway
                .requests
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .len(),
            3,
            "refused loads dispatch nothing"
        );
    }
}
