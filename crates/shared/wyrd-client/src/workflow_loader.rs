//! Workflow loading: one composition from an authored bundle or a registered
//! Workflow Card to the Skald runtime.
//!
//! [`WorkflowLoader`] owns the optional registry client and the caller's tool
//! registry. It gathers the exact Agent and Prompt bodies a Workflow names —
//! loader siblings from disk, external and registered references through exact
//! Cards reads — into a [`WorkflowGraph`]. The graph feeds Skald's existing
//! synchronous resolver seam and runs pure and resolved Workflow validation.
//! Loading never registers, executes, or resolves secrets.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use chrono::Utc;
use skald_agent::{Agent, PromptResolver};
use skald_prompt::Prompt;
use skald_tool::{ToolRegistry, ToolResolver};
use skald_workflow::{AgentResolver, Workflow};
use wyrd_spec::api_version::ApiVersion;
use wyrd_spec::card::workflow::{WorkflowAction, WorkflowCard};
use wyrd_spec::envelope::{Card, CardKind, Relationships, Spec};
use wyrd_spec::error::WyrdError;
use wyrd_spec::metadata::{Annotations, Labels};
use wyrd_spec::reference::{CardRef, CardRefIdentity, InlineableRef};
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
    /// relative to their containing files and runs pure contract validation.
    /// External `ref` dependencies are read exactly from the registry, which
    /// requires [`with_client`](Self::with_client). Nothing is registered.
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
        let mut workflows = tree.cards.iter().filter(|card| {
            card.submission.kind == CardKind::Workflow && card.source_path == entry
        });
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
                graph.insert(exact_ref(&card)?, card.spec)?;
            }
        }
        self.fetch_missing(&mut graph).await?;
        graph.hydrate(self.tools.as_ref())
    }

    /// Fetch a registered Workflow and its locked dependencies for local
    /// execution.
    ///
    /// `workflow` must name an exact Workflow version and space. Every Card is
    /// read by its exact identity — the stored dependency references carry
    /// their registered UIDs — so later versions never float in. The returned
    /// Workflow keeps its registered identity for the run snapshot.
    ///
    /// # Errors
    /// Returns `WYRD_REGISTRY_400_INVALID_CARD_SPEC` without a client or for a
    /// reference that is not an exact Workflow identity;
    /// `WYRD_REGISTRY_422_UNRESOLVED_DEPENDENCY` for a Card that is not
    /// active; the Cards read error for a missing, foreign, or mismatched
    /// Card; and the errors of [`WorkflowGraph::hydrate`].
    pub async fn load_registered(&self, workflow: &CardRef) -> Result<Workflow, WyrdError> {
        if workflow.kind != CardKind::Workflow
            || workflow.space.is_none()
            || !workflow.version.is_pin()
        {
            return Err(WyrdError::registry_invalid_card_spec(
                "registered Workflow loading requires an exact Workflow space, name, and version",
            ));
        }
        let card = self.read(workflow).await?;
        let mut graph = WorkflowGraph::new(WorkflowCard::from_envelope(card)?)?;
        self.fetch_missing(&mut graph).await?;
        graph.hydrate(self.tools.as_ref())
    }

    /// Read every dependency `graph` still lacks until it is closed.
    ///
    /// Each pass reads the current missing set; a fetched Agent may add its
    /// Prompt reference, and Prompts add nothing, so the loop terminates.
    ///
    /// # Errors
    /// Returns the errors of [`read`](Self::read) and
    /// [`WorkflowGraph::insert`].
    async fn fetch_missing(&self, graph: &mut WorkflowGraph) -> Result<(), WyrdError> {
        loop {
            let missing = graph.missing();
            if missing.is_empty() {
                return Ok(());
            }
            for card_ref in missing {
                let card = self.read(&card_ref).await?;
                graph.insert(card_ref, card.spec)?;
            }
        }
    }

    /// Read one active Card by its exact identity.
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
pub struct WorkflowGraph {
    /// The Workflow Card being hydrated.
    workflow: WorkflowCard,
    /// Referenced Agent and Prompt bodies keyed by exact identity, with the
    /// reference that named them.
    bodies: HashMap<CardRefIdentity, (CardRef, Spec)>,
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

    /// Record the body of one referenced Agent or Prompt Card.
    ///
    /// # Errors
    /// Returns `WYRD_REGISTRY_400_INVALID_CARD_SPEC` when `spec` is not the
    /// kind `card_ref` names.
    pub fn insert(&mut self, card_ref: CardRef, spec: Spec) -> Result<(), WyrdError> {
        if spec.kind() != card_ref.kind {
            return Err(WyrdError::registry_invalid_card_spec(format!(
                "card dependency {card_ref} resolved to a {} body",
                spec.kind().wire_name()
            )));
        }
        self.bodies.insert(card_ref.identity_key(), (card_ref, spec));
        Ok(())
    }

    /// Return the Agent and Prompt references whose bodies are still absent.
    ///
    /// Prompt references of known Agents — inline step Agents and recorded
    /// Agent bodies — are included, so repeated fetching closes the graph.
    #[must_use]
    pub fn missing(&self) -> Vec<CardRef> {
        let mut missing: Vec<CardRef> = Vec::new();
        let mut want = |card_ref: Option<&CardRef>| {
            if let Some(card_ref) = card_ref
                && !self.bodies.contains_key(&card_ref.identity_key())
                && !missing.iter().any(|known| known.same_identity(card_ref))
            {
                missing.push(card_ref.clone());
            }
        };
        for step in &self.workflow.spec.steps {
            let WorkflowAction::Agent(agent) = &step.action;
            let agent = match agent {
                InlineableRef::Inline(agent) => Some(agent.as_ref()),
                reference => {
                    want(reference.as_card_ref());
                    reference.as_card_ref().and_then(|card_ref| self.agent(card_ref))
                }
            };
            if let Some(agent) = agent {
                want(agent.prompt.as_card_ref());
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

    /// Return the recorded Agent body `card_ref` names.
    fn agent(&self, card_ref: &CardRef) -> Option<&AgentSpec> {
        match self.bodies.get(&card_ref.identity_key()) {
            Some((_, Spec::Agent(agent))) => Some(agent),
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
    /// Build the runtime Agent for a referenced step from its recorded body,
    /// keeping the reference's exact identity.
    fn resolve(&self, agent_ref: &InlineableRef<AgentSpec>) -> Result<Agent, WyrdError> {
        let card_ref = agent_ref.as_card_ref().ok_or_else(|| {
            WyrdError::registry_invalid_card_spec("workflow step Agent is not a card reference")
        })?;
        let mut spec = self
            .graph
            .agent(card_ref)
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
    /// Return an inline native Prompt or the recorded Prompt Card body.
    fn resolve(&self, prompt_ref: &InlineableRef<skald_spec::Prompt>) -> Result<Prompt, WyrdError> {
        if let InlineableRef::Inline(prompt) = prompt_ref {
            return Ok(Prompt::from_native((**prompt).clone()));
        }
        let card_ref = prompt_ref.as_card_ref().ok_or_else(|| {
            WyrdError::registry_invalid_card_spec("Agent prompt is an unresolved path")
        })?;
        match self.graph.bodies.get(&card_ref.identity_key()) {
            Some((_, Spec::Prompt(prompt))) => Ok(Prompt::from_native(prompt.prompt.clone())),
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

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::Mutex;

    use async_trait::async_trait;
    use serde_json::json;
    use skald_providers::ProviderError;
    use skald_spec::wire::openai_chat::OpenAiChatResponse;
    use skald_spec::ProviderResponse;
    use skald_workflow::{
        WorkflowExecutionDependencies, WorkflowRunOptions, WorkflowRunStatus, WyrdGatewayCall,
        WyrdGatewayCaller,
    };
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

    /// Copy the bundle into a temp directory with `edit` applied to the
    /// Workflow YAML.
    fn edited_bundle(edit: impl Fn(String) -> String) -> tempfile::TempDir {
        let temp = tempfile::TempDir::new().expect("temp directory creates");
        std::fs::create_dir(temp.path().join("agents")).expect("agents directory creates");
        for agent in ["security", "correctness", "final-reviewer"] {
            std::fs::copy(
                bundle().join(format!("agents/{agent}.yaml")),
                temp.path().join(format!("agents/{agent}.yaml")),
            )
            .expect("agent copies");
        }
        let workflow =
            std::fs::read_to_string(bundle().join("workflow.yaml")).expect("workflow reads");
        std::fs::write(temp.path().join("workflow.yaml"), edit(workflow))
            .expect("workflow writes");
        temp
    }

    /// Hydrate and run the checked-in bundle offline: both reviewers feed the
    /// final reviewer's declared Prompt variables through the existing
    /// binder. External refs without a client, extra Prompt bindings, and a
    /// route/dialect mismatch are refused at load before any call.
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
