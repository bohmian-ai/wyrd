use std::collections::VecDeque;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use async_trait::async_trait;
use serde_json::{Value, json};
use skald_agent::{Agent, AgentError, ConversationTurn, FinishReason, RunConfig};
use skald_prompt::Prompt;
use skald_providers::{ProviderError, ProviderStream};
use skald_runtime::{Provider, ProviderRegistry};
use skald_spec::wire::openai_chat::{
    OpenAiChatChoice, OpenAiChatMessage, OpenAiChatRequest, OpenAiChatResponse, OpenAiChatSettings,
    OpenAiMessageContent, OpenAiTool, OpenAiToolCall, OpenAiToolFunctionCall,
};
use skald_spec::{
    Prompt as SpecPrompt, ProviderName, ProviderRequest, ProviderResponse, ResponseType,
};
use skald_tool::{AgentTool, ToolError};

#[tokio::test]
async fn agent_run_executes_bounded_loop_against_recording_provider() {
    let provider = RecordingProvider::new(vec![openai_text_response("done.")]);
    let providers = registry(provider.clone());
    let agent = Agent::new("test", test_prompt()).with_run_config(RunConfig {
        max_iterations: 3,
        ..Default::default()
    });

    let run = agent.run(&providers, "hello").await.expect("run ok");

    assert_eq!(run.finish_reason, FinishReason::ModelStopped);
    assert_eq!(run.iterations, 1);
    assert_eq!(run.output, "done.");
    assert_eq!(provider.requests().len(), 1);
}

#[tokio::test]
async fn agent_render_populates_provider_request_tools_from_agent_tools() {
    let provider = RecordingProvider::new(vec![openai_text_response("ok")]);
    let providers = registry(provider.clone());
    let agent = Agent::new("test", test_prompt())
        .add_tool(fixed_tool("tool_a", json!({"a": true})))
        .add_tool(fixed_tool("tool_b", json!({"b": true})));

    let _ = agent.run(&providers, "hello").await.expect("run ok");

    match provider.requests().first().expect("captured request") {
        ProviderRequest::OpenAiChatCompletion(request) => {
            let tools = request.tools.as_ref().expect("tools populated");
            assert_eq!(tools.len(), 2);
            assert!(tools.iter().any(|tool| openai_tool_name(tool) == "tool_a"));
            assert!(tools.iter().any(|tool| openai_tool_name(tool) == "tool_b"));
        }
        other => panic!("expected OpenAiChatCompletion, got {other:?}"),
    }
}

#[tokio::test]
async fn agent_render_omits_tools_when_agent_has_none() {
    let provider = RecordingProvider::new(vec![openai_text_response("ok")]);
    let providers = registry(provider.clone());
    let agent = Agent::new("test", test_prompt());

    let _ = agent.run(&providers, "hello").await.expect("run ok");

    match provider.requests().first().expect("captured request") {
        ProviderRequest::OpenAiChatCompletion(request) => {
            assert!(request.tools.is_none(), "tools field must be omitted");
        }
        other => panic!("expected OpenAiChatCompletion, got {other:?}"),
    }
}

#[tokio::test]
async fn agent_loop_threads_tool_result_to_next_iteration() {
    let provider = RecordingProvider::new(vec![
        openai_tool_call_response(vec![tool_call("c1", "tool_a", json!({"q": "x"}))]),
        openai_text_response("final"),
    ]);
    let providers = registry(provider.clone());
    let agent =
        Agent::new("test", test_prompt()).add_tool(fixed_tool("tool_a", json!({"score": 0.9})));

    let run = agent.run(&providers, "hello").await.expect("run ok");

    assert_eq!(run.finish_reason, FinishReason::ModelStopped);
    assert_eq!(run.iterations, 2);
    let requests = provider.requests();
    assert_eq!(requests.len(), 2);
    match &requests[1] {
        ProviderRequest::OpenAiChatCompletion(request) => {
            let has_tool_message = request.messages.iter().any(|message| {
                message.role == "tool" && message.tool_call_id.as_deref() == Some("c1")
            });
            assert!(has_tool_message, "iteration 2 must carry prior tool result");
        }
        other => panic!("expected OpenAiChatCompletion, got {other:?}"),
    }
}

#[tokio::test]
async fn agent_tool_results_appended_in_call_order_under_concurrency() {
    let calls = (0..5)
        .map(|i| tool_call(format!("c{i}"), "sleeper", json!({ "i": i })))
        .collect();
    let provider = RecordingProvider::new(vec![
        openai_tool_call_response(calls),
        openai_text_response("done"),
    ]);
    let providers = registry(provider);
    let agent = Agent::new("test", test_prompt())
        .add_tool(Arc::new(SleeperTool { max: 5 }))
        .with_run_config(RunConfig {
            max_iterations: 3,
            tool_concurrency_cap: Some(5),
            ..Default::default()
        });

    let run = agent.run(&providers, "go").await.expect("run ok");

    let call_ids: Vec<String> = run
        .conversation
        .turns()
        .iter()
        .filter_map(|turn| match turn {
            ConversationTurn::ToolResult { call_id, .. } => Some(call_id.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(call_ids, vec!["c0", "c1", "c2", "c3", "c4"]);
}

#[tokio::test]
async fn agent_terminates_when_model_emits_no_tool_calls() {
    let provider = RecordingProvider::new(vec![openai_text_response("stop here")]);
    let providers = registry(provider);
    let agent = Agent::new("test", test_prompt());

    let run = agent.run(&providers, "hi").await.expect("ok");

    assert_eq!(run.finish_reason, FinishReason::ModelStopped);
    assert_eq!(run.iterations, 1);
    assert_eq!(run.output, "stop here");
}

#[tokio::test]
async fn agent_max_iterations_exhausted_returns_error() {
    let provider = RecordingProvider::new(vec![
        openai_tool_call_response(vec![tool_call("c1", "tool_a", json!({}))]),
        openai_tool_call_response(vec![tool_call("c2", "tool_a", json!({}))]),
        openai_tool_call_response(vec![tool_call("c3", "tool_a", json!({}))]),
    ]);
    let providers = registry(provider);
    let agent = Agent::new("test", test_prompt())
        .add_tool(fixed_tool("tool_a", json!({"ok": true})))
        .with_run_config(RunConfig {
            max_iterations: 3,
            ..Default::default()
        });

    let err = agent
        .run(&providers, "loop")
        .await
        .expect_err("should hit cap");

    assert_eq!(err.code(), "SKALD_AGENT_500_MAX_ITERATIONS");
    match err {
        AgentError::MaxIterations { agent, cap } => {
            assert_eq!(agent, "test");
            assert_eq!(cap, 3);
        }
        other => panic!("expected MaxIterations, got {other:?}"),
    }
}

#[tokio::test]
async fn agent_conversation_grows_across_iterations_without_session() {
    let provider = RecordingProvider::new(vec![
        openai_tool_call_response(vec![tool_call("c1", "tool_a", json!({}))]),
        openai_tool_call_response(vec![tool_call("c2", "tool_a", json!({}))]),
        openai_text_response("final"),
    ]);
    let providers = registry(provider);
    let agent = Agent::new("test", test_prompt())
        .add_tool(fixed_tool("tool_a", json!({"ok": true})))
        .with_run_config(RunConfig {
            max_iterations: 5,
            ..Default::default()
        });

    let run = agent.run(&providers, "begin").await.expect("ok");

    assert_eq!(run.iterations, 3);
    let kinds: Vec<&str> = run
        .conversation
        .turns()
        .iter()
        .map(|turn| match turn {
            ConversationTurn::System { .. } => "system",
            ConversationTurn::User { .. } => "user",
            ConversationTurn::Assistant { .. } => "assistant",
            ConversationTurn::ToolResult { .. } => "tool_result",
        })
        .collect();
    assert_eq!(
        kinds,
        vec![
            "user",
            "assistant",
            "tool_result",
            "assistant",
            "tool_result",
            "assistant"
        ]
    );
}

#[tokio::test]
async fn agent_conversation_serializes_via_serde_round_trip() {
    let provider = RecordingProvider::new(vec![openai_text_response("hi back")]);
    let providers = registry(provider);
    let agent = Agent::new("test", test_prompt());

    let run = agent.run(&providers, "hi").await.expect("ok");
    let json = serde_json::to_string(&run).expect("serialize");
    let back: skald_agent::AgentRun = serde_json::from_str(&json).expect("deserialize");

    assert_eq!(back.finish_reason, run.finish_reason);
    assert_eq!(back.iterations, run.iterations);
    assert_eq!(
        back.conversation.turns().len(),
        run.conversation.turns().len()
    );
    assert_eq!(back.output, run.output);
}

#[derive(Clone)]
struct RecordingProvider {
    responses: Arc<Mutex<VecDeque<ProviderResponse>>>,
    requests: Arc<Mutex<Vec<ProviderRequest>>>,
}

impl RecordingProvider {
    fn new(responses: Vec<ProviderResponse>) -> Self {
        Self {
            responses: Arc::new(Mutex::new(responses.into())),
            requests: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn requests(&self) -> Vec<ProviderRequest> {
        self.requests_lock().clone()
    }

    fn requests_lock(&self) -> MutexGuard<'_, Vec<ProviderRequest>> {
        match self.requests.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    fn responses_lock(&self) -> MutexGuard<'_, VecDeque<ProviderResponse>> {
        match self.responses.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }
}

#[async_trait]
impl Provider for RecordingProvider {
    async fn send(&self, request: ProviderRequest) -> Result<ProviderResponse, ProviderError> {
        self.requests_lock().push(request);
        self.responses_lock()
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
        "test tool"
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {},
            "additionalProperties": true
        })
    }

    fn output_schema(&self) -> Value {
        json!({})
    }

    async fn invoke(&self, _args: Value) -> Result<Value, ToolError> {
        Ok(self.output.clone())
    }
}

#[derive(Debug)]
struct SleeperTool {
    max: u64,
}

#[async_trait]
impl AgentTool for SleeperTool {
    fn name(&self) -> &str {
        "sleeper"
    }

    fn description(&self) -> &str {
        "sleeps in inverse index order"
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": { "i": { "type": "integer" } },
            "required": ["i"],
            "additionalProperties": false
        })
    }

    fn output_schema(&self) -> Value {
        json!({})
    }

    async fn invoke(&self, args: Value) -> Result<Value, ToolError> {
        let index = args.get("i").and_then(Value::as_u64).unwrap_or(0);
        let delay = self.max.saturating_sub(index);
        tokio::time::sleep(Duration::from_millis(delay * 10)).await;
        Ok(json!({ "i": index }))
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

fn openai_tool_name(tool: &OpenAiTool) -> &str {
    match tool {
        OpenAiTool::Function { function } => &function.name,
        OpenAiTool::Custom { custom } => &custom.name,
    }
}
