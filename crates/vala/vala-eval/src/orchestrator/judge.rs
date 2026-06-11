//! Skald-backed [`crate::JudgeInvoker`] implementation.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use serde_json::Value;
use skald_agent::{Agent, AgentError};
use skald_prompt::Prompt;
use skald_runtime::ProviderRegistry;
use wyrd_spec::reference::CardRef;

use crate::{JudgeError, JudgeInvoker};

/// Resolves a Prompt card reference into a runtime Skald prompt.
#[async_trait]
pub trait PromptCardResolver: Send + Sync {
    /// Resolve a prompt card reference.
    ///
    /// # Errors
    /// Returns [`JudgeError`] if the prompt cannot be loaded.
    async fn resolve(&self, judge_ref: &CardRef) -> Result<Prompt, JudgeError>;
}

/// Skald-backed judge invoker.
pub struct SkaldJudgeInvoker {
    agent: Arc<Agent>,
    providers: Arc<ProviderRegistry>,
    resolver: Arc<dyn PromptCardResolver>,
    /// Per-attempt deadline.
    pub call_deadline: Duration,
}

impl SkaldJudgeInvoker {
    /// Construct a Skald judge invoker.
    #[must_use]
    pub fn new(
        agent: Arc<Agent>,
        providers: Arc<ProviderRegistry>,
        resolver: Arc<dyn PromptCardResolver>,
    ) -> Self {
        Self {
            agent,
            providers,
            resolver,
            call_deadline: Duration::from_secs(60),
        }
    }
}

#[async_trait]
impl JudgeInvoker for SkaldJudgeInvoker {
    async fn invoke(&self, judge: &CardRef, context: Value) -> Result<Value, JudgeError> {
        let prompt = self.resolver.resolve(judge).await?;
        let vars = flatten_object(&context);
        let borrowed = borrowed_pairs(&vars);

        let run = tokio::time::timeout(
            self.call_deadline,
            self.agent
                .run_prompt(self.providers.as_ref(), &prompt, &borrowed, None),
        )
        .await
        .map_err(|_| JudgeError::Timeout {
            elapsed_ms: elapsed_millis(self.call_deadline),
        })?
        .map_err(map_agent_error)?;

        let map = run
            .structured_output
            .ok_or_else(|| JudgeError::InvalidStructuredOutput {
                reason: "judge prompt produced no structured output".to_owned(),
            })?;
        Ok(Value::Object(map))
    }
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

fn flatten_object(value: &Value) -> Vec<(String, String)> {
    let Value::Object(map) = value else {
        return Vec::new();
    };
    map.iter()
        .map(|(key, value)| {
            let rendered = match value {
                Value::String(value) => value.clone(),
                other => other.to_string(),
            };
            (key.clone(), rendered)
        })
        .collect()
}

fn borrowed_pairs(pairs: &[(String, String)]) -> Vec<(&str, &str)> {
    pairs
        .iter()
        .map(|(key, value)| (key.as_str(), value.as_str()))
        .collect()
}
