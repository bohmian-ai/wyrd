use crate::orchestrator_support;

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
use vala_eval::orchestrator::{
    AgentCardResolver, MediaResolver, PromptCardResolver, SkaldJudgeInvoker,
};
use vala_eval::{JudgeError, JudgeInvoker, MediaBindings};
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
            verified_by: Vec::new(),
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
        verified_by: Vec::new(),
    }))
}

#[tokio::test(flavor = "multi_thread")]
async fn skald_judge_invoker_returns_structured_output() {
    let invoker = invoker_with_registry(registry_returning_text(r#"{"passed":true}"#));

    let value = invoker
        .invoke(
            &judge_agent_ref(),
            json!({"response": "DONE"}),
            &MediaBindings::new(),
        )
        .await
        .expect("judge succeeds");

    assert_eq!(value, json!({"passed": true}));
}

#[tokio::test(flavor = "multi_thread")]
async fn skald_judge_invoker_resolves_sibling_agent_and_prompt() {
    let invoker = invoker_with_registry(registry_returning_text(r#"{"passed":true}"#));

    let value = invoker
        .invoke(
            &sibling_judge_agent_ref(),
            json!({"response": "DONE"}),
            &MediaBindings::new(),
        )
        .await
        .expect("sibling judge succeeds");

    assert_eq!(value, json!({"passed": true}));
}

#[tokio::test(flavor = "multi_thread")]
async fn provider_failure_maps_to_retryable() {
    let providers = Arc::new(ProviderRegistry::new());
    let invoker = invoker_with_registry(providers);

    let error = invoker
        .invoke(
            &judge_agent_ref(),
            json!({"response": "DONE"}),
            &MediaBindings::new(),
        )
        .await
        .expect_err("missing provider is retryable");

    assert!(matches!(error, JudgeError::Retryable { .. }));
}

#[tokio::test(flavor = "multi_thread")]
async fn malformed_structured_output_maps_to_invalid_output() {
    let invoker = invoker_with_registry(registry_returning_text("not json"));

    let error = invoker
        .invoke(
            &judge_agent_ref(),
            json!({"response": "DONE"}),
            &MediaBindings::new(),
        )
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
        .invoke(
            &judge_agent_ref(),
            json!({"response": "DONE"}),
            &MediaBindings::new(),
        )
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
        .invoke(
            &inline_agent_ref(prompt),
            json!("DONE"),
            &MediaBindings::new(),
        )
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
        .invoke(&agent, json!({"response": "DONE"}), &MediaBindings::new())
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
        .invoke(&judge, json!({"response": "one"}), &MediaBindings::new())
        .await
        .expect("first judge succeeds");
    invoker
        .invoke(&judge, json!({"response": "two"}), &MediaBindings::new())
        .await
        .expect("second judge succeeds");
    assert_eq!(calls.load(Ordering::Relaxed), 1);
}

/// Provider that records every request and answers with a passing verdict.
struct CapturingProvider {
    /// Requests received, in order.
    seen: std::sync::Mutex<Vec<ProviderRequest>>,
}

#[async_trait]
impl Provider for CapturingProvider {
    /// Record `request` and return a passing structured verdict.
    async fn send(&self, request: ProviderRequest) -> Result<ProviderResponse, ProviderError> {
        self.seen.lock().expect("capture lock").push(request);
        Ok(orchestrator_support::openai_text_response(
            r#"{"passed":true}"#,
        ))
    }

    /// Streaming is not used by the judge.
    async fn stream(&self, _request: ProviderRequest) -> Result<ProviderStream, ProviderError> {
        Err(ProviderError::timeout("openai"))
    }

    /// Serves the OpenAI provider slot.
    fn name(&self) -> ProviderName {
        ProviderName::OpenAi
    }
}

/// Resolver standing in for the server's authorized storage read.
struct InlineMediaResolver {
    /// Whether to refuse every descriptor.
    refuse: bool,
}

#[async_trait]
impl MediaResolver for InlineMediaResolver {
    /// Return fixed PNG bytes, or a terminal refusal.
    async fn resolve(
        &self,
        media: &wyrd_spec::vala::eval::media::MediaRef,
    ) -> Result<skald_spec::MediaRef, JudgeError> {
        if self.refuse {
            return Err(JudgeError::Terminal {
                reason: format!("media `{}` is not authorized", media.id.as_str()),
            });
        }
        Ok(skald_spec::MediaRef {
            kind: skald_spec::MediaKind::Image,
            source: skald_spec::MediaSource::Base64 {
                mime_type: "image/png".to_owned(),
                data: "aGVsbG8=".to_owned(),
            },
        })
    }
}

/// A judge Prompt that declares one `${media:shot}` placeholder.
fn media_prompt() -> skald_prompt::Prompt {
    let schema = json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["passed"],
        "properties": { "passed": { "type": "boolean" } }
    });
    skald_prompt::openai_chat(
        "gpt-test",
        skald_prompt::OpenAiChatOptions {
            messages: vec!["Judge: {{response}} ${media:shot}".to_owned()],
            variables: vec!["response".to_owned()],
            output: Some(
                skald_prompt::ResponseFormat::json_schema("judge_result", schema).expect("schema"),
            ),
            ..skald_prompt::OpenAiChatOptions::default()
        },
    )
    .expect("static prompt is valid")
}

/// One named image descriptor pointing at a private object URI.
fn shot_media() -> MediaBindings {
    MediaBindings::from_refs([wyrd_spec::vala::eval::media::MediaRef {
        id: wyrd_spec::ids::MediaBindingId::new("shot").expect("static binding id is valid"),
        kind: skald_spec::MediaKind::Image,
        uri: "s3://private-bucket/tenant/cards/card/shot.png".to_owned(),
        media_type: Some("image/png".to_owned()),
    }])
}

/// Named media reaches the provider as native inline bytes, never URI text,
/// and a resolver refusal fails the call without contacting the provider.
#[tokio::test(flavor = "multi_thread")]
async fn named_media_reaches_provider_as_native_content() {
    let provider = Arc::new(CapturingProvider {
        seen: std::sync::Mutex::new(Vec::new()),
    });
    let mut registry = ProviderRegistry::new();
    registry.register(Arc::clone(&provider) as Arc<dyn Provider>);
    let registry = Arc::new(registry);
    let prompt = media_prompt();
    let invoker = |refuse| {
        SkaldJudgeInvoker::new(
            Arc::clone(&registry),
            Arc::new(StaticAgentResolver {
                prompt: prompt.clone(),
                calls: None,
            }),
            Arc::new(StaticResolver {
                prompt: prompt.clone(),
            }),
        )
        .with_media_resolver(Arc::new(InlineMediaResolver { refuse }))
    };

    let value = invoker(false)
        .invoke(
            &judge_agent_ref(),
            json!({"response": "DONE"}),
            &shot_media(),
        )
        .await
        .expect("media judge succeeds");
    assert_eq!(value, json!({"passed": true}));
    let wire = serde_json::to_string(&provider.seen.lock().expect("capture lock")[0])
        .expect("request serializes");
    assert!(wire.contains("aGVsbG8="), "native bytes missing: {wire}");
    assert!(
        !wire.contains("private-bucket"),
        "private URI leaked: {wire}"
    );
    assert!(
        !wire.contains("${media:shot}"),
        "placeholder left unbound: {wire}"
    );

    let error = invoker(true)
        .invoke(
            &judge_agent_ref(),
            json!({"response": "DONE"}),
            &shot_media(),
        )
        .await
        .expect_err("refused media must fail");
    assert!(matches!(error, JudgeError::Terminal { .. }));
    assert_eq!(provider.seen.lock().expect("capture lock").len(), 1);

    let unresolvable = SkaldJudgeInvoker::new(
        Arc::clone(&registry),
        Arc::new(StaticAgentResolver {
            prompt: prompt.clone(),
            calls: None,
        }),
        Arc::new(StaticResolver { prompt }),
    );
    let error = unresolvable
        .invoke(
            &judge_agent_ref(),
            json!({"response": "DONE"}),
            &shot_media(),
        )
        .await
        .expect_err("media without a resolver must fail");
    assert!(matches!(error, JudgeError::Terminal { .. }));
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
