use std::collections::VecDeque;
use std::sync::{Arc, Mutex, MutexGuard};

use async_trait::async_trait;
use skald_agent::{Agent, FinishReason, Observer};
use skald_prompt::Prompt;
use skald_providers::{ProviderError, ProviderStream};
use skald_runtime::{Provider, ProviderRegistry};
use skald_spec::wire::openai_chat::{
    OpenAiChatChoice, OpenAiChatMessage, OpenAiChatRequest, OpenAiChatResponse, OpenAiChatSettings,
    OpenAiMessageContent,
};
use skald_spec::{
    Prompt as SpecPrompt, ProviderName, ProviderRequest, ProviderResponse, ResponseType,
};

#[tokio::test]
async fn agent_run_no_observer_provider_uses_noop() {
    let observer = RecordingObserver::default();
    let providers = registry(RecordingProvider::new(vec![openai_text_response("done")]));
    let agent = Agent::from_resolved("test", test_prompt());

    let run = agent
        .run_with(&providers, None, "hello")
        .await
        .expect("run ok");

    assert_eq!(run.finish_reason, FinishReason::ModelStopped);
    assert_eq!(observer.count(), 0);
}

#[derive(Default)]
struct RecordingObserver {
    count: Mutex<usize>,
}

impl RecordingObserver {
    fn count(&self) -> usize {
        match self.count.lock() {
            Ok(guard) => *guard,
            Err(poisoned) => *poisoned.into_inner(),
        }
    }
}

#[async_trait]
impl Observer for RecordingObserver {
    async fn on_agent_start(
        &self,
        _run_id: &str,
        _parent_run_id: Option<&str>,
        _agent_id: &str,
        _input: &str,
        _session_id: Option<&str>,
    ) {
        match self.count.lock() {
            Ok(mut guard) => *guard += 1,
            Err(poisoned) => *poisoned.into_inner() += 1,
        }
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

fn registry(provider: RecordingProvider) -> ProviderRegistry {
    let mut providers = ProviderRegistry::new();
    providers.register(Arc::new(provider));
    providers
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
            },
            finish_reason: Some("stop".to_owned()),
            logprobs: None,
        }],
        usage: None,
        system_fingerprint: None,
        service_tier: None,
    })
}
