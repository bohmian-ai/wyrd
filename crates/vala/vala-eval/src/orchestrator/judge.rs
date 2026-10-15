//! Skald-backed Agent invoker for constrained LLM judge execution.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use serde_json::{Map, Value};
use skald_agent::{Agent, AgentError, WyrdGatewayCaller, run_config_from_agent_run_config_spec};
use skald_prompt::Prompt;
use skald_runtime::{ProviderRegistry, SkaldRuntimeError};
use tokio::sync::Mutex;
use wyrd_spec::card::agent::{AgentSpec, ExecutionRoute};
use wyrd_spec::reference::InlineableRef;
use wyrd_spec::vala::eval::media::MediaRef as EvalMediaRef;

use crate::blocks::agent::render;
use crate::blocks::{AgentCardResolver, BlockError, PromptCardResolver, block_error};
use crate::{JudgeError, JudgeInvoker, MediaBindings};

/// Reads the authorized object behind an Eval media descriptor.
///
/// The server owns storage and tenancy, so it supplies the implementation; the
/// invoker only binds what the resolver returns.
#[async_trait]
pub trait MediaResolver: Send + Sync {
    /// Resolve `media` to provider-native inline content.
    ///
    /// # Errors
    /// Returns [`JudgeError::Terminal`] for missing, unauthorized,
    /// unsupported, or oversized media and [`JudgeError::Retryable`] for a
    /// transient storage failure.
    async fn resolve(&self, media: &EvalMediaRef) -> Result<skald_spec::MediaRef, JudgeError>;
}

/// Skald-backed invoker for one constrained Agent judge per Eval run.
pub struct SkaldJudgeInvoker {
    providers: Arc<ProviderRegistry>,
    agents: Arc<dyn AgentCardResolver>,
    prompts: Arc<dyn PromptCardResolver>,
    /// Resolver for record media; `None` refuses any media-bearing call.
    media: Option<Arc<dyn MediaResolver>>,
    /// Governed Wyrd gateway serving every judge model call in place of
    /// `providers`; `None` calls `providers` directly.
    gateway: Option<Arc<dyn WyrdGatewayCaller>>,
    cached_agent: Mutex<Option<(InlineableRef<AgentSpec>, Arc<Agent>)>>,
    /// Per-attempt outer deadline.
    pub call_deadline: Duration,
}

impl SkaldJudgeInvoker {
    /// Construct a Skald judge invoker.
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
            media: None,
            gateway: None,
            cached_agent: Mutex::new(None),
            call_deadline: Duration::from_mins(1),
        }
    }

    /// Resolve record media through `resolver` before binding it.
    #[must_use]
    pub fn with_media_resolver(mut self, resolver: Arc<dyn MediaResolver>) -> Self {
        self.media = Some(resolver);
        self
    }

    /// Serve judge model calls through the Wyrd gateway `caller`, marking
    /// this invoker as server-controlled.
    ///
    /// A judge Agent's `wyrd_gateway` route then dispatches only to the
    /// gateway adapter bound to `<deployment>/<Prompt model>`, so the
    /// gateway's authorization, accounting, capture, and audit apply to each
    /// call and `providers` is never used; a `native` route is refused rather
    /// than reaching server credentials. Without a caller, a native route uses
    /// `providers` and a gateway route is refused.
    #[must_use]
    pub fn with_wyrd_gateway(mut self, caller: Arc<dyn WyrdGatewayCaller>) -> Self {
        self.gateway = Some(caller);
        self
    }

    /// Return a per-call copy of `prompt` with every record media binding
    /// bound to its resolved provider-native content.
    ///
    /// The cached Agent's prompt is never mutated, so concurrent calls for
    /// different records cannot see each other's media.
    ///
    /// # Errors
    /// Returns [`JudgeError::Terminal`] when media is present without a
    /// resolver or a binding id names no `${media:id}` placeholder in the
    /// Prompt, and any error the resolver returns.
    async fn bind_media(
        &self,
        prompt: &Prompt,
        media: &MediaBindings,
    ) -> Result<Prompt, JudgeError> {
        let resolver = self.media.as_ref().ok_or_else(|| JudgeError::Terminal {
            reason: "record media requires a media resolver".to_owned(),
        })?;
        let mut native = prompt.native().clone();
        for descriptor in media.iter() {
            let resolved = resolver.resolve(descriptor).await?;
            native
                .bind_media_mut(descriptor.id.as_str(), &resolved)
                .map_err(|error| JudgeError::Terminal {
                    reason: format!("media binding `{}`: {error}", descriptor.id.as_str()),
                })?;
        }
        Ok(Prompt::from_native(native))
    }

    /// Resolve, validate, and cache the one judge Agent this invoker runs.
    ///
    /// The judge reference resolves through the Agent resolver (inline specs
    /// are used as authored), its Prompt through the Prompt resolver, and the
    /// result is checked to be a structured-output judge. The Agent keeps its
    /// declared execution route: a `wyrd_gateway` route dispatches through
    /// this invoker's gateway caller, a `native` route through the caller's
    /// providers. The first resolved Agent is cached for the invoker's
    /// lifetime; holding the cache lock across resolution makes concurrent
    /// first calls resolve once.
    ///
    /// # Errors
    /// Returns [`JudgeError::Terminal`] when a second, different judge
    /// reference is requested, a path reference was not resolved by the
    /// loader, the judge Agent or its Prompt is invalid for judging, a gateway
    /// route has no gateway caller, or a server-controlled invoker meets a
    /// native route; and the resolvers' own errors.
    async fn agent_for(
        &self,
        judge_ref: &InlineableRef<AgentSpec>,
    ) -> Result<Arc<Agent>, JudgeError> {
        let mut cached = self.cached_agent.lock().await;
        if let Some((cached_ref, agent)) = cached.as_ref() {
            if cached_ref == judge_ref {
                return Ok(Arc::clone(agent));
            }
            return Err(JudgeError::Terminal {
                reason: "one JudgeInvoker cannot execute multiple judge Agents in one Eval run"
                    .to_owned(),
            });
        }

        let spec = match judge_ref {
            InlineableRef::Ref(agent_ref) | InlineableRef::Sibling { sibling: agent_ref } => {
                self.agents.resolve(agent_ref).await.map_err(judge_error)?
            }
            InlineableRef::Inline(spec) => (**spec).clone(),
            InlineableRef::Path(path) => {
                return Err(JudgeError::Terminal {
                    reason: format!(
                        "WYRD_REGISTRY_400_UNRESOLVED_PATH_REF: judge Agent path `{}` must be resolved by the loader",
                        path.display()
                    ),
                });
            }
        };
        validate_judge_agent(&spec)?;

        let prompt = match &spec.prompt {
            InlineableRef::Inline(prompt) => Prompt::from_native((**prompt).clone()),
            InlineableRef::Ref(prompt_ref)
            | InlineableRef::Sibling {
                sibling: prompt_ref,
            } => self
                .prompts
                .resolve(prompt_ref)
                .await
                .map_err(judge_error)?,
            InlineableRef::Path(path) => {
                return Err(JudgeError::Terminal {
                    reason: format!(
                        "WYRD_REGISTRY_400_UNRESOLVED_PATH_REF: judge Prompt path `{}` must be resolved by the loader",
                        path.display()
                    ),
                });
            }
        };
        if !prompt.native().yields_structured_output() {
            return Err(JudgeError::Terminal {
                reason: "judge Agent prompt must require structured JSON output".to_owned(),
            });
        }

        let id = judge_ref.as_card_ref().map_or_else(
            || "inline-judge".to_owned(),
            |reference| reference.name.to_string(),
        );
        let mut agent = Agent::new(prompt)
            .with_id(id)
            .with_tools(std::iter::empty())
            .with_run_config(run_config_from_agent_run_config_spec(&spec.run_config))
            .with_execution_route(spec.execution_route.clone());
        match (&spec.execution_route, &self.gateway) {
            (ExecutionRoute::WyrdGateway { .. }, Some(caller)) => {
                agent = agent.with_gateway_caller(Arc::clone(caller));
            }
            (ExecutionRoute::Native {}, None) => {}
            (ExecutionRoute::WyrdGateway { .. }, None) => {
                return Err(JudgeError::Terminal {
                    reason: "judge Agent routes through the Wyrd gateway, which this invoker \
                             is not configured with"
                        .to_owned(),
                });
            }
            (ExecutionRoute::Native {}, Some(_)) => {
                return Err(JudgeError::Terminal {
                    reason: "server-controlled judge execution refuses a native judge Agent \
                             route; declare a wyrd_gateway route"
                        .to_owned(),
                });
            }
        }
        let agent = Arc::new(agent);
        *cached = Some((judge_ref.clone(), Arc::clone(&agent)));
        Ok(agent)
    }
}

#[async_trait]
impl JudgeInvoker for SkaldJudgeInvoker {
    /// Ask the judge Agent's Prompt once and return its structured answer.
    ///
    /// A judge is constrained to one iteration, no tools, and no session, so
    /// it is one model call for every provider: the Prompt renders with the
    /// context variables (media bound on a per-call copy), dispatches once to
    /// the Agent's providers (the gateway registry when one is configured)
    /// under [`Self::call_deadline`], and the provider-neutral
    /// [`skald_spec::ResponseAdapter::structured_output`] becomes the
    /// judgment.
    ///
    /// # Errors
    /// Returns [`JudgeError::Terminal`] for an invalid judge or a Prompt that
    /// does not render, [`JudgeError::Timeout`] past the deadline, the
    /// provider failure classified by [`map_provider_error`], and
    /// [`JudgeError::InvalidStructuredOutput`] when the answer is not a JSON
    /// object.
    async fn invoke(
        &self,
        judge: &InlineableRef<AgentSpec>,
        context: Value,
        media: &MediaBindings,
    ) -> Result<Value, JudgeError> {
        let agent = self.agent_for(judge).await?;
        let bound;
        let prompt = if media.is_empty() {
            agent.prompt().as_ref()
        } else {
            bound = self.bind_media(agent.prompt(), media).await?;
            &bound
        };
        let vars = context_variables(prompt, &context);
        let borrowed = borrowed_pairs(&vars);

        let request = prompt
            .render_native(&borrowed)
            .map_err(|error| JudgeError::Terminal {
                reason: format!("judge Prompt does not render: {error}"),
            })?;
        let gateway = agent
            .route_registry(prompt)
            .map_err(|error| JudgeError::Terminal {
                reason: error.to_string(),
            })?;
        let providers = gateway
            .as_ref()
            .unwrap_or_else(|| agent.effective_providers(&self.providers));
        let call = skald_runtime::dispatch(providers, prompt.native().provider(), request);
        let response = tokio::time::timeout(self.call_deadline, call)
            .await
            .map_err(|_| JudgeError::Timeout {
                elapsed_ms: elapsed_millis(self.call_deadline),
            })?
            .map_err(map_provider_error)?;

        let answer = response.adapter().structured_output().ok_or_else(|| {
            JudgeError::InvalidStructuredOutput {
                reason: "judge answer carries no structured JSON output".to_owned(),
            }
        })?;
        serde_json::from_str::<Map<String, Value>>(answer.get())
            .map(Value::Object)
            .map_err(|error| JudgeError::InvalidStructuredOutput {
                reason: error.to_string(),
            })
    }
}

fn validate_judge_agent(spec: &AgentSpec) -> Result<(), JudgeError> {
    if !spec.tool_names.is_empty() {
        return Err(JudgeError::Terminal {
            reason: "judge Agent must not declare tools or delegation".to_owned(),
        });
    }
    if spec.run_config.max_iterations != Some(1) {
        return Err(JudgeError::Terminal {
            reason: "judge Agent max_iterations must be explicitly set to 1".to_owned(),
        });
    }
    if spec.run_config.session_recent_limit.is_some() {
        return Err(JudgeError::Terminal {
            reason: "judge Agent must not use session state".to_owned(),
        });
    }
    Ok(())
}

/// Classify a judge model-call failure with the shared Agent block
/// classification.
fn map_provider_error(error: SkaldRuntimeError) -> JudgeError {
    judge_error(block_error(AgentError::Provider(error)))
}

/// The judge form of a shared block failure.
fn judge_error(error: BlockError) -> JudgeError {
    match error {
        BlockError::Retryable { reason } => JudgeError::Retryable { reason },
        BlockError::Terminal { reason } => JudgeError::Terminal { reason },
    }
}

fn elapsed_millis(duration: Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}

fn context_variables(prompt: &Prompt, value: &Value) -> Vec<(String, String)> {
    let variables = &prompt.native().variables;
    match value {
        Value::Object(map) => variables
            .iter()
            .filter_map(|name| map.get(name).map(|value| (name.clone(), render(value))))
            .collect(),
        scalar => {
            let name = variables
                .iter()
                .find(|name| name.as_str() == "context")
                .or_else(|| (variables.len() == 1).then(|| &variables[0]));
            name.map(|name| vec![(name.clone(), render(scalar))])
                .unwrap_or_default()
        }
    }
}

fn borrowed_pairs(pairs: &[(String, String)]) -> Vec<(&str, &str)> {
    pairs
        .iter()
        .map(|(key, value)| (key.as_str(), value.as_str()))
        .collect()
}
