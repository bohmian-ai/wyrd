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
use skald_observer::{NoopObserver, Observer, set_global, with_observer};

static OBSERVER_SLOT: OnceLock<Arc<ObserverSlot>> = OnceLock::new();
static TEST_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

#[tokio::test]
async fn agent_run_observer_auto_attached_via_provider() {
    let _guard = TEST_LOCK.lock().await;
    let observer = install_observer(RecordingObserver::new());
    let providers = registry(RecordingProvider::new(vec![openai_text_response("done")]));
    let agent = Agent::from_resolved("test", test_prompt());

    let run = agent
        .run_with(&providers, None, "hello")
        .await
        .expect("run ok");

    assert_eq!(run.finish_reason, FinishReason::ModelStopped);
    assert_eq!(
        observer.kinds(),
        vec![
            EventKind::AgentStart,
            EventKind::Iteration,
            EventKind::ModelCall,
            EventKind::ModelResult,
            EventKind::AgentFinish,
        ]
    );
}

#[tokio::test]
async fn agent_run_prompt_observer_auto_attached_via_provider() {
    let _guard = TEST_LOCK.lock().await;
    let observer = install_observer(RecordingObserver::new());
    let providers = registry(RecordingProvider::new(vec![openai_text_response("done")]));
    let prompt = test_prompt();
    let agent = Agent::from_resolved("test", Arc::clone(&prompt));

    let run = agent
        .run_prompt(&providers, &prompt, &[], None)
        .await
        .expect("run ok");

    assert_eq!(run.finish_reason, FinishReason::ModelStopped);
    assert_eq!(
        observer.kinds(),
        vec![
            EventKind::AgentStart,
            EventKind::Iteration,
            EventKind::ModelCall,
            EventKind::ModelResult,
            EventKind::AgentFinish,
        ]
    );
}

#[tokio::test]
async fn agent_observer_mirrors_journal() {
    let _guard = TEST_LOCK.lock().await;
    let observer = install_observer(RecordingObserver::new());
    let journal = Arc::new(RecordingJournal::new());
    let providers = registry(RecordingProvider::new(vec![
        openai_tool_call_response(vec![tool_call("c1", "tool_a", json!({"q": "x"}))]),
        openai_text_response("done"),
    ]));
    let agent = Agent::from_resolved("test", test_prompt())
        .add_tool(fixed_tool("tool_a", json!({"ok": true})))
        .with_journal(journal.clone())
        .with_run_config(RunConfig {
            max_iterations: 3,
            ..Default::default()
        });

    let run = agent
        .run_with(&providers, None, "hello")
        .await
        .expect("run ok");

    assert_eq!(run.finish_reason, FinishReason::ModelStopped);
    let journal_projection: Vec<_> = journal.events().iter().map(project_journal).collect();
    let observer_projection: Vec<_> = observer.events().iter().map(project_observer).collect();
    assert_eq!(observer_projection, journal_projection);
}

#[tokio::test]
async fn agent_observer_captured_once_per_run_then_cloned_into_spawned_tool_futures() {
    let _guard = TEST_LOCK.lock().await;
    let observer_a = install_observer(RecordingObserver::new());
    let observer_b = Arc::new(RecordingObserver::new());
    let started = Arc::new(Notify::new());
    let release = Arc::new(Notify::new());
    let providers = registry(RecordingProvider::new(vec![
        openai_tool_call_response(vec![tool_call("c1", "waiter", json!({}))]),
        openai_text_response("done"),
    ]));
    let agent = Agent::from_resolved("test", test_prompt())
        .add_tool(Arc::new(ControlledTool {
            started: Arc::clone(&started),
            release: Arc::clone(&release),
            delay: Duration::from_millis(0),
        }))
        .with_run_config(RunConfig {
            max_iterations: 3,
            ..Default::default()
        });

    let obs = Arc::clone(&observer_a) as Arc<dyn Observer>;
    let handle =
        tokio::spawn(
            async move { with_observer(obs, agent.run_with(&providers, None, "hello")).await },
        );
    started.notified().await;
    observer_slot().set(observer_b.clone());
    release.notify_one();

    let run = handle
        .await
        .expect("run task should join")
        .expect("run should complete");

    assert_eq!(run.finish_reason, FinishReason::ModelStopped);
    assert!(observer_a.kinds().contains(&EventKind::ToolCall));
    assert!(observer_a.kinds().contains(&EventKind::ToolResult));
    assert!(observer_a.kinds().contains(&EventKind::AgentFinish));
    assert!(observer_b.events().is_empty());
}

#[tokio::test]
async fn agent_run_timeout_terminates_cleanly() {
    let _guard = TEST_LOCK.lock().await;
    let observer = install_observer(RecordingObserver::new());
    let journal = Arc::new(RecordingJournal::new());
    let providers = registry(RecordingProvider::new(vec![openai_tool_call_response(
        vec![tool_call("c1", "waiter", json!({}))],
    )]));
    let started = Arc::new(Notify::new());
    let release = Arc::new(Notify::new());
    let timeout = Duration::from_millis(50);
    let agent = Agent::from_resolved("test", test_prompt())
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
    let observer_errors: Vec<_> = observer
        .events()
        .into_iter()
        .filter(|event| matches!(event, ObserverEvent::AgentError { .. }))
        .collect();
    assert_eq!(observer_errors.len(), 1);
    assert!(matches!(
        observer_errors.first(),
        Some(ObserverEvent::AgentError { code, .. }) if code == "SKALD_AGENT_504_TIMEOUT"
    ));
}

#[tokio::test]
async fn agent_run_no_timeout_runs_to_completion() {
    let _guard = TEST_LOCK.lock().await;
    let observer = install_observer(RecordingObserver::new());
    let journal = Arc::new(RecordingJournal::new());
    let providers = registry(RecordingProvider::new(vec![
        openai_tool_call_response(vec![tool_call("c1", "slow", json!({}))]),
        openai_text_response("done"),
    ]));
    let agent = Agent::from_resolved("test", test_prompt())
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
    assert!(
        !observer
            .events()
            .iter()
            .any(|event| matches!(event, ObserverEvent::AgentError { .. }))
    );
}

fn install_observer(observer: RecordingObserver) -> Arc<RecordingObserver> {
    let observer = Arc::new(observer);
    observer_slot().set(observer.clone());
    observer
}

fn observer_slot() -> Arc<ObserverSlot> {
    OBSERVER_SLOT
        .get_or_init(|| {
            let slot = Arc::new(ObserverSlot::new());
            set_global(Arc::clone(&slot) as Arc<dyn Observer>);
            slot
        })
        .clone()
}

struct ObserverSlot {
    observer: Mutex<Arc<dyn Observer>>,
}

impl ObserverSlot {
    fn new() -> Self {
        Self {
            observer: Mutex::new(Arc::new(NoopObserver)),
        }
    }

    fn set(&self, observer: Arc<dyn Observer>) {
        *self.lock() = observer;
    }

    fn current(&self) -> Arc<dyn Observer> {
        Arc::clone(&self.lock())
    }

    fn lock(&self) -> MutexGuard<'_, Arc<dyn Observer>> {
        match self.observer.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }
}

#[async_trait]
impl Observer for ObserverSlot {
    async fn on_agent_start(&self, a: &str, b: Option<&str>, c: &str, d: &str, e: Option<&str>) {
        self.current().on_agent_start(a, b, c, d, e).await;
    }

    async fn on_iteration(&self, a: &str, b: &str, c: u32) {
        self.current().on_iteration(a, b, c).await;
    }

    async fn on_model_call(
        &self,
        a: &str,
        b: &str,
        c: u32,
        d: &str,
        e: &str,
        f: &skald_spec::ProviderRequest,
    ) {
        self.current().on_model_call(a, b, c, d, e, f).await;
    }

    async fn on_model_result(
        &self,
        a: &str,
        b: &str,
        c: u32,
        d: &str,
        e: bool,
        f: &skald_spec::ProviderResponse,
    ) {
        self.current().on_model_result(a, b, c, d, e, f).await;
    }

    async fn on_tool_call(&self, a: &str, b: &str, c: u32, d: &str, e: &str) {
        self.current().on_tool_call(a, b, c, d, e).await;
    }

    async fn on_tool_result(&self, a: &str, b: &str, c: u32, d: &str, e: bool) {
        self.current().on_tool_result(a, b, c, d, e).await;
    }

    async fn on_agent_finish(&self, a: &str, b: &str, c: &str, d: u32, e: std::time::Duration) {
        self.current().on_agent_finish(a, b, c, d, e).await;
    }

    async fn on_agent_error(&self, a: &str, b: &str, c: &str, d: &str) {
        self.current().on_agent_error(a, b, c, d).await;
    }

    async fn on_workflow_start(&self, a: &str, b: &str, c: usize) {
        self.current().on_workflow_start(a, b, c).await;
    }

    async fn on_workflow_finish(&self, a: &str, b: &str, c: std::time::Duration) {
        self.current().on_workflow_finish(a, b, c).await;
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum EventKind {
    AgentStart,
    Iteration,
    ModelCall,
    ModelResult,
    ToolCall,
    ToolResult,
    AgentFinish,
    AgentError,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum EventProjection {
    AgentStart {
        agent_id: String,
        input: String,
        session_id: Option<String>,
    },
    Iteration {
        index: u32,
    },
    ModelCall {
        iteration: u32,
        provider: String,
        model: String,
    },
    ModelResult {
        iteration: u32,
        finish_reason: String,
        synthetic: bool,
    },
    ToolCall {
        iteration: u32,
        call_id: String,
        tool_name: String,
    },
    ToolResult {
        iteration: u32,
        call_id: String,
        ok: bool,
    },
    AgentFinish {
        agent_id: String,
        finish_reason: String,
        iterations: u32,
    },
    AgentError {
        agent_id: String,
        code: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ObserverEvent {
    AgentStart {
        agent_id: String,
        input: String,
        session_id: Option<String>,
    },
    Iteration {
        index: u32,
    },
    ModelCall {
        iteration: u32,
        provider: String,
        model: String,
    },
    ModelResult {
        iteration: u32,
        finish_reason: String,
        synthetic: bool,
    },
    ToolCall {
        iteration: u32,
        call_id: String,
        tool_name: String,
    },
    ToolResult {
        iteration: u32,
        call_id: String,
        ok: bool,
    },
    AgentFinish {
        agent_id: String,
        finish_reason: String,
        iterations: u32,
    },
    AgentError {
        agent_id: String,
        code: String,
        message: String,
    },
}

impl ObserverEvent {
    fn kind(&self) -> EventKind {
        match self {
            Self::AgentStart { .. } => EventKind::AgentStart,
            Self::Iteration { .. } => EventKind::Iteration,
            Self::ModelCall { .. } => EventKind::ModelCall,
            Self::ModelResult { .. } => EventKind::ModelResult,
            Self::ToolCall { .. } => EventKind::ToolCall,
            Self::ToolResult { .. } => EventKind::ToolResult,
            Self::AgentFinish { .. } => EventKind::AgentFinish,
            Self::AgentError { .. } => EventKind::AgentError,
        }
    }
}

#[derive(Default)]
struct RecordingObserver {
    events: Mutex<Vec<ObserverEvent>>,
}

impl RecordingObserver {
    fn new() -> Self {
        Self::default()
    }

    fn events(&self) -> Vec<ObserverEvent> {
        self.lock().clone()
    }

    fn kinds(&self) -> Vec<EventKind> {
        self.events().iter().map(ObserverEvent::kind).collect()
    }

    fn push(&self, event: ObserverEvent) {
        self.lock().push(event);
    }

    fn lock(&self) -> MutexGuard<'_, Vec<ObserverEvent>> {
        match self.events.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }
}

#[async_trait]
impl Observer for RecordingObserver {
    async fn on_agent_start(
        &self,
        _run_id: &str,
        _parent_run_id: Option<&str>,
        agent_id: &str,
        input: &str,
        session_id: Option<&str>,
    ) {
        self.push(ObserverEvent::AgentStart {
            agent_id: agent_id.to_owned(),
            input: input.to_owned(),
            session_id: session_id.map(str::to_owned),
        });
    }

    async fn on_iteration(&self, _run_id: &str, _agent_id: &str, index: u32) {
        self.push(ObserverEvent::Iteration { index });
    }

    async fn on_model_call(
        &self,
        _run_id: &str,
        _agent_id: &str,
        iteration: u32,
        provider: &str,
        model: &str,
        _request: &skald_spec::ProviderRequest,
    ) {
        self.push(ObserverEvent::ModelCall {
            iteration,
            provider: provider.to_owned(),
            model: model.to_owned(),
        });
    }

    async fn on_model_result(
        &self,
        _run_id: &str,
        _agent_id: &str,
        iteration: u32,
        finish_reason: &str,
        synthetic: bool,
        _response: &skald_spec::ProviderResponse,
    ) {
        self.push(ObserverEvent::ModelResult {
            iteration,
            finish_reason: finish_reason.to_owned(),
            synthetic,
        });
    }

    async fn on_tool_call(
        &self,
        _run_id: &str,
        _agent_id: &str,
        iteration: u32,
        call_id: &str,
        tool_name: &str,
    ) {
        self.push(ObserverEvent::ToolCall {
            iteration,
            call_id: call_id.to_owned(),
            tool_name: tool_name.to_owned(),
        });
    }

    async fn on_tool_result(
        &self,
        _run_id: &str,
        _agent_id: &str,
        iteration: u32,
        call_id: &str,
        ok: bool,
    ) {
        self.push(ObserverEvent::ToolResult {
            iteration,
            call_id: call_id.to_owned(),
            ok,
        });
    }

    async fn on_agent_finish(
        &self,
        _run_id: &str,
        agent_id: &str,
        finish_reason: &str,
        iterations: u32,
        _duration: Duration,
    ) {
        self.push(ObserverEvent::AgentFinish {
            agent_id: agent_id.to_owned(),
            finish_reason: finish_reason.to_owned(),
            iterations,
        });
    }

    async fn on_agent_error(&self, _run_id: &str, agent_id: &str, code: &str, message: &str) {
        self.push(ObserverEvent::AgentError {
            agent_id: agent_id.to_owned(),
            code: code.to_owned(),
            message: message.to_owned(),
        });
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

fn project_journal(event: &JournalEvent) -> EventProjection {
    match event {
        JournalEvent::AgentStart {
            agent_id,
            input,
            session_id,
        } => EventProjection::AgentStart {
            agent_id: agent_id.clone(),
            input: input.clone(),
            session_id: session_id.clone(),
        },
        JournalEvent::Iteration { index } => EventProjection::Iteration { index: *index },
        JournalEvent::ModelCall {
            iteration,
            provider,
            model,
        } => EventProjection::ModelCall {
            iteration: *iteration,
            provider: provider.clone(),
            model: model.clone(),
        },
        JournalEvent::ModelResult {
            iteration,
            finish_reason,
            synthetic,
        } => EventProjection::ModelResult {
            iteration: *iteration,
            finish_reason: finish_reason.clone(),
            synthetic: *synthetic,
        },
        JournalEvent::ToolCall {
            iteration,
            call_id,
            tool_name,
            ..
        } => EventProjection::ToolCall {
            iteration: *iteration,
            call_id: call_id.clone(),
            tool_name: tool_name.clone(),
        },
        JournalEvent::ToolResult {
            iteration,
            call_id,
            ok,
            ..
        } => EventProjection::ToolResult {
            iteration: *iteration,
            call_id: call_id.clone(),
            ok: *ok,
        },
        JournalEvent::AgentFinish {
            agent_id,
            finish_reason,
            iterations,
        } => EventProjection::AgentFinish {
            agent_id: agent_id.clone(),
            finish_reason: finish_reason.clone(),
            iterations: *iterations,
        },
        JournalEvent::AgentError { agent_id, code, .. } => EventProjection::AgentError {
            agent_id: agent_id.clone(),
            code: code.clone(),
        },
    }
}

fn project_observer(event: &ObserverEvent) -> EventProjection {
    match event {
        ObserverEvent::AgentStart {
            agent_id,
            input,
            session_id,
        } => EventProjection::AgentStart {
            agent_id: agent_id.clone(),
            input: input.clone(),
            session_id: session_id.clone(),
        },
        ObserverEvent::Iteration { index } => EventProjection::Iteration { index: *index },
        ObserverEvent::ModelCall {
            iteration,
            provider,
            model,
        } => EventProjection::ModelCall {
            iteration: *iteration,
            provider: provider.clone(),
            model: model.clone(),
        },
        ObserverEvent::ModelResult {
            iteration,
            finish_reason,
            synthetic,
        } => EventProjection::ModelResult {
            iteration: *iteration,
            finish_reason: finish_reason.clone(),
            synthetic: *synthetic,
        },
        ObserverEvent::ToolCall {
            iteration,
            call_id,
            tool_name,
        } => EventProjection::ToolCall {
            iteration: *iteration,
            call_id: call_id.clone(),
            tool_name: tool_name.clone(),
        },
        ObserverEvent::ToolResult {
            iteration,
            call_id,
            ok,
        } => EventProjection::ToolResult {
            iteration: *iteration,
            call_id: call_id.clone(),
            ok: *ok,
        },
        ObserverEvent::AgentFinish {
            agent_id,
            finish_reason,
            iterations,
        } => EventProjection::AgentFinish {
            agent_id: agent_id.clone(),
            finish_reason: finish_reason.clone(),
            iterations: *iterations,
        },
        ObserverEvent::AgentError { agent_id, code, .. } => EventProjection::AgentError {
            agent_id: agent_id.clone(),
            code: code.clone(),
        },
    }
}

fn registry(provider: RecordingProvider) -> ProviderRegistry {
    let mut providers = ProviderRegistry::new();
    providers.register(Arc::new(provider));
    providers
}

fn fixed_tool(name: &str, output: Value) -> Arc<dyn AgentTool> {
    Arc::new(FixedTool {
        name: name.to_owned(),
        output,
    })
}

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
