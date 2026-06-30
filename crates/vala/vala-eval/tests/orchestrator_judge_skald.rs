mod orchestrator_support;

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use orchestrator_support::{judge_ref, registry_returning_text, structured_prompt};
use serde_json::{Value, json};
use skald_agent::Agent;
use skald_providers::{ProviderError, ProviderStream};
use skald_runtime::{Provider, ProviderRegistry};
use skald_spec::{ProviderName, ProviderRequest, ProviderResponse};
use vala_eval::orchestrator::{PromptCardResolver, SkaldJudgeInvoker};
use vala_eval::{JudgeError, JudgeInvoker};
use wyrd_spec::reference::CardRef;

struct StaticResolver {
    prompt: skald_prompt::Prompt,
}

#[async_trait]
impl PromptCardResolver for StaticResolver {
    async fn resolve(&self, _judge_ref: &CardRef) -> Result<skald_prompt::Prompt, JudgeError> {
        Ok(self.prompt.clone())
    }
}

fn invoker_with_registry(providers: Arc<ProviderRegistry>) -> SkaldJudgeInvoker {
    let prompt = structured_prompt();
    let agent = Arc::new(Agent::new(prompt.clone()));
    SkaldJudgeInvoker::new(agent, providers, Arc::new(StaticResolver { prompt }))
}

#[tokio::test(flavor = "multi_thread")]
async fn skald_judge_invoker_returns_structured_output() {
    let invoker = invoker_with_registry(registry_returning_text(r#"{"passed":true}"#));

    let value = invoker
        .invoke(&judge_ref(), json!({"response": "DONE"}))
        .await
        .expect("judge succeeds");

    assert_eq!(value, json!({"passed": true}));
}

#[tokio::test(flavor = "multi_thread")]
async fn provider_failure_maps_to_retryable() {
    let providers = Arc::new(ProviderRegistry::new());
    let invoker = invoker_with_registry(providers);

    let error = invoker
        .invoke(&judge_ref(), json!({"response": "DONE"}))
        .await
        .expect_err("missing provider is retryable");

    assert!(matches!(error, JudgeError::Retryable { .. }));
}

#[tokio::test(flavor = "multi_thread")]
async fn malformed_structured_output_maps_to_invalid_output() {
    let invoker = invoker_with_registry(registry_returning_text("not json"));

    let error = invoker
        .invoke(&judge_ref(), json!({"response": "DONE"}))
        .await
        .expect_err("malformed structured output rejected");

    assert!(matches!(error, JudgeError::InvalidStructuredOutput { .. }));
}

#[tokio::test(flavor = "multi_thread")]
async fn judge_call_deadline_maps_to_timeout() {
    let prompt = structured_prompt();
    let agent = Arc::new(Agent::new(prompt.clone()));
    let mut registry = ProviderRegistry::new();
    registry.register(Arc::new(SlowProvider));
    let mut invoker = SkaldJudgeInvoker::new(
        agent,
        Arc::new(registry),
        Arc::new(StaticResolver { prompt }),
    );
    invoker.call_deadline = Duration::from_millis(1);

    let error = invoker
        .invoke(&judge_ref(), json!({"response": "DONE"}))
        .await
        .expect_err("deadline should fire");

    assert!(matches!(error, JudgeError::Timeout { .. }));
}

struct SlowProvider;

#[async_trait]
impl Provider for SlowProvider {
    async fn send(&self, _request: ProviderRequest) -> Result<ProviderResponse, ProviderError> {
        tokio::time::sleep(Duration::from_millis(100)).await;
        Err(ProviderError::timeout("openai"))
    }

    async fn stream(&self, _request: ProviderRequest) -> Result<ProviderStream, ProviderError> {
        Err(ProviderError::timeout("openai"))
    }

    fn name(&self) -> ProviderName {
        ProviderName::OpenAi
    }
}

#[allow(dead_code)]
fn _assert_json_value(_: &Value) {}
