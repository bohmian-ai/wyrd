//! Skald-backed Agent invoker for constrained LLM judge execution.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use serde_json::Value;
use skald_agent::{Agent, AgentError, run_config_from_agent_run_config_spec};
use skald_prompt::Prompt;
use skald_runtime::ProviderRegistry;
use tokio::sync::Mutex;
use wyrd_spec::card::agent::AgentSpec;
use wyrd_spec::reference::{CardRef, InlineableRef};

use crate::{JudgeError, JudgeInvoker};

/// Resolves a durable Agent card into its pure Agent spec.
#[async_trait]
pub trait AgentCardResolver: Send + Sync {
    /// Resolve an Agent card reference.
    async fn resolve(&self, agent_ref: &CardRef) -> Result<AgentSpec, JudgeError>;
}

/// Resolves a durable Prompt card into a runtime Skald prompt.
#[async_trait]
pub trait PromptCardResolver: Send + Sync {
    /// Resolve a Prompt card reference.
    async fn resolve(&self, prompt_ref: &CardRef) -> Result<Prompt, JudgeError>;
}

/// Skald-backed invoker for one constrained Agent judge per Eval run.
pub struct SkaldJudgeInvoker {
    providers: Arc<ProviderRegistry>,
    agents: Arc<dyn AgentCardResolver>,
    prompts: Arc<dyn PromptCardResolver>,
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
            cached_agent: Mutex::new(None),
            call_deadline: Duration::from_secs(60),
        }
    }

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
            InlineableRef::Ref(agent_ref) => self.agents.resolve(agent_ref).await?,
            InlineableRef::Inline(spec) => (**spec).clone(),
            InlineableRef::Path(path) => {
                return Err(JudgeError::Terminal {
                    reason: format!(
                        "WYRD_REGISTRY_400_UNRESOLVED_PATH_REF: judge Agent path `{}` must be resolved by the loader",
                        path.display()
                    ),
                });
            }
            InlineableRef::Sibling { .. } => {
                return Err(JudgeError::Terminal {
                    reason: "WYRD_REGISTRY_400_UNRESOLVED_SIBLING_REF: judge Agent sibling reference must be bound by the registry before execution".to_owned(),
                });
            }
        };
        validate_judge_agent(&spec)?;

        let prompt = match &spec.prompt {
            InlineableRef::Inline(prompt) => Prompt::from_native((**prompt).clone()),
            InlineableRef::Ref(prompt_ref) => self.prompts.resolve(prompt_ref).await?,
            InlineableRef::Path(path) => {
                return Err(JudgeError::Terminal {
                    reason: format!(
                        "WYRD_REGISTRY_400_UNRESOLVED_PATH_REF: judge Prompt path `{}` must be resolved by the loader",
                        path.display()
                    ),
                });
            }
            InlineableRef::Sibling { .. } => {
                return Err(JudgeError::Terminal {
                    reason: "WYRD_REGISTRY_400_UNRESOLVED_SIBLING_REF: judge Prompt sibling reference must be bound by the registry before execution".to_owned(),
                });
            }
        };
        if !matches!(
            prompt.native().response_type,
            skald_spec::ResponseType::JsonSchema { .. }
        ) {
            return Err(JudgeError::Terminal {
                reason: "judge Agent prompt must require structured JSON output".to_owned(),
            });
        }

        let id = judge_ref
            .as_card_ref()
            .map(|reference| reference.name.to_string())
            .unwrap_or_else(|| "inline-judge".to_owned());
        let agent = Agent::new(prompt)
            .with_id(id)
            .with_tools(std::iter::empty())
            .with_run_config(run_config_from_agent_run_config_spec(&spec.run_config));
        let agent = Arc::new(agent);
        *cached = Some((judge_ref.clone(), Arc::clone(&agent)));
        Ok(agent)
    }
}

#[async_trait]
impl JudgeInvoker for SkaldJudgeInvoker {
    async fn invoke(
        &self,
        judge: &InlineableRef<AgentSpec>,
        context: Value,
    ) -> Result<Value, JudgeError> {
        let agent = self.agent_for(judge).await?;
        let vars = context_variables(agent.prompt().as_ref(), &context);
        let borrowed = borrowed_pairs(&vars);

        let run = tokio::time::timeout(
            self.call_deadline,
            agent.run_prompt(self.providers.as_ref(), agent.prompt(), &borrowed, None),
        )
        .await
        .map_err(|_| JudgeError::Timeout {
            elapsed_ms: elapsed_millis(self.call_deadline),
        })?
        .map_err(map_agent_error)?;

        let map = run
            .structured_output
            .ok_or_else(|| JudgeError::InvalidStructuredOutput {
                reason: "judge Agent produced no structured output".to_owned(),
            })?;
        Ok(Value::Object(map))
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

fn map_agent_error(error: AgentError) -> JudgeError {
    match error {
        AgentError::Timeout { duration } => JudgeError::Timeout {
            elapsed_ms: elapsed_millis(duration),
        },
        AgentError::StructuredOutputDecode { detail, .. } => {
            JudgeError::InvalidStructuredOutput { reason: detail }
        }
        AgentError::Provider(source) => JudgeError::Retryable {
            reason: source.to_string(),
        },
        AgentError::Prompt { detail, .. } | AgentError::InvalidArgument { detail, .. } => {
            JudgeError::Terminal { reason: detail }
        }
        AgentError::ProviderMismatch { .. } => JudgeError::Terminal {
            reason: error.to_string(),
        },
        other => JudgeError::Terminal {
            reason: other.to_string(),
        },
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

fn render(value: &Value) -> String {
    match value {
        Value::String(value) => value.clone(),
        other => other.to_string(),
    }
}

fn borrowed_pairs(pairs: &[(String, String)]) -> Vec<(&str, &str)> {
    pairs
        .iter()
        .map(|(key, value)| (key.as_str(), value.as_str()))
        .collect()
}
