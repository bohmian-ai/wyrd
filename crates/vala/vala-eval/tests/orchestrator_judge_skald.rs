mod orchestrator_support;

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use async_trait::async_trait;
use orchestrator_support::{
    judge_ref, registry_returning_text, registry_returning_texts, structured_prompt,
};
use serde_json::{Value, json};
use skald_providers::{ProviderError, ProviderStream};
use skald_runtime::{Provider, ProviderRegistry};
use skald_spec::{ProviderName, ProviderRequest, ProviderResponse};
use vala_eval::orchestrator::{AgentCardResolver, PromptCardResolver, SkaldJudgeInvoker};
use vala_eval::{JudgeError, JudgeInvoker};
use wyrd_spec::card::agent::{AgentRunConfigSpec, AgentSpec};
use wyrd_spec::reference::{CardRef, InlineableRef};

struct StaticResolver {
    prompt: skald_prompt::Prompt,
}

struct StaticAgentResolver {
    prompt: skald_prompt::Prompt,
    calls: Option<Arc<AtomicUsize>>,
}

#[async_trait]
impl AgentCardResolver for StaticAgentResolver {
    async fn resolve(&self, _agent_ref: &CardRef) -> Result<AgentSpec, JudgeError> {
        if let Some(calls) = &self.calls {
            calls.fetch_add(1, Ordering::Relaxed);
        }
        Ok(AgentSpec {
            prompt: InlineableRef::Inline(Box::new(self.prompt.clone().into_native())),
            tool_names: Vec::new(),
            run_config: AgentRunConfigSpec {
                max_iterations: Some(1),
                ..AgentRunConfigSpec::default()
            },
            publishes_to: Vec::new(),
        })
    }
}

#[async_trait]
impl PromptCardResolver for StaticResolver {
    async fn resolve(&self, _judge_ref: &CardRef) -> Result<skald_prompt::Prompt, JudgeError> {
        Ok(self.prompt.clone())
    }
}

fn invoker_with_registry(providers: Arc<ProviderRegistry>) -> SkaldJudgeInvoker {
    let prompt = structured_prompt();
    SkaldJudgeInvoker::new(
        providers,
        Arc::new(StaticAgentResolver {
            prompt: prompt.clone(),
            calls: None,
        }),
        Arc::new(StaticResolver { prompt }),
    )
}

fn judge_agent_ref() -> InlineableRef<AgentSpec> {
    judge_ref().into()
}

fn sibling_judge_agent_ref() -> InlineableRef<AgentSpec> {
    InlineableRef::Sibling {
        sibling: judge_ref(),
    }
}

fn inline_agent_ref(prompt: skald_prompt::Prompt) -> InlineableRef<AgentSpec> {
    InlineableRef::Inline(Box::new(AgentSpec {
        prompt: InlineableRef::Inline(Box::new(prompt.into_native())),
        tool_names: Vec::new(),
        run_config: AgentRunConfigSpec {
            max_iterations: Some(1),
            ..AgentRunConfigSpec::default()
        },
        publishes_to: Vec::new(),
    }))
}

#[tokio::test(flavor = "multi_thread")]
async fn skald_judge_invoker_returns_structured_output() {
    let invoker = invoker_with_registry(registry_returning_text(r#"{"passed":true}"#));

    let value = invoker
        .invoke(&judge_agent_ref(), json!({"response": "DONE"}))
        .await
        .expect("judge succeeds");

    assert_eq!(value, json!({"passed": true}));
}

#[tokio::test(flavor = "multi_thread")]
async fn skald_judge_invoker_resolves_sibling_agent_and_prompt() {
    let invoker = invoker_with_registry(registry_returning_text(r#"{"passed":true}"#));

    let value = invoker
        .invoke(&sibling_judge_agent_ref(), json!({"response": "DONE"}))
        .await
        .expect("sibling judge succeeds");

    assert_eq!(value, json!({"passed": true}));
}

#[tokio::test(flavor = "multi_thread")]
async fn provider_failure_maps_to_retryable() {
    let providers = Arc::new(ProviderRegistry::new());
    let invoker = invoker_with_registry(providers);

    let error = invoker
        .invoke(&judge_agent_ref(), json!({"response": "DONE"}))
        .await
        .expect_err("missing provider is retryable");

    assert!(matches!(error, JudgeError::Retryable { .. }));
}

#[tokio::test(flavor = "multi_thread")]
async fn malformed_structured_output_maps_to_invalid_output() {
    let invoker = invoker_with_registry(registry_returning_text("not json"));

    let error = invoker
        .invoke(&judge_agent_ref(), json!({"response": "DONE"}))
        .await
        .expect_err("malformed structured output rejected");

    assert!(matches!(error, JudgeError::InvalidStructuredOutput { .. }));
}

#[tokio::test(flavor = "multi_thread")]
async fn judge_call_deadline_maps_to_timeout() {
    let prompt = structured_prompt();
    let mut registry = ProviderRegistry::new();
    registry.register(Arc::new(SlowProvider));
    let mut invoker = SkaldJudgeInvoker::new(
        Arc::new(registry),
        Arc::new(StaticAgentResolver {
            prompt: prompt.clone(),
            calls: None,
        }),
        Arc::new(StaticResolver { prompt }),
    );
    invoker.call_deadline = Duration::from_millis(1);

    let error = invoker
        .invoke(&judge_agent_ref(), json!({"response": "DONE"}))
        .await
        .expect_err("deadline should fire");

    assert!(matches!(error, JudgeError::Timeout { .. }));
}

#[tokio::test(flavor = "multi_thread")]
async fn inline_agent_judge_executes_with_scalar_context() {
    let prompt = structured_prompt();
    let invoker = SkaldJudgeInvoker::new(
        registry_returning_text(r#"{"passed":true}"#),
        Arc::new(StaticAgentResolver {
            prompt: prompt.clone(),
            calls: None,
        }),
        Arc::new(StaticResolver {
            prompt: prompt.clone(),
        }),
    );

    let value = invoker
        .invoke(&inline_agent_ref(prompt), json!("DONE"))
        .await
        .expect("inline Agent judge succeeds");
    assert_eq!(value, json!({"passed": true}));
}

#[tokio::test(flavor = "multi_thread")]
async fn unsafe_agent_judge_configuration_fails_closed() {
    let prompt = structured_prompt();
    let mut agent = inline_agent_ref(prompt);
    let InlineableRef::Inline(spec) = &mut agent else {
        unreachable!("fixture is inline")
    };
    spec.tool_names.push("delegate".to_owned());
    spec.run_config.max_iterations = Some(2);

    let invoker = invoker_with_registry(registry_returning_text(r#"{"passed":true}"#));
    let error = invoker
        .invoke(&agent, json!({"response": "DONE"}))
        .await
        .expect_err("unsafe judge must be rejected");
    assert!(matches!(error, JudgeError::Terminal { .. }));
}

#[tokio::test(flavor = "multi_thread")]
async fn agent_resolution_is_cached_for_an_eval_run() {
    let calls = Arc::new(AtomicUsize::new(0));
    let prompt = structured_prompt();
    let invoker = SkaldJudgeInvoker::new(
        registry_returning_texts(&[r#"{"passed":true}"#, r#"{"passed":true}"#]),
        Arc::new(StaticAgentResolver {
            prompt: prompt.clone(),
            calls: Some(Arc::clone(&calls)),
        }),
        Arc::new(StaticResolver { prompt }),
    );
    let judge = judge_agent_ref();

    invoker
        .invoke(&judge, json!({"response": "one"}))
        .await
        .expect("first judge succeeds");
    invoker
        .invoke(&judge, json!({"response": "two"}))
        .await
        .expect("second judge succeeds");
    assert_eq!(calls.load(Ordering::Relaxed), 1);
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

fn _assert_json_value(_: &Value) {}
