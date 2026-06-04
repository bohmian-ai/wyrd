use std::sync::{Arc, Mutex, MutexGuard, OnceLock};
use std::time::Duration;

use async_trait::async_trait;
use skald_runtime::{MockProvider, ProviderRegistry};
use skald_spec::wire::openai_chat::{
    OpenAiChatChoice, OpenAiChatMessage, OpenAiChatRequest, OpenAiChatResponse, OpenAiChatSettings,
    OpenAiMessageContent,
};
use skald_spec::{ProviderName, ProviderRequest, ProviderResponse, ResponseType};
use wyrd::agent::*;

static TEST_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
static GLOBAL_RECORDER: OnceLock<Arc<RecordingObserver>> = OnceLock::new();

#[tokio::test]
async fn agent_run_via_umbrella_imports() {
    let _guard = TEST_LOCK.lock().await;
    let mock = MockProvider::new(ProviderName::OpenAi);
    mock.push_response(openai_text_response("done"));
    let mut providers = ProviderRegistry::new();
    providers.register(Arc::new(mock));
    let agent = Agent::new(Prompt::from_native(prompt()))
        .with_id("planner-agent")
        .name("planner-agent")
        .version("0.3.0");

    let run = agent
        .run_with(&providers, None, "make a plan")
        .await
        .expect("agent run succeeds");

    assert_eq!(run.finish_reason, FinishReason::ModelStopped);
    assert_eq!(run.output, "done");
    assert_eq!(run.conversation.len(), 2);
    assert!(matches!(
        run.conversation.turns().first(),
        Some(ConversationTurn::User { content }) if content == "make a plan"
    ));
    assert!(matches!(
        run.conversation.turns().last(),
        Some(ConversationTurn::Assistant { .. })
    ));
}

#[tokio::test]
async fn wyrd_init_attaches_observer_provider() {
    let _guard = TEST_LOCK.lock().await;
    let global = global_recorder();
    let mock = MockProvider::new(ProviderName::OpenAi);
    mock.push_response(openai_text_response("done"));
    let mut providers = ProviderRegistry::new();
    providers.register(Arc::new(mock));
    let agent = Agent::new(Prompt::from_native(prompt()))
        .with_id("planner-agent")
        .name("planner-agent")
        .version("0.3.0");

    wyrd::init();
    let run = agent
        .run_with(&providers, None, "make a plan")
        .await
        .expect("agent run succeeds");

    assert_eq!(run.finish_reason, FinishReason::ModelStopped);
    assert_eq!(
        global.events(),
        vec![
            "start:planner-agent".to_owned(),
            "iteration:planner-agent:0".to_owned(),
            "finish:planner-agent:modelstopped:1".to_owned(),
        ]
    );
}

#[tokio::test]
async fn agent_run_scoped_observer_overrides_global() {
    let _guard = TEST_LOCK.lock().await;
    let global = global_recorder();
    let scoped = Arc::new(RecordingObserver::default());
    let mock = MockProvider::new(ProviderName::OpenAi);
    mock.push_response(openai_text_response("done"));
    let mut providers = ProviderRegistry::new();
    providers.register(Arc::new(mock));
    let agent = Agent::new(Prompt::from_native(prompt()))
        .with_id("planner-agent")
        .name("planner-agent")
        .version("0.3.0");

    wyrd::init();
    let scoped_observer: Arc<dyn Observer> = scoped.clone();
    let run = wyrd_observe::with_observer(scoped_observer, async {
        agent
            .run_with(&providers, None, "make a plan")
            .await
            .expect("agent run succeeds")
    })
    .await;

    assert_eq!(run.finish_reason, FinishReason::ModelStopped);
    assert_eq!(global.events(), Vec::<String>::new());
    assert_eq!(
        scoped.events(),
        vec![
            "start:planner-agent".to_owned(),
            "iteration:planner-agent:0".to_owned(),
            "finish:planner-agent:modelstopped:1".to_owned(),
        ]
    );
}

fn global_recorder() -> Arc<RecordingObserver> {
    let recorder = GLOBAL_RECORDER
        .get_or_init(|| {
            let recorder = Arc::new(RecordingObserver::default());
            let observer: Arc<dyn Observer> = recorder.clone();
            wyrd_observe::set_global(observer);
            recorder
        })
        .clone();
    recorder.clear();
    recorder
}

fn prompt() -> skald_spec::Prompt {
    skald_spec::Prompt::new(openai_request(), "gpt-4o-mini", None, ResponseType::Text)
        .expect("static prompt is valid")
}

fn openai_request() -> ProviderRequest {
    ProviderRequest::OpenAiChatCompletion(OpenAiChatRequest {
        model: "gpt-4o-mini".to_owned(),
        messages: vec![OpenAiChatMessage {
            role: "user".to_owned(),
            content: Some(OpenAiMessageContent::Text("plan".to_owned())),
            name: None,
            tool_calls: None,
            tool_call_id: None,
            refusal: None,
        }],
        response_format: None,
        stream: None,
        stream_options: None,
        tools: None,
        tool_choice: None,
        parallel_tool_calls: None,
        settings: OpenAiChatSettings::default(),
    })
}

fn openai_text_response(text: &str) -> ProviderResponse {
    ProviderResponse::OpenAiChatCompletion(OpenAiChatResponse {
        id: "resp_1".to_owned(),
        object: "chat.completion".to_owned(),
        created: 0,
        model: "gpt-4o-mini".to_owned(),
        choices: vec![OpenAiChatChoice {
            index: 0,
            message: OpenAiChatMessage {
                role: "assistant".to_owned(),
                content: Some(OpenAiMessageContent::Text(text.to_owned())),
                name: None,
                tool_calls: None,
                tool_call_id: None,
                refusal: None,
            },
            finish_reason: Some("stop".to_owned()),
            logprobs: None,
        }],
        usage: None,
        system_fingerprint: None,
        service_tier: None,
    })
}

#[derive(Default)]
struct RecordingObserver {
    events: Mutex<Vec<String>>,
}

impl RecordingObserver {
    fn events(&self) -> Vec<String> {
        self.lock().clone()
    }

    fn clear(&self) {
        self.lock().clear();
    }

    fn lock(&self) -> MutexGuard<'_, Vec<String>> {
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
        _input: &str,
        _session_id: Option<&str>,
    ) {
        self.lock().push(format!("start:{agent_id}"));
    }

    async fn on_iteration(&self, _run_id: &str, agent_id: &str, index: u32) {
        self.lock().push(format!("iteration:{agent_id}:{index}"));
    }

    async fn on_agent_finish(
        &self,
        _run_id: &str,
        agent_id: &str,
        finish_reason: &str,
        iterations: u32,
        _duration: Duration,
    ) {
        self.lock()
            .push(format!("finish:{agent_id}:{finish_reason}:{iterations}"));
    }
}
