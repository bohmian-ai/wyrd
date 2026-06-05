use std::collections::VecDeque;
use std::sync::{Arc, Mutex, MutexGuard};

use async_trait::async_trait;
use skald_agent::{Agent, Observer};
use skald_prompt::{OpenAiChatOptions, openai_chat};
use skald_providers::{ProviderError, ProviderStream};
use skald_runtime::{Provider, ProviderRegistry};
use skald_spec::wire::openai_chat::{
    OpenAiChatChoice, OpenAiChatMessage, OpenAiChatResponse, OpenAiMessageContent,
};
use skald_spec::{ProviderName, ProviderRequest, ProviderResponse};
use skald_workflow::Workflow;

#[derive(Clone)]
struct RecordingProvider {
    responses: Arc<Mutex<VecDeque<String>>>,
}

impl RecordingProvider {
    fn new(responses: Vec<&str>) -> Self {
        Self {
            responses: Arc::new(Mutex::new(
                responses.into_iter().map(str::to_owned).collect(),
            )),
        }
    }

    fn responses(&self) -> MutexGuard<'_, VecDeque<String>> {
        match self.responses.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }
}

#[async_trait]
impl Provider for RecordingProvider {
    async fn send(&self, _request: ProviderRequest) -> Result<ProviderResponse, ProviderError> {
        let text = self
            .responses()
            .pop_front()
            .ok_or_else(|| ProviderError::bad_request("recording", "response queue is empty"))?;
        Ok(openai_text_response(&text))
    }

    async fn stream(&self, _request: ProviderRequest) -> Result<ProviderStream, ProviderError> {
        Err(ProviderError::bad_request(
            "recording",
            "streaming is not supported",
        ))
    }

    fn name(&self) -> ProviderName {
        ProviderName::OpenAi
    }
}

#[derive(Default)]
struct EventLog {
    events: Mutex<Vec<String>>,
}

impl EventLog {
    fn push(&self, event: impl Into<String>) {
        self.events.lock().expect("event log lock").push(event.into());
    }

    fn events(&self) -> Vec<String> {
        self.events.lock().expect("event log lock").clone()
    }
}

#[async_trait]
impl Observer for EventLog {
    async fn on_agent_start(
        &self,
        _run_id: &str,
        parent_run_id: Option<&str>,
        agent_id: &str,
        _input: &str,
        _session_id: Option<&str>,
    ) {
        self.push(format!("agent_start:{agent_id}:parent={}", parent_run_id.is_some()));
    }

    async fn on_model_call(
        &self,
        _run_id: &str,
        agent_id: &str,
        _iteration: u32,
        _provider: &str,
        _model: &str,
        _request: &ProviderRequest,
    ) {
        self.push(format!("model_call:{agent_id}"));
    }

    async fn on_model_result(
        &self,
        _run_id: &str,
        agent_id: &str,
        _iteration: u32,
        _finish_reason: &str,
        _synthetic: bool,
        _response: &ProviderResponse,
    ) {
        self.push(format!("model_result:{agent_id}"));
    }

    async fn on_agent_finish(
        &self,
        _run_id: &str,
        agent_id: &str,
        _finish_reason: &str,
        _iterations: u32,
        _duration: std::time::Duration,
    ) {
        self.push(format!("agent_finish:{agent_id}"));
    }

    async fn on_workflow_start(&self, _run_id: &str, workflow_id: &str, _step_count: usize) {
        self.push(format!("workflow_start:{workflow_id}"));
    }

    async fn on_workflow_finish(
        &self,
        _run_id: &str,
        workflow_id: &str,
        _duration: std::time::Duration,
    ) {
        self.push(format!("workflow_finish:{workflow_id}"));
    }
}

struct OrderedObserver {
    name: &'static str,
    sink: Arc<Mutex<Vec<String>>>,
}

impl OrderedObserver {
    fn new(name: &'static str, sink: Arc<Mutex<Vec<String>>>) -> Self {
        Self { name, sink }
    }
}

#[async_trait]
impl Observer for OrderedObserver {
    async fn on_workflow_start(&self, _run_id: &str, _workflow_id: &str, _step_count: usize) {
        self.sink
            .lock()
            .expect("ordered observer lock")
            .push(self.name.to_owned());
    }
}

fn registry(responses: Vec<&str>) -> ProviderRegistry {
    let mut providers = ProviderRegistry::new();
    providers.register(Arc::new(RecordingProvider::new(responses)));
    providers
}

fn agent(name: &str) -> Agent {
    Agent::new(
        openai_chat(
            "gpt-test",
            OpenAiChatOptions {
                messages: vec![format!("run {name}")],
                ..OpenAiChatOptions::default()
            },
    )
    .expect("static prompt is valid"),
    )
    .name(name)
    .with_id(name)
}

fn workflow() -> Workflow {
    workflow_named("research")
}

fn workflow_named(name: &str) -> Workflow {
    Workflow::sequential(name, [agent("planner"), agent("writer")])
        .expect("workflow is valid")
}

fn openai_text_response(text: &str) -> ProviderResponse {
    ProviderResponse::OpenAiChatCompletion(OpenAiChatResponse {
        id: "resp_1".to_owned(),
        object: "chat.completion".to_owned(),
        created: 0,
        model: "gpt-test".to_owned(),
        choices: vec![OpenAiChatChoice {
            index: 0,
            message: OpenAiChatMessage {
                role: "assistant".to_owned(),
                content: Some(OpenAiMessageContent::Text(text.to_owned())),
                ..Default::default()
            },
            finish_reason: Some("stop".to_owned()),
            logprobs: None,
        }],
        usage: None,
        system_fingerprint: None,
        service_tier: None,
    })
}

#[tokio::test(flavor = "multi_thread")]
async fn workflow_with_no_observers_runs_cleanly() {
    let providers = registry(vec!["plan", "write"]);
    let run = workflow().run_with(&providers, "topic").await.unwrap();

    assert_eq!(run.tasks.len(), 2);
}

#[tokio::test(flavor = "multi_thread")]
async fn workflow_with_one_observer_scopes_correctly() {
    let log = Arc::new(EventLog::default());
    let wf = workflow().with_observers(vec![Arc::clone(&log) as Arc<dyn Observer>]);
    let providers = registry(vec!["plan", "write"]);

    wf.run_with(&providers, "topic").await.unwrap();

    let events = log.events();
    assert!(events.contains(&"workflow_start:research".to_owned()));
    assert!(events.contains(&"agent_start:planner:parent=true".to_owned()));
    assert!(events.contains(&"model_call:planner".to_owned()));
    assert!(events.contains(&"model_result:writer".to_owned()));
    assert!(events.contains(&"agent_finish:writer".to_owned()));
    assert!(events.contains(&"workflow_finish:research".to_owned()));
}

#[tokio::test(flavor = "multi_thread")]
async fn workflow_with_multiple_observers_dispatches_in_order() {
    let sink = Arc::new(Mutex::new(Vec::new()));
    let wf = workflow().with_observers(vec![
        Arc::new(OrderedObserver::new("a", Arc::clone(&sink))) as Arc<dyn Observer>,
        Arc::new(OrderedObserver::new("b", Arc::clone(&sink))) as Arc<dyn Observer>,
        Arc::new(OrderedObserver::new("c", Arc::clone(&sink))) as Arc<dyn Observer>,
    ]);
    let providers = registry(vec!["plan", "write"]);

    wf.run_with(&providers, "topic").await.unwrap();

    assert_eq!(*sink.lock().expect("ordered observer lock"), ["a", "b", "c"]);
}

#[tokio::test(flavor = "multi_thread")]
async fn workflow_observers_override_global() {
    let global = Arc::new(EventLog::default());
    wyrd_observe::set_global(Arc::clone(&global) as Arc<dyn Observer>);

    let scoped = Arc::new(EventLog::default());
    let wf = workflow_named("scoped-research")
        .with_observers(vec![Arc::clone(&scoped) as Arc<dyn Observer>]);
    let providers = registry(vec!["plan", "write", "solo"]);

    wf.run_with(&providers, "topic").await.unwrap();
    agent("solo").run_with(&providers, None, "topic").await.unwrap();

    assert!(
        scoped
            .events()
            .iter()
            .any(|event| event == "workflow_start:scoped-research")
    );
    assert!(
        !global
            .events()
            .iter()
            .any(|event| event == "workflow_start:scoped-research")
    );
    assert!(global.events().iter().any(|event| event == "agent_start:solo:parent=false"));
}

#[test]
fn workflow_observers_not_serialized() {
    let log = Arc::new(EventLog::default());
    let wf = workflow()
        .with_version("0.1.0")
        .with_observers(vec![log as Arc<dyn Observer>]);

    let yaml = wf.to_yaml_string().unwrap();
    let loaded = Workflow::from_yaml_str(
        &yaml,
        skald_tool::default_registry(),
        skald_agent::default_prompt_resolver(),
    )
    .unwrap();

    assert!(loaded.observers().is_empty());
}
