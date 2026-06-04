use std::collections::VecDeque;
use std::sync::{Arc, Mutex, MutexGuard};

use async_trait::async_trait;
use serde_json::{Map, Value, json};
use skald_agent::Agent;
use skald_prompt::{OpenAiChatOptions, ResponseFormat, openai_chat};
use skald_providers::{ProviderError, ProviderStream};
use skald_runtime::{Provider, ProviderRegistry};
use skald_spec::wire::openai_chat::{
    OpenAiChatChoice, OpenAiChatMessage, OpenAiChatResponse, OpenAiMessageContent,
};
use skald_spec::{ProviderName, ProviderRequest, ProviderResponse};
use skald_workflow::{Workflow, WorkflowInput};

#[derive(Clone)]
struct RecordingProvider {
    responses: Arc<Mutex<VecDeque<String>>>,
    requests: Arc<Mutex<Vec<String>>>,
}

impl RecordingProvider {
    fn new(responses: Vec<String>) -> Self {
        Self {
            responses: Arc::new(Mutex::new(responses.into())),
            requests: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn requests(&self) -> Vec<String> {
        self.requests_lock().clone()
    }

    fn last_request_text(&self) -> String {
        self.requests().last().cloned().unwrap_or_default()
    }

    fn requests_lock(&self) -> MutexGuard<'_, Vec<String>> {
        match self.requests.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    fn responses_lock(&self) -> MutexGuard<'_, VecDeque<String>> {
        match self.responses.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }
}

#[async_trait]
impl Provider for RecordingProvider {
    async fn send(&self, request: ProviderRequest) -> Result<ProviderResponse, ProviderError> {
        self.requests_lock().push(request_text(&request));
        let text = self
            .responses_lock()
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

fn registry(provider: RecordingProvider) -> ProviderRegistry {
    let mut providers = ProviderRegistry::new();
    providers.register(Arc::new(provider));
    providers
}

fn request_text(request: &ProviderRequest) -> String {
    match request {
        ProviderRequest::OpenAiChatCompletion(request)
        | ProviderRequest::OpenAiChatCompatible { request, .. } => request
            .messages
            .iter()
            .filter_map(|message| message.content.as_ref())
            .map(openai_content_text)
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}

fn openai_content_text(content: &OpenAiMessageContent) -> String {
    match content {
        OpenAiMessageContent::Text(text) => text.clone(),
        OpenAiMessageContent::Parts(parts) => parts
            .iter()
            .filter_map(|part| match part {
                skald_spec::wire::openai_chat::OpenAiContentPart::Text { text } => {
                    Some(text.as_str())
                }
                _ => None,
            })
            .collect(),
    }
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

fn agent(name: &str, messages: Vec<String>, output: Option<ResponseFormat>) -> Agent {
    Agent::new(
        openai_chat(
            "gpt-test",
            OpenAiChatOptions {
                messages,
                output,
                ..OpenAiChatOptions::default()
            },
        )
        .unwrap(),
    )
    .name(name)
}

#[tokio::test(flavor = "multi_thread")]
async fn workflow_dict_input_binds_first_step_prompt() {
    let planner = agent("planner", vec!["hello ${name}".into()], None);
    let wf = Workflow::sequential("demo", [planner]).unwrap();
    let provider = RecordingProvider::new(vec!["ok".into()]);
    let providers = registry(provider.clone());
    let run = wf
        .run_with(
            &providers,
            WorkflowInput::Vars(Map::from_iter([(
                "name".into(),
                Value::String("Steven".into()),
            )])),
        )
        .await
        .unwrap();

    assert!(run.parameters.is_empty());
    assert!(provider.last_request_text().contains("hello Steven"));
}

#[tokio::test(flavor = "multi_thread")]
async fn workflow_string_input_binds_input_variable() {
    let planner = agent("planner", vec!["got ${input}".into()], None);
    let wf = Workflow::sequential("demo", [planner]).unwrap();
    let provider = RecordingProvider::new(vec!["ok".into()]);
    let providers = registry(provider.clone());

    wf.run_with(&providers, "topic-X").await.unwrap();

    assert!(provider.last_request_text().contains("got topic-X"));
}

#[tokio::test(flavor = "multi_thread")]
async fn workflow_upstream_structured_output_binds_downstream() {
    let schema = json!({
        "type": "object",
        "properties": {"foo": {"type": "string"}, "bar": {"type": "string"}},
        "required": ["foo", "bar"],
        "additionalProperties": false
    });
    let planner = agent(
        "planner",
        vec!["plan ${input}".into()],
        Some(ResponseFormat::json_schema("plan", schema).unwrap()),
    );
    let writer = agent("writer", vec!["pickup ${foo}".into()], None);
    let wf = Workflow::sequential("demo", [planner, writer]).unwrap();
    let provider = RecordingProvider::new(vec![r#"{"foo":"A","bar":"B"}"#.into(), "ok".into()]);
    let providers = registry(provider.clone());

    let run = wf.run_with(&providers, "go").await.unwrap();

    assert_eq!(run.parameters.get("foo"), Some(&Value::String("A".into())));
    assert!(provider.requests().get(1).unwrap().contains("pickup A"));
}

#[tokio::test(flavor = "multi_thread")]
async fn workflow_upstream_overrides_input_on_collision() {
    let schema = json!({
        "type": "object",
        "properties": {"foo": {"type": "string"}},
        "required": ["foo"],
        "additionalProperties": false
    });
    let planner = agent(
        "planner",
        vec!["plan ${input}".into()],
        Some(ResponseFormat::json_schema("plan", schema).unwrap()),
    );
    let writer = agent("writer", vec!["use ${foo}".into()], None);
    let wf = Workflow::sequential("demo", [planner, writer]).unwrap();
    let provider = RecordingProvider::new(vec![r#"{"foo":"upstream-val"}"#.into(), "ok".into()]);
    let providers = registry(provider.clone());

    let run = wf
        .run_with(
            &providers,
            WorkflowInput::Vars(Map::from_iter([
                ("input".into(), Value::String("topic".into())),
                ("foo".into(), Value::String("input-val".into())),
            ])),
        )
        .await
        .unwrap();

    assert_eq!(
        run.parameters.get("foo"),
        Some(&Value::String("upstream-val".into()))
    );
    assert!(
        provider
            .requests()
            .get(1)
            .unwrap()
            .contains("use upstream-val")
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn workflow_missing_parameter_surfaces_code() {
    let writer = agent("writer", vec!["use ${nonexistent}".into()], None);
    let wf = Workflow::sequential("demo", [writer]).unwrap();
    let provider = RecordingProvider::new(vec!["ok".into()]);
    let providers = registry(provider);

    let err = wf.run_with(&providers, "topic").await.unwrap_err();

    assert_eq!(err.code(), "SKALD_WORKFLOW_422_MISSING_PARAMETER");
}

#[tokio::test(flavor = "multi_thread")]
async fn workflow_nested_structured_output_renders_as_json_string() {
    let schema = json!({
        "type": "object",
        "properties": {
            "plan": {
                "type": "object",
                "properties": {"steps": {"type": "array", "items": {"type": "string"}}},
                "required": ["steps"]
            }
        },
        "required": ["plan"],
        "additionalProperties": false
    });
    let planner = agent(
        "planner",
        vec!["go".into()],
        Some(ResponseFormat::json_schema("plan", schema).unwrap()),
    );
    let writer = agent("writer", vec!["consume ${plan}".into()], None);
    let wf = Workflow::sequential("demo", [planner, writer]).unwrap();
    let provider =
        RecordingProvider::new(vec![r#"{"plan":{"steps":["a","b"]}}"#.into(), "ok".into()]);
    let providers = registry(provider.clone());

    wf.run_with(&providers, "").await.unwrap();

    let captured = provider.requests().get(1).unwrap().clone();
    assert!(
        captured.contains(r#"{"steps":["a","b"]}"#),
        "captured payload: {captured}"
    );
}
