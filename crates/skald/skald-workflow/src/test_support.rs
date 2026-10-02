//! Deterministic fixtures shared by the crate's inline Workflow tests.
//!
//! [`ScriptedProvider`] answers OpenAI Chat requests from per-needle reply
//! queues, so results stay deterministic under any completion order, and it
//! records requests, in-flight concurrency, and abandoned calls.
//! [`RecordingObserver`] captures Workflow step events.

use std::collections::{BTreeMap, VecDeque};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use async_trait::async_trait;
use serde_json::{Value, json};
use skald_agent::{Agent, Observer};
use skald_prompt::{OpenAiChatOptions, ResponseFormat, openai_chat};
use skald_providers::{ProviderError, ProviderStream};
use skald_runtime::{Provider, ProviderRegistry};
use skald_spec::wire::openai_chat::{
    OpenAiChatChoice, OpenAiChatMessage, OpenAiChatResponse, OpenAiMessageContent, OpenAiToolCall,
    OpenAiToolFunctionCall,
};
use skald_spec::{ProviderName, ProviderRequest, ProviderResponse};
use skald_tool::{AgentTool, ToolError};
use wyrd_spec::card::workflow::WorkflowBinding;

/// Lock a fixture mutex, recovering a poisoned guard.
pub(crate) fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// One scripted provider answer.
#[derive(Clone)]
pub(crate) enum Reply {
    /// Assistant text.
    Text(String),
    /// Provider failure.
    Fail(ProviderError),
    /// Never answer.
    Hang,
    /// Answer after a virtual-time delay.
    After(Duration, Box<Reply>),
    /// Request one tool call with JSON arguments.
    ToolCall(String, Value),
}

/// Decrements the in-flight count and marks abandonment when dropped early.
struct InFlight<'a> {
    /// Owning provider.
    provider: &'a ScriptedProvider,
    /// Whether the call completed normally.
    finished: bool,
}

impl Drop for InFlight<'_> {
    /// Release the in-flight slot; count an abandoned call when unfinished.
    fn drop(&mut self) {
        self.provider.in_flight.fetch_sub(1, Ordering::SeqCst);
        if !self.finished {
            self.provider.abandoned.fetch_add(1, Ordering::SeqCst);
        }
    }
}

/// OpenAI Chat provider answering from per-needle reply queues.
#[derive(Default)]
pub(crate) struct ScriptedProvider {
    /// Rules: the first needle contained in the request text selects a queue.
    rules: Mutex<Vec<(String, VecDeque<Reply>)>>,
    /// Request texts in arrival order.
    requests: Mutex<Vec<String>>,
    /// Calls currently awaiting an answer.
    in_flight: AtomicUsize,
    /// Highest observed in-flight count.
    peak: AtomicUsize,
    /// Calls dropped before answering.
    abandoned: AtomicUsize,
}

impl ScriptedProvider {
    /// Build an empty script.
    pub(crate) fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// Queue `replies` for requests whose text contains `needle`.
    pub(crate) fn on(&self, needle: &str, replies: Vec<Reply>) {
        lock(&self.rules).push((needle.to_owned(), replies.into()));
    }

    /// Request texts received so far.
    pub(crate) fn requests(&self) -> Vec<String> {
        lock(&self.requests).clone()
    }

    /// Number of received requests whose text contains `needle`.
    pub(crate) fn count(&self, needle: &str) -> usize {
        self.requests()
            .iter()
            .filter(|text| text.contains(needle))
            .count()
    }

    /// Highest concurrent call count.
    pub(crate) fn peak(&self) -> usize {
        self.peak.load(Ordering::SeqCst)
    }

    /// Calls currently in flight.
    pub(crate) fn in_flight(&self) -> usize {
        self.in_flight.load(Ordering::SeqCst)
    }

    /// Calls dropped before they answered.
    pub(crate) fn abandoned(&self) -> usize {
        self.abandoned.load(Ordering::SeqCst)
    }

    /// Registry holding only this provider.
    pub(crate) fn registry(self: &Arc<Self>) -> ProviderRegistry {
        ProviderRegistry::new().with(Arc::clone(self) as Arc<dyn Provider>)
    }

    /// Pop the next reply for `text`.
    fn next(&self, text: &str) -> Reply {
        let mut rules = lock(&self.rules);
        rules
            .iter_mut()
            .find(|(needle, _)| text.contains(needle.as_str()))
            .and_then(|(_, replies)| replies.pop_front())
            .unwrap_or_else(|| {
                Reply::Fail(ProviderError::bad_request("scripted", "no scripted reply"))
            })
    }
}

#[async_trait]
impl Provider for ScriptedProvider {
    /// Record the request and answer from the script.
    async fn send(&self, request: ProviderRequest) -> Result<ProviderResponse, ProviderError> {
        let text = request_text(&request);
        lock(&self.requests).push(text.clone());
        let now = self.in_flight.fetch_add(1, Ordering::SeqCst) + 1;
        self.peak.fetch_max(now, Ordering::SeqCst);
        let mut guard = InFlight {
            provider: self,
            finished: false,
        };
        let mut reply = self.next(&text);
        let result = loop {
            match reply {
                Reply::Text(text) => break Ok(text_response(&text)),
                Reply::Fail(error) => break Err(error),
                Reply::Hang => std::future::pending::<()>().await,
                Reply::After(delay, next) => {
                    tokio::time::sleep(delay).await;
                    reply = *next;
                }
                Reply::ToolCall(name, args) => break Ok(tool_call_response(&name, &args)),
            }
        };
        guard.finished = true;
        result
    }

    /// Streaming is not scripted.
    async fn stream(&self, _request: ProviderRequest) -> Result<ProviderStream, ProviderError> {
        Err(ProviderError::bad_request(
            "scripted",
            "streaming is not scripted",
        ))
    }

    /// Serves OpenAI prompts.
    fn name(&self) -> ProviderName {
        ProviderName::OpenAi
    }
}

/// Concatenated text of an OpenAI Chat request's messages.
pub(crate) fn request_text(request: &ProviderRequest) -> String {
    match request {
        ProviderRequest::OpenAiChatCompletion(request) => request
            .messages
            .iter()
            .filter_map(|message| match message.content.as_ref()? {
                OpenAiMessageContent::Text(text) => Some(text.clone()),
                OpenAiMessageContent::Parts(_) => None,
            })
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}

/// OpenAI Chat assistant answer with `text`.
pub(crate) fn text_response(text: &str) -> ProviderResponse {
    chat_response(
        Some(OpenAiMessageContent::Text(text.to_owned())),
        None,
        "stop",
    )
}

/// OpenAI Chat assistant answer requesting tool `name`.
pub(crate) fn tool_call_response(name: &str, args: &Value) -> ProviderResponse {
    let call = OpenAiToolCall {
        id: format!("call_{name}"),
        kind: "function".to_owned(),
        function: OpenAiToolFunctionCall {
            name: name.to_owned(),
            arguments: args.to_string(),
        },
    };
    chat_response(None, Some(vec![call]), "tool_calls")
}

/// Build one OpenAI Chat response.
fn chat_response(
    content: Option<OpenAiMessageContent>,
    tool_calls: Option<Vec<OpenAiToolCall>>,
    finish_reason: &str,
) -> ProviderResponse {
    ProviderResponse::OpenAiChatCompletion(OpenAiChatResponse {
        id: "resp".to_owned(),
        object: "chat.completion".to_owned(),
        created: 0,
        model: "gpt-test".to_owned(),
        choices: vec![OpenAiChatChoice {
            index: 0,
            message: OpenAiChatMessage {
                role: "assistant".to_owned(),
                content,
                tool_calls,
                ..Default::default()
            },
            finish_reason: Some(finish_reason.to_owned()),
            logprobs: None,
        }],
        usage: None,
        system_fingerprint: None,
        service_tier: None,
    })
}

/// OpenAI Chat Agent named `name` with one user message `template`, and a
/// JSON-schema response when `schema` is set.
pub(crate) fn agent(name: &str, template: &str, schema: Option<Value>) -> Agent {
    let output = schema.map(|schema| {
        ResponseFormat::json_schema(name, schema)
            .expect("fixture schema is a valid response format")
    });
    let prompt = openai_chat(
        "gpt-test",
        OpenAiChatOptions {
            messages: vec![template.to_owned()],
            output,
            ..OpenAiChatOptions::default()
        },
    )
    .expect("fixture prompt builds");
    Agent::new(prompt).name(name)
}

/// Object schema whose listed properties are required strings.
pub(crate) fn string_schema(fields: &[&str]) -> Value {
    let properties: serde_json::Map<String, Value> = fields
        .iter()
        .map(|field| ((*field).to_owned(), json!({ "type": "string" })))
        .collect();
    json!({ "type": "object", "properties": properties, "required": fields })
}

/// Parse a binding fixture.
pub(crate) fn binding(source: &str) -> WorkflowBinding {
    WorkflowBinding::new(source).expect("fixture binding is valid")
}

/// Map of binding fixtures.
pub(crate) fn bindings(pairs: &[(&str, &str)]) -> BTreeMap<String, WorkflowBinding> {
    pairs
        .iter()
        .map(|(name, source)| ((*name).to_owned(), binding(source)))
        .collect()
}

/// Tool that records its invocations and returns a fixed value.
pub(crate) struct RecordingTool {
    /// Tool name.
    pub(crate) name: String,
    /// Invocation count.
    pub(crate) calls: AtomicUsize,
}

#[async_trait]
impl AgentTool for RecordingTool {
    /// Tool name.
    fn name(&self) -> &str {
        &self.name
    }

    /// Fixed description.
    fn description(&self) -> &str {
        "records calls"
    }

    /// Accepts any object.
    fn input_schema(&self) -> Value {
        json!({ "type": "object", "additionalProperties": true })
    }

    /// Any output.
    fn output_schema(&self) -> Value {
        json!({})
    }

    /// Count the call and return a fixed value.
    async fn invoke(&self, _args: Value) -> Result<Value, ToolError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(json!({ "tool": "answered" }))
    }
}

/// Observer recording Workflow step events as compact strings.
#[derive(Default)]
pub(crate) struct RecordingObserver {
    /// Recorded events.
    pub(crate) events: Mutex<Vec<String>>,
    /// Text of every observed model result.
    pub(crate) model_results: Mutex<Vec<String>>,
}

impl RecordingObserver {
    /// Recorded events.
    pub(crate) fn events(&self) -> Vec<String> {
        lock(&self.events).clone()
    }

    /// Text of every observed model result, in order.
    pub(crate) fn model_results(&self) -> Vec<String> {
        lock(&self.model_results).clone()
    }
}

#[async_trait]
impl Observer for RecordingObserver {
    /// Record the result text, the payload a model-result observation carries.
    async fn on_model_result(
        &self,
        _run_id: &str,
        _agent_id: &str,
        _iteration: u32,
        _finish_reason: &str,
        _synthetic: bool,
        response: &ProviderResponse,
    ) {
        let text = response.adapter().text().unwrap_or_default().into_owned();
        lock(&self.model_results).push(text);
    }

    /// Record `attempt:<step>:<n>`.
    async fn on_workflow_step_attempt(&self, _run_id: &str, step_id: &str, attempt: u32) {
        lock(&self.events).push(format!("attempt:{step_id}:{attempt}"));
    }

    /// Record `result:<step>:<n>:<code|ok>`.
    async fn on_workflow_step_result(
        &self,
        _run_id: &str,
        step_id: &str,
        attempt: u32,
        error_code: Option<&str>,
    ) {
        lock(&self.events).push(format!(
            "result:{step_id}:{attempt}:{}",
            error_code.unwrap_or("ok")
        ));
    }

    /// Record `backoff:<step>:<next>:<ms>`.
    async fn on_workflow_step_backoff(
        &self,
        _run_id: &str,
        step_id: &str,
        next_attempt: u32,
        delay: Duration,
    ) {
        lock(&self.events).push(format!(
            "backoff:{step_id}:{next_attempt}:{}",
            delay.as_millis()
        ));
    }
}

/// Observer whose every Workflow callback misbehaves: it panics, or it never
/// completes (optionally sparing the run-start callback).
pub(crate) struct HostileObserver {
    /// Panic instead of hanging.
    pub(crate) panic: bool,
    /// When hanging, let the run-start callback complete.
    pub(crate) spare_start: bool,
}

impl HostileObserver {
    /// Misbehave for callback `hook`.
    async fn misbehave(&self, hook: &str) {
        assert!(!self.panic, "hostile observer panics in {hook}");
        if !(self.spare_start && hook == "start") {
            std::future::pending::<()>().await;
        }
    }
}

#[async_trait]
impl Observer for HostileObserver {
    /// Misbehave at run start.
    async fn on_workflow_start(&self, _run_id: &str, _workflow_id: &str, _step_count: usize) {
        self.misbehave("start").await;
    }

    /// Misbehave at run finish.
    async fn on_workflow_finish(&self, _run_id: &str, _workflow_id: &str, _duration: Duration) {
        self.misbehave("finish").await;
    }

    /// Misbehave at attempt start.
    async fn on_workflow_step_attempt(&self, _run_id: &str, _step_id: &str, _attempt: u32) {
        self.misbehave("attempt").await;
    }

    /// Misbehave at attempt result.
    async fn on_workflow_step_result(
        &self,
        _run_id: &str,
        _step_id: &str,
        _attempt: u32,
        _error_code: Option<&str>,
    ) {
        self.misbehave("result").await;
    }

    /// Misbehave at backoff.
    async fn on_workflow_step_backoff(
        &self,
        _run_id: &str,
        _step_id: &str,
        _next_attempt: u32,
        _delay: Duration,
    ) {
        self.misbehave("backoff").await;
    }
}
