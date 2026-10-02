//! Agent run timeout and telemetry coverage.
//!
//! Each case drives a real Agent run against a scripted provider and proves
//! its outcome through the returned result, the journal, and the `tracing`
//! spans captured by `wyrd_telemetry`'s production-shaped test pipeline.
//! Spans are selected by a per-test Agent id, so cases may share the one
//! process-wide capture.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use serde_json::{Value, json};
use skald_agent::{
    Agent, AgentError, FinishReason, Journal, JournalError, JournalEvent, RunConfig,
};
use skald_prompt::Prompt;
use skald_providers::{ProviderError, ProviderStream};
use skald_runtime::{Provider, ProviderRegistry};
use skald_spec::wire::openai_chat::{
    OpenAiChatChoice, OpenAiChatMessage, OpenAiChatRequest, OpenAiChatResponse, OpenAiChatSettings,
    OpenAiMessageContent, OpenAiToolCall, OpenAiToolFunctionCall,
};
use skald_spec::{
    Prompt as SpecPrompt, ProviderName, ProviderRequest, ProviderResponse, ResponseType,
};
use skald_tool::{AgentTool, ToolError};
use tokio::sync::Notify;
use wyrd_telemetry::{
    CapturedSpan, CapturedSpanStatus, TelemetryConfig, TelemetryGuard, TestTraceCapture,
    init_test_capture,
};

/// Process-wide trace capture and the guard that keeps its provider alive.
static CAPTURE: OnceLock<(TelemetryGuard, TestTraceCapture)> = OnceLock::new();

/// Install the production-shaped capture pipeline once and return its handle.
///
/// # Panics
/// Panics when another global subscriber is already installed.
fn capture() -> &'static TestTraceCapture {
    &CAPTURE
        .get_or_init(|| {
            init_test_capture(TelemetryConfig::default()).expect("trace capture installs once")
        })
        .1
}

/// The finished `invoke_agent` span for `agent_id` and its child spans.
///
/// # Panics
/// Panics unless exactly one `invoke_agent` span carries `agent_id`.
fn agent_spans(agent_id: &str) -> (CapturedSpan, Vec<CapturedSpan>) {
    let spans = capture().finished_since(0);
    let mut roots = spans.iter().filter(|span| {
        span.name == "invoke_agent"
            && span.attributes.get("gen_ai.agent.id").map(String::as_str) == Some(agent_id)
    });
    let root = roots.next().expect("one invoke_agent span").clone();
    assert!(roots.next().is_none(), "one invoke_agent span per agent id");
    let children = spans
        .into_iter()
        .filter(|span| span.parent_span_id == root.span_id)
        .collect();
    (root, children)
}

/// A run exceeding its timeout returns the stable timeout error, journals one
/// terminal error, and marks its `invoke_agent` span failed with that code.
#[tokio::test]
async fn agent_run_timeout_terminates_cleanly() {
    capture();
    let journal = Arc::new(RecordingJournal::new());
    let providers = registry(RecordingProvider::new(vec![openai_tool_call_response(
        vec![tool_call("c1", "waiter", json!({}))],
    )]));
    let started = Arc::new(Notify::new());
    let release = Arc::new(Notify::new());
    let timeout = Duration::from_millis(50);
    let agent = Agent::from_resolved("timeout-agent", test_prompt())
        .add_tool(Arc::new(ControlledTool {
            started,
            release,
            delay: Duration::from_secs(1),
        }))
        .with_journal(journal.clone())
        .with_run_config(RunConfig {
            max_iterations: 3,
            timeout: Some(timeout),
            ..Default::default()
        });

    let error = agent
        .run_with(&providers, None, "hello")
        .await
        .expect_err("run should time out");

    assert!(matches!(error, AgentError::Timeout { duration } if duration == timeout));
    assert_eq!(error.code(), "SKALD_AGENT_504_TIMEOUT");
    let journal_errors: Vec<_> = journal
        .events()
        .into_iter()
        .filter(|event| matches!(event, JournalEvent::AgentError { .. }))
        .collect();
    assert_eq!(journal_errors.len(), 1);
    assert!(matches!(
        journal_errors.first(),
        Some(JournalEvent::AgentError { code, .. }) if code == "SKALD_AGENT_504_TIMEOUT"
    ));
    let (root, children) = agent_spans("timeout-agent");
    assert_eq!(
        root.attributes.get("error.type").map(String::as_str),
        Some("SKALD_AGENT_504_TIMEOUT")
    );
    assert!(matches!(root.status, CapturedSpanStatus::Error(_)));
    assert!(root.duration_nanos >= u64::try_from(timeout.as_nanos()).unwrap_or(u64::MAX));
    let mut names: Vec<_> = children.iter().map(|span| span.name.as_str()).collect();
    names.sort_unstable();
    assert_eq!(names, ["chat", "execute_tool"]);
}

/// Without a timeout a slow tool runs to completion: no terminal error is
/// journaled and the `invoke_agent` span carries no error.
#[tokio::test]
async fn agent_run_no_timeout_runs_to_completion() {
    capture();
    let journal = Arc::new(RecordingJournal::new());
    let providers = registry(RecordingProvider::new(vec![
        openai_tool_call_response(vec![tool_call("c1", "slow", json!({}))]),
        openai_text_response("done"),
    ]));
    let agent = Agent::from_resolved("patient-agent", test_prompt())
        .add_tool(Arc::new(SlowTool {
            delay: Duration::from_millis(200),
        }))
        .with_journal(journal.clone())
        .with_run_config(RunConfig {
            max_iterations: 3,
            timeout: None,
            ..Default::default()
        });
    let started_at = Instant::now();

    let run = agent
        .run_with(&providers, None, "hello")
        .await
        .expect("run ok");

    assert_eq!(run.finish_reason, FinishReason::ModelStopped);
    assert!(started_at.elapsed() >= Duration::from_millis(200));
    assert!(
        !journal
            .events()
            .iter()
            .any(|event| matches!(event, JournalEvent::AgentError { .. }))
    );
    let (root, _) = agent_spans("patient-agent");
    assert!(!root.attributes.contains_key("error.type"));
    assert!(!matches!(root.status, CapturedSpanStatus::Error(_)));
}

/// One tool-calling run emits `invoke_agent` with one `chat` span per model
/// call and one `execute_tool` span per tool call as children, using GenAI
/// attribute names; a failing tool records its stable code; no span carries
/// the prompt, input, tool arguments, or output.
#[tokio::test]
async fn agent_run_emits_genai_spans_without_payloads() {
    capture();
    let providers = registry(RecordingProvider::new(vec![
        openai_tool_call_response(vec![
            tool_call(
                "c1",
                "lookup",
                json!({ "secret_arg": "argument-pii-marker" }),
            ),
            tool_call("c2", "missing", json!({})),
        ]),
        openai_text_response("final-output-pii-marker"),
    ]));
    let agent = Agent::from_resolved("span-agent", test_prompt())
        .add_tool(fixed_tool(
            "lookup",
            json!({ "result": "tool-output-pii-marker" }),
        ))
        .add_tool(Arc::new(FailingTool))
        .with_run_config(RunConfig {
            max_iterations: 3,
            ..Default::default()
        });

    let run = agent
        .run_with(&providers, None, "input-pii-marker")
        .await
        .expect("run ok");

    assert_eq!(run.output, "final-output-pii-marker");
    let (root, children) = agent_spans("span-agent");
    for (key, value) in [
        ("gen_ai.operation.name", "invoke_agent"),
        ("gen_ai.provider.name", "openai"),
        ("gen_ai.request.model", "gpt-4o"),
    ] {
        assert_eq!(root.attributes.get(key).map(String::as_str), Some(value));
    }
    let chats: Vec<_> = children.iter().filter(|span| span.name == "chat").collect();
    assert_eq!(chats.len(), 2);
    for chat in &chats {
        assert_eq!(
            chat.attributes
                .get("gen_ai.operation.name")
                .map(String::as_str),
            Some("chat")
        );
        assert_eq!(
            chat.attributes
                .get("gen_ai.request.model")
                .map(String::as_str),
            Some("gpt-4o")
        );
    }
    let mut tools: Vec<_> = children
        .iter()
        .filter(|span| span.name == "execute_tool")
        .map(|span| {
            (
                span.attributes["gen_ai.tool.name"].as_str(),
                span.attributes["gen_ai.tool.call.id"].as_str(),
                span.attributes.get("error.type").map(String::as_str),
            )
        })
        .collect();
    tools.sort_unstable();
    assert_eq!(
        tools,
        [
            ("lookup", "c1", None),
            ("missing", "c2", Some("SKALD_TOOL_500_CALL")),
        ]
    );
    for span in std::iter::once(&root).chain(&children) {
        for value in span.attributes.values() {
            assert!(!value.contains("pii-marker"), "{}: {value}", span.name);
        }
    }
}

#[derive(Clone, Default)]
struct RecordingJournal {
    events: Arc<Mutex<Vec<JournalEvent>>>,
}

impl RecordingJournal {
    fn new() -> Self {
        Self::default()
    }

    fn events(&self) -> Vec<JournalEvent> {
        match self.events.lock() {
            Ok(guard) => guard.clone(),
            Err(poisoned) => poisoned.into_inner().clone(),
        }
    }
}

#[async_trait]
impl Journal for RecordingJournal {
    async fn append(&self, event: JournalEvent) -> Result<(), JournalError> {
        match self.events.lock() {
            Ok(mut guard) => guard.push(event),
            Err(poisoned) => poisoned.into_inner().push(event),
        }
        Ok(())
    }
}

#[derive(Clone)]
struct RecordingProvider {
    responses: Arc<Mutex<VecDeque<ProviderResponse>>>,
}

impl RecordingProvider {
    fn new(responses: Vec<ProviderResponse>) -> Self {
        Self {
            responses: Arc::new(Mutex::new(responses.into())),
        }
    }

    fn responses(&self) -> MutexGuard<'_, VecDeque<ProviderResponse>> {
        match self.responses.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }
}

#[async_trait]
impl Provider for RecordingProvider {
    async fn send(&self, _request: ProviderRequest) -> Result<ProviderResponse, ProviderError> {
        self.responses()
            .pop_front()
            .ok_or_else(|| ProviderError::bad_request("recording", "response queue is empty"))
    }

    async fn stream(&self, _request: ProviderRequest) -> Result<ProviderStream, ProviderError> {
        Err(ProviderError::bad_request(
            "recording",
            "recording provider does not stream",
        ))
    }

    fn name(&self) -> ProviderName {
        ProviderName::OpenAi
    }
}

#[derive(Debug)]
struct FixedTool {
    name: String,
    output: Value,
}

#[async_trait]
impl AgentTool for FixedTool {
    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> &str {
        "fixed test tool"
    }

    fn input_schema(&self) -> Value {
        json!({"type": "object", "additionalProperties": true})
    }

    fn output_schema(&self) -> Value {
        json!({})
    }

    async fn invoke(&self, _args: Value) -> Result<Value, ToolError> {
        Ok(self.output.clone())
    }
}

#[derive(Debug)]
struct ControlledTool {
    started: Arc<Notify>,
    release: Arc<Notify>,
    delay: Duration,
}

#[async_trait]
impl AgentTool for ControlledTool {
    fn name(&self) -> &str {
        "waiter"
    }

    fn description(&self) -> &str {
        "controlled test tool"
    }

    fn input_schema(&self) -> Value {
        json!({"type": "object"})
    }

    fn output_schema(&self) -> Value {
        json!({})
    }

    async fn invoke(&self, _args: Value) -> Result<Value, ToolError> {
        self.started.notify_one();
        if self.delay.is_zero() {
            self.release.notified().await;
        } else {
            tokio::time::sleep(self.delay).await;
        }
        Ok(json!({"ok": true}))
    }
}

/// Tool named `missing` whose every invocation fails.
#[derive(Debug)]
struct FailingTool;

#[async_trait]
impl AgentTool for FailingTool {
    /// Name the model calls.
    fn name(&self) -> &str {
        "missing"
    }

    /// Fixed description.
    fn description(&self) -> &str {
        "failing test tool"
    }

    /// Accept any object.
    fn input_schema(&self) -> Value {
        json!({"type": "object"})
    }

    /// No declared output.
    fn output_schema(&self) -> Value {
        json!({})
    }

    /// Fail with an invocation error.
    async fn invoke(&self, _args: Value) -> Result<Value, ToolError> {
        Err(ToolError::Invocation {
            detail: "tool failed".to_owned(),
            cause: None,
        })
    }
}

#[derive(Debug)]
struct SlowTool {
    delay: Duration,
}

#[async_trait]
impl AgentTool for SlowTool {
    fn name(&self) -> &str {
        "slow"
    }

    fn description(&self) -> &str {
        "slow test tool"
    }

    fn input_schema(&self) -> Value {
        json!({"type": "object"})
    }

    fn output_schema(&self) -> Value {
        json!({})
    }

    async fn invoke(&self, _args: Value) -> Result<Value, ToolError> {
        tokio::time::sleep(self.delay).await;
        Ok(json!({"ok": true}))
    }
}

/// Builds a registry holding only the scripted provider.
fn registry(provider: RecordingProvider) -> ProviderRegistry {
    let mut providers = ProviderRegistry::new();
    providers.register(Arc::new(provider));
    providers
}

/// Builds a tool that always returns `output`.
fn fixed_tool(name: &str, output: Value) -> Arc<dyn AgentTool> {
    Arc::new(FixedTool {
        name: name.to_owned(),
        output,
    })
}

/// Builds the minimal OpenAI chat Prompt every test agent runs.
fn test_prompt() -> Arc<Prompt> {
    let request = ProviderRequest::OpenAiChatCompletion(OpenAiChatRequest {
        model: "gpt-4o".to_owned(),
        messages: vec![OpenAiChatMessage {
            role: "system".to_owned(),
            content: Some(OpenAiMessageContent::Text("system".to_owned())),
            name: None,
            tool_calls: None,
            tool_call_id: None,
            refusal: None,
            annotations: Vec::new(),
            audio: None,
        }],
        response_format: None,
        stream: None,
        stream_options: None,
        tools: None,
        tool_choice: None,
        parallel_tool_calls: None,
        settings: OpenAiChatSettings::default(),
    });
    Arc::new(Prompt::from_native(
        SpecPrompt::new(request, "gpt-4o", None, ResponseType::Text).expect("test prompt builds"),
    ))
}

/// Builds a terminal OpenAI chat response carrying `text`.
fn openai_text_response(text: &str) -> ProviderResponse {
    ProviderResponse::OpenAiChatCompletion(OpenAiChatResponse {
        id: "resp_text".to_owned(),
        object: "chat.completion".to_owned(),
        created: 0,
        model: "gpt-4o".to_owned(),
        choices: vec![OpenAiChatChoice {
            index: 0,
            message: OpenAiChatMessage {
                role: "assistant".to_owned(),
                content: Some(OpenAiMessageContent::Text(text.to_owned())),
                name: None,
                tool_calls: None,
                tool_call_id: None,
                refusal: None,
                annotations: Vec::new(),
                audio: None,
            },
            finish_reason: Some("stop".to_owned()),
            logprobs: None,
        }],
        usage: None,
        system_fingerprint: None,
        service_tier: None,
    })
}

/// Builds an OpenAI chat response requesting `calls`.
fn openai_tool_call_response(calls: Vec<OpenAiToolCall>) -> ProviderResponse {
    ProviderResponse::OpenAiChatCompletion(OpenAiChatResponse {
        id: "resp_call".to_owned(),
        object: "chat.completion".to_owned(),
        created: 0,
        model: "gpt-4o".to_owned(),
        choices: vec![OpenAiChatChoice {
            index: 0,
            message: OpenAiChatMessage {
                role: "assistant".to_owned(),
                content: None,
                name: None,
                tool_calls: Some(calls),
                tool_call_id: None,
                refusal: None,
                annotations: Vec::new(),
                audio: None,
            },
            finish_reason: Some("tool_calls".to_owned()),
            logprobs: None,
        }],
        usage: None,
        system_fingerprint: None,
        service_tier: None,
    })
}

/// Builds one OpenAI function tool call.
fn tool_call(id: impl Into<String>, name: impl Into<String>, args: Value) -> OpenAiToolCall {
    OpenAiToolCall {
        id: id.into(),
        kind: "function".to_owned(),
        function: OpenAiToolFunctionCall {
            name: name.into(),
            arguments: args.to_string(),
        },
    }
}
