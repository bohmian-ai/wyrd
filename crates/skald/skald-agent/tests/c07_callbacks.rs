use std::collections::VecDeque;
use std::sync::{Arc, Mutex, MutexGuard};

use async_trait::async_trait;
use serde_json::{Value, json};
use skald_agent::{
    Agent, AgentError, AgentRun, CallbackOutcome, ConversationTurn, FinishReason, RunConfig,
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

#[tokio::test]
async fn agent_run_callbacks_fire_in_order() {
    let provider = RecordingProvider::new(vec![openai_text_response("done")]);
    let providers = registry(provider);
    let order = Arc::new(Mutex::new(Vec::new()));
    let before_agent_order = Arc::clone(&order);
    let before_model_order = Arc::clone(&order);
    let after_model_order = Arc::clone(&order);
    let after_agent_order = Arc::clone(&order);
    let agent = Agent::new("test", test_prompt())
        .before_agent(Arc::new(move |ctx, input| {
            assert_eq!(ctx.agent_id, "test");
            assert_eq!(ctx.iteration, 0);
            assert_eq!(input, "hello");
            before_agent_order
                .lock()
                .expect("order lock")
                .push("before_agent");
            CallbackOutcome::Continue
        }))
        .before_model(Arc::new(move |ctx, _request| {
            assert_eq!(ctx.iteration, 0);
            before_model_order
                .lock()
                .expect("order lock")
                .push("before_model");
            CallbackOutcome::Continue
        }))
        .after_model(Arc::new(move |_ctx, response| {
            after_model_order
                .lock()
                .expect("order lock")
                .push("after_model");
            CallbackOutcome::ReplaceWith(response.clone())
        }))
        .after_agent(Arc::new(move |_ctx, run| {
            assert_eq!(run.finish_reason, FinishReason::ModelStopped);
            after_agent_order
                .lock()
                .expect("order lock")
                .push("after_agent");
            CallbackOutcome::ReplaceWith(run.clone())
        }));

    let run = agent.run(&providers, "hello").await.expect("run ok");

    assert_eq!(run.output, "done");
    assert_eq!(
        order.lock().expect("order lock").as_slice(),
        ["before_agent", "before_model", "after_model", "after_agent"]
    );
}

#[tokio::test]
async fn agent_run_before_agent_replace_changes_user_turn() {
    let provider = RecordingProvider::new(vec![openai_text_response("done")]);
    let providers = registry(provider.clone());
    let agent = Agent::new("test", test_prompt()).before_agent(Arc::new(|_ctx, _input| {
        CallbackOutcome::ReplaceWith("replacement".to_owned())
    }));

    let run = agent.run(&providers, "original").await.expect("run ok");

    assert_eq!(user_turn_text(&run), Some("replacement"));
    match provider.requests().first().expect("request captured") {
        ProviderRequest::OpenAiChatCompletion(request) => {
            assert!(request.messages.iter().any(|message| {
                message.role == "user"
                    && matches!(
                        message.content.as_ref(),
                        Some(OpenAiMessageContent::Text(text)) if text == "replacement"
                    )
            }));
        }
        other => panic!("expected OpenAiChatCompletion, got {other:?}"),
    }
}

#[tokio::test]
async fn agent_run_before_agent_skip_returns_callback_skipped() {
    let provider = RecordingProvider::new(vec![openai_text_response("unused")]);
    let providers = registry(provider.clone());
    let after_agent_count = Arc::new(Mutex::new(0_u32));
    let after_agent_count_cb = Arc::clone(&after_agent_count);
    let agent = Agent::new("test", test_prompt())
        .before_agent(Arc::new(|_ctx, _input| CallbackOutcome::Skip))
        .after_agent(Arc::new(move |_ctx, run| {
            *after_agent_count_cb.lock().expect("counter lock") += 1;
            CallbackOutcome::ReplaceWith(run.clone())
        }));

    let run = agent.run(&providers, "hello").await.expect("run ok");

    assert_eq!(run.finish_reason, FinishReason::CallbackSkipped);
    assert_eq!(run.iterations, 0);
    assert_eq!(run.output, "");
    assert_eq!(provider.requests().len(), 0);
    assert_eq!(*after_agent_count.lock().expect("counter lock"), 0);
}

#[tokio::test]
async fn agent_run_before_model_replace_swaps_native_request() {
    let provider = RecordingProvider::new(vec![openai_text_response("done")]);
    let providers = registry(provider.clone());
    let agent = Agent::new("test", test_prompt()).before_model(Arc::new(|_ctx, request| {
        let mut replacement = request.clone();
        match &mut replacement {
            ProviderRequest::OpenAiChatCompletion(request) => {
                request.model = "replacement-model".to_owned();
            }
            other => panic!("expected OpenAiChatCompletion, got {other:?}"),
        }
        CallbackOutcome::ReplaceWith(replacement)
    }));

    let run = agent.run(&providers, "hello").await.expect("run ok");

    assert_eq!(run.finish_reason, FinishReason::ModelStopped);
    match provider.requests().first().expect("request captured") {
        ProviderRequest::OpenAiChatCompletion(request) => {
            assert_eq!(request.model, "replacement-model");
        }
        other => panic!("expected OpenAiChatCompletion, got {other:?}"),
    }
}

#[tokio::test]
async fn agent_run_before_model_skip_skips_provider_and_after_model() {
    let provider = RecordingProvider::new(vec![openai_text_response("unused")]);
    let providers = registry(provider.clone());
    let after_model_count = Arc::new(Mutex::new(0_u32));
    let after_model_count_cb = Arc::clone(&after_model_count);
    let agent = Agent::new("test", test_prompt())
        .before_model(Arc::new(|_ctx, _request| CallbackOutcome::Skip))
        .after_model(Arc::new(move |_ctx, response| {
            *after_model_count_cb.lock().expect("counter lock") += 1;
            CallbackOutcome::ReplaceWith(response.clone())
        }));

    let run = agent.run(&providers, "hello").await.expect("run ok");

    assert_eq!(run.finish_reason, FinishReason::CallbackSkipped);
    assert_eq!(run.iterations, 1);
    assert_eq!(provider.requests().len(), 0);
    assert_eq!(*after_model_count.lock().expect("counter lock"), 0);
}

#[tokio::test]
async fn agent_run_after_model_replace_swaps_response() {
    let provider = RecordingProvider::new(vec![openai_text_response("original")]);
    let providers = registry(provider);
    let agent = Agent::new("test", test_prompt()).after_model(Arc::new(|_ctx, _response| {
        CallbackOutcome::ReplaceWith(openai_text_response("replacement"))
    }));

    let run = agent.run(&providers, "hello").await.expect("run ok");

    assert_eq!(run.output, "replacement");
    assert_eq!(last_assistant_text(&run), Some("replacement"));
}

#[tokio::test]
async fn agent_run_after_agent_replace_can_alter_run() {
    let provider = RecordingProvider::new(vec![openai_text_response("model")]);
    let providers = registry(provider);
    let agent = Agent::new("test", test_prompt()).after_agent(Arc::new(|_ctx, run| {
        let mut replacement = run.clone();
        replacement.output = "after-agent".to_owned();
        CallbackOutcome::ReplaceWith(replacement)
    }));

    let run = agent.run(&providers, "hello").await.expect("run ok");

    assert_eq!(run.output, "after-agent");
    assert_eq!(run.finish_reason, FinishReason::ModelStopped);
}

#[tokio::test]
async fn agent_run_before_tool_replace_changes_tool_args() {
    let provider = RecordingProvider::new(vec![
        openai_tool_call_response(vec![tool_call("c1", "recorder", json!({"original": true}))]),
        openai_text_response("done"),
    ]);
    let providers = registry(provider);
    let tool = Arc::new(RecordingTool::new("recorder", json!({"tool": "ok"})));
    let agent = Agent::new("test", test_prompt())
        .add_tool(tool.clone())
        .before_tool(Arc::new(|_ctx, tool, _args| {
            assert_eq!(tool.name(), "recorder");
            CallbackOutcome::ReplaceWith(json!({"replacement": true}))
        }))
        .with_run_config(RunConfig {
            max_iterations: 3,
            ..Default::default()
        });

    let run = agent.run(&providers, "hello").await.expect("run ok");

    assert_eq!(run.output, "done");
    assert_eq!(tool.calls(), vec![json!({"replacement": true})]);
}

#[tokio::test]
async fn agent_run_before_tool_skip_records_sentinel_result() {
    let provider = RecordingProvider::new(vec![
        openai_tool_call_response(vec![tool_call("c1", "recorder", json!({"original": true}))]),
        openai_text_response("done"),
    ]);
    let providers = registry(provider);
    let tool = Arc::new(RecordingTool::new("recorder", json!({"tool": "ok"})));
    let after_tool_count = Arc::new(Mutex::new(0_u32));
    let after_tool_count_cb = Arc::clone(&after_tool_count);
    let agent = Agent::new("test", test_prompt())
        .add_tool(tool.clone())
        .before_tool(Arc::new(|_ctx, _tool, _args| CallbackOutcome::Skip))
        .after_tool(Arc::new(move |_ctx, _tool, result| {
            *after_tool_count_cb.lock().expect("counter lock") += 1;
            match result {
                Ok(value) => CallbackOutcome::ReplaceWith(Ok(value.clone())),
                Err(_) => CallbackOutcome::ReplaceWith(Ok(json!({"unused": true}))),
            }
        }))
        .with_run_config(RunConfig {
            max_iterations: 3,
            ..Default::default()
        });

    let run = agent.run(&providers, "hello").await.expect("run ok");

    assert_eq!(tool.calls().len(), 0);
    assert_eq!(*after_tool_count.lock().expect("counter lock"), 0);
    assert!(run.conversation.turns().iter().any(|turn| matches!(
        turn,
        ConversationTurn::ToolResult { call_id, ok, content }
            if call_id == "c1" && *ok && content == &json!({"skipped": true})
    )));
}

#[tokio::test]
async fn agent_run_after_tool_replace_changes_tool_result() {
    let provider = RecordingProvider::new(vec![
        openai_tool_call_response(vec![tool_call("c1", "recorder", json!({"a": 1}))]),
        openai_text_response("done"),
    ]);
    let providers = registry(provider);
    let tool = Arc::new(RecordingTool::new("recorder", json!({"original": true})));
    let agent = Agent::new("test", test_prompt())
        .add_tool(tool)
        .after_tool(Arc::new(|_ctx, tool, _result| {
            assert_eq!(tool.name(), "recorder");
            CallbackOutcome::ReplaceWith(Ok(json!({"replacement": true})))
        }))
        .with_run_config(RunConfig {
            max_iterations: 3,
            ..Default::default()
        });

    let run = agent.run(&providers, "hello").await.expect("run ok");

    assert!(run.conversation.turns().iter().any(|turn| matches!(
        turn,
        ConversationTurn::ToolResult { call_id, ok, content }
            if call_id == "c1" && *ok && content == &json!({"replacement": true})
    )));
}

#[tokio::test]
async fn agent_run_callback_panic_maps_to_agent_error() {
    let provider = RecordingProvider::new(vec![openai_text_response("unused")]);
    let providers = registry(provider);
    let agent = Agent::new("test", test_prompt()).before_model(Arc::new(|_ctx, _request| {
        panic!("callback exploded");
    }));

    let error = agent
        .run(&providers, "hello")
        .await
        .expect_err("callback panic should map to AgentError");

    assert_eq!(error.code(), "SKALD_AGENT_500_CALLBACK_PANIC");
    match error {
        AgentError::CallbackPanic { hook, payload } => {
            assert_eq!(hook, "before_model");
            assert!(payload.contains("callback exploded"));
        }
        other => panic!("expected CallbackPanic, got {other:?}"),
    }
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
struct RecordingTool {
    name: String,
    output: Value,
    calls: Mutex<Vec<Value>>,
}

impl RecordingTool {
    fn new(name: &str, output: Value) -> Self {
        Self {
            name: name.to_owned(),
            output,
            calls: Mutex::new(Vec::new()),
        }
    }

    fn calls(&self) -> Vec<Value> {
        match self.calls.lock() {
            Ok(guard) => guard.clone(),
            Err(poisoned) => poisoned.into_inner().clone(),
        }
    }
}

#[async_trait]
impl AgentTool for RecordingTool {
    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> &str {
        "records call args"
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

    async fn invoke(&self, args: Value) -> Result<Value, ToolError> {
        match self.calls.lock() {
            Ok(mut guard) => guard.push(args),
            Err(poisoned) => poisoned.into_inner().push(args),
        }
        Ok(self.output.clone())
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

fn user_turn_text(run: &AgentRun) -> Option<&str> {
    run.conversation.turns().iter().find_map(|turn| match turn {
        ConversationTurn::User { content } => Some(content.as_str()),
        _ => None,
    })
}

fn last_assistant_text(run: &AgentRun) -> Option<&str> {
    run.conversation
        .turns()
        .iter()
        .rev()
        .find_map(|turn| match turn {
            ConversationTurn::Assistant { message } => match message {
                skald_spec::MessageNum::OpenAi(message) => match message.content.as_ref() {
                    Some(OpenAiMessageContent::Text(text)) => Some(text.as_str()),
                    _ => None,
                },
                _ => None,
            },
            _ => None,
        })
}
