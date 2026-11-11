//! Agent actions and checks on the shared Skald Agent loop.
//!
//! [`AgentExecutor`] resolves each call's exact Agent and Prompt, binds the
//! call's explicit inputs to the Prompt placeholders, applies the per-call
//! execution route, attaches only the declared built-in tools, and runs
//! [`Agent::run_prompt`]. A step Agent returns data shaped by its Prompt's
//! response schema; a check Agent gets the same declared tools and
//! workspace and returns the closed [`Verdict`]. Nothing is cached between calls, so one executor runs
//! any number of distinct Agents, and no call can see another's media,
//! tools, or workspace.

use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use serde_json::{Map, Value};
use skald_agent::{
    Agent, AgentError, WyrdGatewayCaller, agent_error_retryable,
    run_config_from_agent_run_config_spec,
};
use skald_prompt::Prompt;
use skald_providers::ProviderError;
use skald_runtime::{ProviderRegistry, SkaldRuntimeError};
use skald_spec::{MediaRef, PreparedSchemas, ResponseType};
use tokio::time::Instant;
use wyrd_spec::card::agent::{AgentSpec, BuiltinTool, ExecutionRoute};
use wyrd_spec::card::verifier::{Action, AgentCall, Check, InputSlot};
use wyrd_spec::error::WyrdError;
use wyrd_spec::ids::SlotName;
use wyrd_spec::reference::{CardRef, InlineableRef};
use wyrd_spec::verification::TaskId;
use wyrd_spec::verification::VerificationVerdict;
use wyrd_spec::verification::execution::{CallProvenance, Citation, InputDeclaration, Verdict};

use super::tools::{AgentToolset, ToolSession, finished};
use super::{
    ActionExecutor, ActionOutput, BlockCall, BlockError, CallInput, CallValue, CheckExecutor,
    CheckOutput, EngineError, Position, verdict_from,
};

/// Histogram of one Agent call's phases, seconds, labelled `phase`:
/// `resolve` (the Agent, its Prompt, and the route), `bind` (inputs and
/// citation slots into the Prompt), `run` (the Agent loop, its tools and model
/// calls), and, for a check, `verdict` (the answer against the Verdict format)
/// and `cite` (citations against reachable evidence). It is benchmark-only
/// timing: recorded only once [`wyrd_benchmark::install`] has run, so a build
/// without the `bench` feature never records or times it.
pub const AGENT_PHASE_DURATION_SECONDS: &str = "wyrd_verification_agent_phase_duration_seconds";

/// One benchmark-only Agent phase's start: `None` unless the benchmark
/// recorder is installed, so an ordinary build reads no clock.
fn phase_start() -> Option<std::time::Instant> {
    wyrd_benchmark::enabled().then(std::time::Instant::now)
}

/// Records the Agent `phase` begun at `started` into
/// [`AGENT_PHASE_DURATION_SECONDS`] and returns the next phase's start.
fn phase_end(
    phase: &'static str,
    started: Option<std::time::Instant>,
) -> Option<std::time::Instant> {
    let started = started?;
    let now = std::time::Instant::now();
    metrics::histogram!(AGENT_PHASE_DURATION_SECONDS, "phase" => phase)
        .record(now.duration_since(started).as_secs_f64());
    Some(now)
}

/// Resolves a registered Agent Card into its pure Agent spec.
#[async_trait]
pub trait AgentCardResolver: Send + Sync {
    /// Resolve an Agent Card reference.
    ///
    /// # Errors
    /// Returns [`BlockError::Terminal`] for an absent or non-Agent Card and
    /// [`BlockError::Retryable`] for a transient registry failure.
    async fn resolve(&self, agent_ref: &CardRef) -> Result<AgentSpec, BlockError>;
}

/// Resolves a registered Prompt Card into a runtime Skald Prompt.
#[async_trait]
pub trait PromptCardResolver: Send + Sync {
    /// Resolve a Prompt Card reference.
    ///
    /// # Errors
    /// Returns [`BlockError::Terminal`] for an absent or non-Prompt Card and
    /// [`BlockError::Retryable`] for a transient registry failure.
    async fn resolve(&self, prompt_ref: &CardRef) -> Result<Prompt, BlockError>;
}

/// Reads pinned original evidence as provider-native media.
///
/// The host owns storage and tenancy; it returns only content whose length,
/// digest, and sniffed format match the declaration and that has a native
/// provider media kind.
#[async_trait]
pub trait EvidenceReader: Send + Sync {
    /// Read `evidence` as inline provider-native media.
    ///
    /// # Errors
    /// Returns [`BlockError::Terminal`] for missing, unauthorized,
    /// mismatched, oversized, or non-native evidence and
    /// [`BlockError::Retryable`] for a transient storage failure.
    async fn media(&self, evidence: &InputDeclaration) -> Result<MediaRef, BlockError>;

    /// The subset of `citations` whose slot is in `reachable` and whose
    /// locator names a location in that slot's pinned original evidence.
    ///
    /// # Errors
    /// Returns [`BlockError::Retryable`] when evidence cannot be read now;
    /// evidence that can never be read leaves its citations invalid.
    async fn cited(
        &self,
        reachable: &BTreeSet<SlotName>,
        citations: Vec<Citation>,
    ) -> Result<Vec<Citation>, BlockError>;

    /// The inventory paths of exact Artifact `card` that `path` selects:
    /// that file, every file beneath that directory on `/` boundaries, or
    /// with `None` every file, in path order.
    ///
    /// [`VerificationEngine::run`](super::VerificationEngine::run) calls it
    /// for each fixed workspace attachment before admission. The default
    /// mounts no Artifact files.
    ///
    /// # Errors
    /// Returns [`BlockError::Terminal`] for an unreadable Card or a selection
    /// matching no file and [`BlockError::Retryable`] when the registry is
    /// unavailable.
    async fn select(
        &self,
        _card: &CardRef,
        _path: Option<&str>,
    ) -> Result<Vec<String>, BlockError> {
        Err(terminal("this host mounts no Artifact files"))
    }

    /// Admit an execution's whole pinned `evidence` against its declared
    /// `slots` before any block runs.
    ///
    /// `evidence` holds the caller's slots plus one undeclared slot per
    /// fixed Artifact attachment file; every admitted slot is citable. It is
    /// the run's one evidence backing: a reader that keeps it for later
    /// citations keeps an [`Arc::downgrade`] view, so the engine moves the
    /// evidence uncopied into its outcome once every block has settled.
    ///
    /// [`VerificationEngine::run`](super::VerificationEngine::run) calls it
    /// once, after its own slot validation, for checks only the host can
    /// make, such as a registered Artifact file's format and size or a
    /// combined input-file bound. The default admits everything.
    ///
    /// # Errors
    /// Returns [`EngineError::InputInvalid`] or [`EngineError::InputTooLarge`]
    /// for evidence the host refuses and [`EngineError::EvidenceUnavailable`]
    /// when it cannot decide now.
    async fn admit(
        &self,
        _slots: &BTreeMap<SlotName, InputSlot>,
        _evidence: &Arc<BTreeMap<SlotName, InputDeclaration>>,
    ) -> Result<(), EngineError> {
        Ok(())
    }
}

/// Runs Agent step actions and Agent requirement checks.
///
/// Built per execution with the accepted caller's gateway, evidence reader,
/// toolset, and route overrides; the provider registry serves native routes
/// only when no gateway is configured.
pub struct AgentExecutor {
    /// Providers for native routes outside server-controlled execution.
    providers: Arc<ProviderRegistry>,
    /// Registered Agent Cards.
    agents: Arc<dyn AgentCardResolver>,
    /// Registered Prompt Cards.
    prompts: Arc<dyn PromptCardResolver>,
    /// Native media reader; `None` refuses media placeholders.
    evidence: Option<Arc<dyn EvidenceReader>>,
    /// Governed gateway; when set, native routes are refused.
    gateway: Option<Arc<dyn WyrdGatewayCaller>>,
    /// Built-in tools for Agent calls; `None` refuses declared tools.
    toolset: Option<Arc<dyn AgentToolset>>,
    /// Whole-route replacements keyed by step or requirement ID.
    routes: BTreeMap<TaskId, ExecutionRoute>,
    /// Response and Verdict schema preparation, shared with the host's other
    /// executions when it lends one.
    schemas: Arc<PreparedSchemas>,
}

impl AgentExecutor {
    /// An executor resolving Cards through `agents` and `prompts` and
    /// dispatching native routes to `providers`.
    #[must_use]
    pub fn new(
        providers: Arc<ProviderRegistry>,
        agents: Arc<dyn AgentCardResolver>,
        prompts: Arc<dyn PromptCardResolver>,
    ) -> Self {
        Self {
            providers,
            agents,
            prompts,
            evidence: None,
            gateway: None,
            toolset: None,
            routes: BTreeMap::new(),
            schemas: Arc::default(),
        }
    }

    /// Reuse `schemas` to prepare each call's narrowed response schema and
    /// declared Verdict format, so executions with unchanged schemas skip
    /// meta-validation and compilation. Every answer is still validated.
    #[must_use]
    pub fn with_prepared_schemas(mut self, schemas: Arc<PreparedSchemas>) -> Self {
        self.schemas = schemas;
        self
    }

    /// Read native media placeholders' evidence through `reader`.
    #[must_use]
    pub fn with_evidence(mut self, reader: Arc<dyn EvidenceReader>) -> Self {
        self.evidence = Some(reader);
        self
    }

    /// Serve every model call through the Wyrd gateway `caller`, refusing
    /// native routes, as server-hosted execution requires.
    #[must_use]
    pub fn with_wyrd_gateway(mut self, caller: Arc<dyn WyrdGatewayCaller>) -> Self {
        self.gateway = Some(caller);
        self
    }

    /// Bind Agent calls' declared built-ins through `toolset`.
    #[must_use]
    pub fn with_toolset(mut self, toolset: Arc<dyn AgentToolset>) -> Self {
        self.toolset = Some(toolset);
        self
    }

    /// Replace the declared route of each keyed step or requirement whole.
    #[must_use]
    pub fn with_route_overrides(mut self, routes: BTreeMap<TaskId, ExecutionRoute>) -> Self {
        self.routes = routes;
        self
    }

    /// Resolve the Agent and its Prompt, keeping each Card identity; an
    /// inline Agent is borrowed from the call, not copied.
    ///
    /// # Errors
    /// Returns [`BlockError::Terminal`] for a loader-only path reference and
    /// the resolvers' errors.
    async fn resolve<'a>(
        &self,
        agent: &'a InlineableRef<AgentSpec>,
    ) -> Result<Resolved<'a>, BlockError> {
        let spec = match agent {
            InlineableRef::Ref(card) => Cow::Owned(self.agents.resolve(card).await?),
            InlineableRef::Sibling { sibling } => return Err(unbound_sibling("Agent", sibling)),
            InlineableRef::Inline(spec) => Cow::Borrowed(&**spec),
            InlineableRef::Path(path) => return Err(unresolved_path("Agent", path)),
        };
        let prompt = match &spec.prompt {
            InlineableRef::Inline(prompt) => Prompt::from_native((**prompt).clone()),
            InlineableRef::Ref(card) => self.prompts.resolve(card).await?,
            InlineableRef::Sibling { sibling } => return Err(unbound_sibling("Prompt", sibling)),
            InlineableRef::Path(path) => return Err(unresolved_path("Prompt", path)),
        };
        Ok(Resolved {
            agent_ref: agent.as_card_ref().cloned(),
            prompt_ref: spec.prompt.as_card_ref().cloned(),
            prompt,
            spec,
        })
    }

    /// The call's effective route, refusing routes this host cannot serve.
    ///
    /// # Errors
    /// Returns [`BlockError::Terminal`] for a native route under a gateway or
    /// a gateway route without one.
    fn route(&self, id: &TaskId, declared: &ExecutionRoute) -> Result<ExecutionRoute, BlockError> {
        let route = self.routes.get(id).unwrap_or(declared).clone();
        match (&route, &self.gateway) {
            (ExecutionRoute::WyrdGateway { .. }, Some(_)) | (ExecutionRoute::Native {}, None) => {
                Ok(route)
            }
            (ExecutionRoute::WyrdGateway { .. }, None) => Err(terminal(
                "the Agent routes through the Wyrd gateway, which this host is not configured with",
            )),
            (ExecutionRoute::Native {}, Some(_)) => Err(terminal(
                "server-hosted Agent calls refuse a native route; declare a wyrd_gateway route",
            )),
        }
    }

    /// Bind `inputs` to the placeholders of `prompt`, the call's own copy.
    ///
    /// Input keys must equal the Prompt's text and media placeholders. JSON
    /// strings render raw and other JSON as JSON text; evidence binds as
    /// provider-native media, which Skald checks against the placeholder kind.
    /// Non-empty `slots` narrow the copy's Verdict citation slots, so the
    /// provider and the Agent's validation admit only the reachable ones.
    ///
    /// # Errors
    /// Returns [`BlockError::Terminal`] for a placeholder mismatch, evidence
    /// bound to text or JSON bound to media, a media kind the placeholder
    /// does not accept, a missing evidence reader, or a narrowed schema its
    /// provider rejects, plus the reader's errors.
    async fn bind(
        &self,
        prompt: Prompt,
        inputs: &BTreeMap<String, CallInput>,
        slots: &BTreeSet<SlotName>,
    ) -> Result<(Prompt, Vec<(String, String)>), BlockError> {
        let mut bound = prompt.into_native();
        let placeholders: BTreeSet<&str> = bound
            .variables
            .iter()
            .chain(&bound.media_variables)
            .map(String::as_str)
            .collect();
        if placeholders != inputs.keys().map(String::as_str).collect() {
            return Err(terminal("Agent inputs must match the Prompt placeholders"));
        }
        let mut vars = Vec::new();
        for (name, input) in inputs {
            let is_media = bound.media_variables.contains(name);
            match (&input.value, is_media) {
                (CallValue::Json(value), false) => vars.push((name.clone(), render(value))),
                (CallValue::Evidence(evidence), true) => {
                    let reader = self
                        .evidence
                        .as_ref()
                        .ok_or_else(|| terminal("media inputs need an evidence reader"))?;
                    let media = reader.media(evidence).await?;
                    bound
                        .bind_media_mut(name, &media)
                        .map_err(|error| BlockError::Terminal {
                            reason: format!("media placeholder {name}: {error}"),
                        })?;
                }
                (CallValue::Json(_), true) => {
                    return Err(BlockError::Terminal {
                        reason: format!("media placeholder {name} needs a media input"),
                    });
                }
                (CallValue::Evidence(_), false) => {
                    return Err(BlockError::Terminal {
                        reason: format!("text placeholder {name} cannot bind media"),
                    });
                }
            }
        }
        if !slots.is_empty() {
            bound
                .edit_response_schema_mut(&self.schemas, |schema| {
                    Verdict::restrict_slots(schema, slots);
                })
                .map_err(|error| BlockError::Terminal {
                    reason: format!("the Verdict citation slots cannot be narrowed: {error}"),
                })?;
        }
        Ok((Prompt::from_native(bound), vars))
    }

    /// Run one Agent call and return its raw run plus provenance.
    ///
    /// `slots` are the citation slots a check invocation can reach, empty for
    /// a step; see [`Self::bind`].
    ///
    /// Steps and checks get exactly their declared built-ins from the toolset, which holds the
    /// invocation's workspace until the run ends. The first tool refused
    /// process capacity ends the invocation busy at once: the loop is raced
    /// against the session's refusal and dropped with any sibling tool calls
    /// still pending, so no further model call or tool result follows.
    ///
    /// # Errors
    /// Returns resolution, binding, route, tool, and run failures as
    /// [`BlockError`]s, and [`BlockError::Busy`] when the workspace or a tool
    /// is refused process capacity.
    ///
    /// # Panics
    /// Never panics: the loop stops before completing only once a refusal
    /// is raised, and a raised refusal returns busy first.
    async fn invoke(
        &self,
        id: &TaskId,
        call: &AgentCall,
        inputs: &BTreeMap<String, CallInput>,
        workspace: &BTreeMap<String, CallInput>,
        slots: &BTreeSet<SlotName>,
        deadline: Instant,
    ) -> Result<Invoked, BlockError> {
        let started = phase_start();
        let resolved = self.resolve(&call.agent).await?;
        let route = self.route(id, &resolved.spec.execution_route)?;
        let started = phase_end("resolve", started);
        let (prompt, vars) = self.bind(resolved.prompt, inputs, slots).await?;
        let started = phase_end("bind", started);
        let declared = resolved
            .spec
            .tool_names
            .iter()
            .map(|name| {
                BuiltinTool::from_name(name).ok_or_else(|| BlockError::Terminal {
                    reason: format!("unknown built-in tool {name}"),
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        let records = Arc::new(Mutex::new(Vec::new()));
        if declared.is_empty() && !call.workspace.is_empty() {
            return Err(terminal("a workspace needs a filesystem or code tool"));
        }
        let session = if declared.is_empty() {
            None
        } else {
            let toolset = self
                .toolset
                .as_ref()
                .ok_or_else(|| terminal("this host provides no built-in tools"))?;
            Some(toolset.open(id, workspace, deadline, &declared).await?)
        };
        let tools = match &session {
            Some(session) => session.resolve(&declared, &records)?,
            None => Vec::new(),
        };
        let prompt = Arc::new(prompt);
        let mut agent = Agent::from_resolved(id.as_str(), Arc::clone(&prompt))
            .with_prepared_schemas(Arc::clone(&self.schemas))
            .with_tools(tools)
            .with_run_config(run_config_from_agent_run_config_spec(
                &resolved.spec.run_config,
            ))
            .with_execution_route(route.clone());
        if let Some(caller) = &self.gateway {
            agent = agent.with_gateway_caller(Arc::clone(caller));
        }
        if let Some(session) = &session {
            agent = agent.before_model(session.halt_on_refusal());
        }
        let borrowed: Vec<(&str, &str)> = vars
            .iter()
            .map(|(key, value)| (key.as_str(), value.as_str()))
            .collect();
        let run = agent.run_prompt(&self.providers, &prompt, &borrowed);
        let run = match &session {
            Some(session) => tokio::select! {
                run = run => Some(run),
                () = session.refusal_raised() => None,
            },
            None => Some(run.await),
        };
        let refused = session.as_ref().and_then(ToolSession::refused);
        drop(session);
        if let Some(reason) = refused {
            return Err(BlockError::Busy { reason });
        }
        let run = run
            .expect("the loop only stops early for a raised refusal")
            .map_err(block_error)?;
        phase_end("run", started);
        Ok(Invoked {
            structured: run.structured_output,
            text: run.output,
            prompt,
            provenance: CallProvenance::Agent {
                agent: resolved.agent_ref,
                prompt: resolved.prompt_ref,
                route,
            },
            tool_calls: finished(&records),
        })
    }
}

#[async_trait]
impl ActionExecutor for AgentExecutor {
    /// Run an Agent step action and return its output data.
    ///
    /// The Agent loop validates structured output against the Prompt's
    /// response schema; a text Prompt's answer is returned as a JSON string.
    ///
    /// # Errors
    /// Returns [`BlockError::Terminal`] for a non-Agent action, plus
    /// [`Self::invoke`] failures.
    async fn execute(&self, call: BlockCall<'_, Action>) -> Result<ActionOutput, BlockError> {
        let Action::Agent(agent) = call.block else {
            return Err(terminal("the Agent executor runs only Agent actions"));
        };
        let invoked = self
            .invoke(
                call.id,
                agent,
                &call.inputs,
                &call.workspace,
                &BTreeSet::new(),
                call.deadline,
            )
            .await?;
        let output = invoked
            .structured
            .map_or(Value::String(invoked.text), Value::Object);
        Ok(ActionOutput {
            output,
            provenance: Some(invoked.provenance),
            tool_calls: invoked.tool_calls,
        })
    }
}

#[async_trait]
impl CheckExecutor for AgentExecutor {
    /// Run an Agent check and return its Verdict.
    ///
    /// Only citations into original evidence reachable through the call's
    /// bindings survive; a pass left without one becomes inconclusive.
    ///
    /// # Errors
    /// Returns [`BlockError::Terminal`] for a non-Agent check, a step-position
    /// call, or an answer [`verdict_from`] refuses against the Prompt's
    /// declared format, plus [`Self::invoke`] failures.
    async fn execute(&self, call: BlockCall<'_, Check>) -> Result<CheckOutput, BlockError> {
        let Check::Agent(agent) = call.block else {
            return Err(terminal("the Agent executor runs only Agent checks"));
        };
        if call.position == Position::Step {
            return Err(terminal("an Agent check cannot run in step position"));
        }
        let reachable: BTreeSet<SlotName> = call
            .inputs
            .values()
            .chain(call.workspace.values())
            .flat_map(|input| input.sources.iter().cloned())
            .collect();
        let invoked = self
            .invoke(
                call.id,
                agent,
                &call.inputs,
                &call.workspace,
                &reachable,
                call.deadline,
            )
            .await?;
        let answer = invoked
            .structured
            .ok_or_else(|| terminal("the check Agent returned no structured Verdict"))?;
        let ResponseType::JsonSchema { schema, .. } = &invoked.prompt.native().response_type else {
            return Err(terminal(
                "the check Agent's Prompt declares no Verdict format",
            ));
        };
        let started = phase_start();
        let mut verdict = verdict_from(&self.schemas, schema, Value::Object(answer))?;
        let started = phase_end("verdict", started);
        let citations = std::mem::take(&mut verdict.citations);
        if let Some(reader) = &self.evidence {
            verdict.citations = reader.cited(&reachable, citations).await?;
        }
        phase_end("cite", started);
        if verdict.outcome == VerificationVerdict::Passed && verdict.citations.is_empty() {
            verdict.outcome = VerificationVerdict::Inconclusive;
            verdict.reason = format!(
                "no valid original-evidence citation supports this pass: {}",
                verdict.reason
            );
        }
        Ok(CheckOutput {
            verdict,
            actual: None,
            detail: None,
            provenance: Some(invoked.provenance),
        })
    }
}

/// An Agent and Prompt resolved for one call.
struct Resolved<'a> {
    /// The Agent body, borrowed when the call declares it inline.
    spec: Cow<'a, AgentSpec>,
    /// The Agent Card, when referenced.
    agent_ref: Option<CardRef>,
    /// The runtime Prompt.
    prompt: Prompt,
    /// The Prompt Card, when referenced.
    prompt_ref: Option<CardRef>,
}

/// The result of one Agent run.
struct Invoked {
    /// Structured output of a structured Prompt.
    structured: Option<Map<String, Value>>,
    /// The final text answer.
    text: String,
    /// The bound Prompt that ran.
    prompt: Arc<Prompt>,
    /// Agent, Prompt, and effective route.
    provenance: CallProvenance,
    /// Tool calls in call order.
    tool_calls: Vec<wyrd_spec::verification::execution::ToolCallRecord>,
}

/// The stable code of a server that cannot take the call now.
const SERVER_UNAVAILABLE: &str = "WYRD_SERVER_503_SERVICE_UNAVAILABLE";

/// Classify an Agent run failure.
///
/// Provider answers follow the shared Agent classification and expose only a
/// gateway problem's stable code and message or a provider's stable code,
/// never a provider body. A runtime failure with no provider answer, such as
/// an unregistered provider, may clear on retry. Structured output still
/// invalid when the Agent's iteration budget ran out carries its stable code
/// and schema locations only. It is terminal, like every non-provider
/// failure except an Agent timeout. A gateway answering
/// [`SERVER_UNAVAILABLE`] is the serving process draining, shedding load, or
/// still loading the tenant's gateway configuration, so the block is
/// [`BlockError::Busy`] and its execution runs again later instead of being
/// judged.
#[must_use]
pub fn block_error(error: AgentError) -> BlockError {
    if let AgentError::Provider(SkaldRuntimeError::Provider {
        source: ProviderError::RemoteProblem(problem),
        ..
    }) = &error
        && problem.code == SERVER_UNAVAILABLE
    {
        return BlockError::Busy {
            reason: format!("{}: {}", problem.code, problem.message),
        };
    }
    let reason = match &error {
        AgentError::Provider(SkaldRuntimeError::Provider {
            source: ProviderError::RemoteProblem(problem),
            ..
        }) => format!("{}: {}", problem.code, problem.message),
        AgentError::Provider(SkaldRuntimeError::Provider { source, .. }) => {
            format!("provider failure {}", source.code())
        }
        AgentError::StructuredOutputInvalid { .. } => {
            format!("{}: {error}", WyrdError::from(&error).code())
        }
        other => other.to_string(),
    };
    let retryable = match &error {
        AgentError::Provider(SkaldRuntimeError::Provider { .. }) | AgentError::Timeout { .. } => {
            agent_error_retryable(&error)
        }
        AgentError::Provider(_) => true,
        _ => false,
    };
    if retryable {
        BlockError::Retryable { reason }
    } else {
        BlockError::Terminal { reason }
    }
}

/// A terminal error with fixed `reason`.
fn terminal(reason: &str) -> BlockError {
    BlockError::Terminal {
        reason: reason.to_owned(),
    }
}

/// The refusal of a path reference the loader should have resolved.
fn unresolved_path(kind: &str, path: &std::path::Path) -> BlockError {
    BlockError::Terminal {
        reason: format!(
            "WYRD_REGISTRY_400_UNRESOLVED_PATH_REF: {kind} path `{}` must be resolved by the loader",
            path.display()
        ),
    }
}

/// The terminal refusal for a declaration-only sibling reaching execution,
/// which runs only registered specs whose siblings declaration replaced.
fn unbound_sibling(kind: &str, sibling: &wyrd_spec::reference::SiblingRef) -> BlockError {
    BlockError::Terminal {
        reason: format!(
            "WYRD_REGISTRY_422_UNRESOLVED_DEPENDENCY: {kind} sibling {sibling} must be registered before execution"
        ),
    }
}

/// Render a JSON value as Prompt text: strings raw, anything else as JSON.
pub(crate) fn render(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use skald_providers::RemoteProblem;
    use skald_spec::ProviderName;

    use super::*;

    /// An Agent failure carrying the gateway problem `code`.
    fn gateway_problem(code: &str, status: u16) -> AgentError {
        AgentError::Provider(SkaldRuntimeError::from_provider(
            ProviderName::OpenAi,
            ProviderError::RemoteProblem(Box::new(RemoteProblem {
                code: code.to_owned(),
                status,
                message: "gateway configuration is loading; retry shortly".to_owned(),
                field: None,
                remediation: String::new(),
            })),
        ))
    }

    /// A gateway that cannot serve the call yet makes the block busy, so the
    /// execution runs again instead of judging it; a capacity refusal keeps
    /// the shared retry classification.
    #[test]
    fn unavailable_gateway_is_busy_and_other_problems_are_classified() {
        assert_eq!(
            block_error(gateway_problem(SERVER_UNAVAILABLE, 503)),
            BlockError::Busy {
                reason: "WYRD_SERVER_503_SERVICE_UNAVAILABLE: \
                         gateway configuration is loading; retry shortly"
                    .to_owned()
            }
        );
        assert!(matches!(
            block_error(gateway_problem("WYRD_GATEWAY_429_LIMIT_EXCEEDED", 429)),
            BlockError::Retryable { .. }
        ));
        assert!(matches!(
            block_error(gateway_problem("WYRD_PERMISSION_403_DENIED_RBAC", 403)),
            BlockError::Terminal { .. }
        ));
    }
}
