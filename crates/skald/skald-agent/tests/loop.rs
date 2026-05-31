use std::sync::{Arc, Mutex, MutexGuard};

use async_trait::async_trait;
use serde_json::value::RawValue;
use serde_json::{Map, Value, json};
use skald_agent::{
    Agent, AgentDef, AgentRun, AgentTool, AgentToolError, FinishReason, Observer, RunConfig,
    ToolRegistry,
};
use skald_runtime::{MockProvider, ProviderRegistry};
use skald_spec::wire::anthropic_messages::{
    AnthropicContentBlock, AnthropicMessage, AnthropicMessagesRequest, AnthropicMessagesResponse,
    AnthropicMessagesSettings, AnthropicStopReason, AnthropicSystem, AnthropicTool,
    AnthropicToolResultContent, AnthropicUsage,
};
use skald_spec::wire::openai_chat::{
    OpenAiChatChoice, OpenAiChatMessage, OpenAiChatRequest, OpenAiChatResponse, OpenAiChatSettings,
    OpenAiFunction, OpenAiJsonSchema, OpenAiMessageContent, OpenAiResponseFormat, OpenAiTool,
    OpenAiToolCall, OpenAiToolFunctionCall,
};
use skald_spec::{Prompt, ProviderName, ProviderRequest, ProviderResponse, ResponseType};
use skald_tool::ToolDef;

#[derive(Default)]
struct RecordingObserver {
    events: Mutex<Vec<String>>,
}

impl RecordingObserver {
    fn events(&self) -> Vec<String> {
        self.events_guard().clone()
    }

    fn terminal_count(&self) -> usize {
        self.events_guard()
            .iter()
            .filter(|event| event.starts_with("finish:") || event.starts_with("error:"))
            .count()
    }

    fn events_guard(&self) -> MutexGuard<'_, Vec<String>> {
        match self.events.lock() {
            Ok(events) => events,
            Err(poisoned) => poisoned.into_inner(),
        }
    }
}

impl Observer for RecordingObserver {
    fn on_agent_start(&self, agent_id: &str, _cap: u32) {
        self.events_guard().push(format!("start:{agent_id}"));
    }

    fn on_iteration(&self, _id: &str, iteration: u32) {
        self.events_guard().push(format!("iter:{iteration}"));
    }

    fn on_tool_call(&self, _id: &str, tool: &str, _args: &RawValue) {
        self.events_guard().push(format!("tool_call:{tool}"));
    }

    fn on_tool_result(&self, _id: &str, tool: &str, ok: bool) {
        self.events_guard().push(format!("tool_result:{tool}:{ok}"));
    }

    fn on_agent_finish(&self, _id: &str, finish: FinishReason, iterations: u32) {
        self.events_guard()
            .push(format!("finish:{finish:?}:{iterations}"));
    }

    fn on_agent_error(&self, _id: &str, code: &'static str, _detail: &str) {
        self.events_guard().push(format!("error:{code}"));
    }
}

struct EchoTool {
    def: ToolDef,
}

impl EchoTool {
    fn new() -> Self {
        Self {
            def: echo_tool_def(),
        }
    }
}

#[async_trait]
impl AgentTool for EchoTool {
    fn name(&self) -> &str {
        &self.def.name
    }

    fn def(&self) -> &ToolDef {
        &self.def
    }

    async fn call(&self, args: &RawValue) -> Result<Box<RawValue>, AgentToolError> {
        let parsed: Value = serde_json::from_str(args.get()).unwrap_or(Value::Null);
        RawValue::from_string(parsed.to_string()).map_err(|error| AgentToolError::Execution {
            tool: self.name().to_owned(),
            detail: error.to_string(),
        })
    }
}

fn echo_tool_def() -> ToolDef {
    ToolDef::new(
        "echo",
        "Echo back the JSON args.",
        json!({ "type": "object", "additionalProperties": true }),
    )
    .expect("static tool def must validate")
}

async fn build_agent_with(
    provider_name: ProviderName,
    mock: MockProvider,
    tool: bool,
    observer: Arc<dyn Observer>,
    max_iterations: u32,
) -> Agent {
    let mut providers = ProviderRegistry::new();
    providers.register(Arc::new(mock));
    let mut tools = ToolRegistry::new();
    if tool {
        tools.register(Arc::new(EchoTool::new()));
    }
    let def = AgentDef {
        id: "a".to_owned(),
        provider: provider_name,
        system_prompt: Some("be helpful".to_owned()),
        model: Some("gpt-4o".to_owned()),
        tool_names: if tool {
            vec!["echo".to_owned()]
        } else {
            Vec::new()
        },
        run_config: RunConfig { max_iterations },
    };
    Agent::from_def(def, &providers, &tools, observer)
        .await
        .expect("agent must bind")
}

fn openai_user(text: &str) -> OpenAiChatMessage {
    OpenAiChatMessage {
        role: "user".to_owned(),
        content: Some(OpenAiMessageContent::Text(text.to_owned())),
        name: None,
        tool_calls: None,
        tool_call_id: None,
        refusal: None,
    }
}

fn openai_system(text: &str) -> OpenAiChatMessage {
    OpenAiChatMessage {
        role: "system".to_owned(),
        content: Some(OpenAiMessageContent::Text(text.to_owned())),
        name: None,
        tool_calls: None,
        tool_call_id: None,
        refusal: None,
    }
}

fn openai_assistant_tool_call(name: &str, args: Value) -> OpenAiChatMessage {
    OpenAiChatMessage {
        role: "assistant".to_owned(),
        content: None,
        name: None,
        tool_calls: Some(vec![OpenAiToolCall {
            id: "call_1".to_owned(),
            kind: "function".to_owned(),
            function: OpenAiToolFunctionCall {
                name: name.to_owned(),
                arguments: args.to_string(),
            },
        }]),
        tool_call_id: None,
        refusal: None,
    }
}

fn openai_tool_result(name: &str, payload: &str) -> OpenAiChatMessage {
    OpenAiChatMessage {
        role: "tool".to_owned(),
        content: Some(OpenAiMessageContent::Text(payload.to_owned())),
        name: Some(name.to_owned()),
        tool_calls: None,
        tool_call_id: Some("call_1".to_owned()),
        refusal: None,
    }
}

fn openai_tool_def() -> OpenAiTool {
    OpenAiTool::Function {
        function: OpenAiFunction {
            name: "echo".to_owned(),
            description: Some("Echo back the JSON args.".to_owned()),
            parameters: echo_tool_def().input_schema.as_object().cloned(),
            strict: None,
        },
    }
}

fn openai_text_response(text: &str) -> ProviderResponse {
    ProviderResponse::OpenAiChatCompletion(OpenAiChatResponse {
        id: "resp_1".to_owned(),
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

fn openai_tool_call_response(name: &str, args: Value) -> ProviderResponse {
    ProviderResponse::OpenAiChatCompletion(OpenAiChatResponse {
        id: "resp_call".to_owned(),
        object: "chat.completion".to_owned(),
        created: 0,
        model: "gpt-4o".to_owned(),
        choices: vec![OpenAiChatChoice {
            index: 0,
            message: openai_assistant_tool_call(name, args),
            finish_reason: Some("tool_calls".to_owned()),
            logprobs: None,
        }],
        usage: None,
        system_fingerprint: None,
        service_tier: None,
    })
}

fn anthropic_user(text: &str) -> AnthropicMessage {
    AnthropicMessage {
        role: "user".to_owned(),
        content: vec![AnthropicContentBlock::Text {
            text: text.to_owned(),
            cache_control: None,
            citations: None,
        }],
    }
}

fn anthropic_assistant_tool_call(name: &str, args: Value) -> AnthropicMessage {
    AnthropicMessage {
        role: "assistant".to_owned(),
        content: vec![AnthropicContentBlock::ToolUse {
            id: "call_1".to_owned(),
            name: name.to_owned(),
            input: args,
        }],
    }
}

fn anthropic_tool_result(payload: &str) -> AnthropicMessage {
    AnthropicMessage {
        role: "user".to_owned(),
        content: vec![AnthropicContentBlock::ToolResult {
            tool_use_id: "call_1".to_owned(),
            content: AnthropicToolResultContent::Text(payload.to_owned()),
            is_error: Some(false),
            cache_control: None,
        }],
    }
}

fn anthropic_tool_def() -> AnthropicTool {
    AnthropicTool {
        name: "echo".to_owned(),
        description: Some("Echo back the JSON args.".to_owned()),
        input_schema: echo_tool_def().input_schema,
        cache_control: None,
        kind: None,
        display_width_px: None,
        display_height_px: None,
        display_number: None,
    }
}

fn anthropic_text_response(text: &str) -> ProviderResponse {
    ProviderResponse::AnthropicMessage(AnthropicMessagesResponse {
        id: "msg_1".to_owned(),
        r#type: "message".to_owned(),
        role: "assistant".to_owned(),
        model: "claude".to_owned(),
        content: vec![AnthropicContentBlock::Text {
            text: text.to_owned(),
            cache_control: None,
            citations: None,
        }],
        stop_reason: Some(AnthropicStopReason::EndTurn),
        stop_sequence: None,
        usage: AnthropicUsage {
            input_tokens: 1,
            output_tokens: 1,
            cache_creation_input_tokens: 0,
            cache_read_input_tokens: 0,
        },
    })
}

fn anthropic_tool_call_response(name: &str, args: Value) -> ProviderResponse {
    ProviderResponse::AnthropicMessage(AnthropicMessagesResponse {
        id: "msg_call".to_owned(),
        r#type: "message".to_owned(),
        role: "assistant".to_owned(),
        model: "claude".to_owned(),
        content: vec![AnthropicContentBlock::ToolUse {
            id: "call_1".to_owned(),
            name: name.to_owned(),
            input: args,
        }],
        stop_reason: Some(AnthropicStopReason::ToolUse),
        stop_sequence: None,
        usage: AnthropicUsage {
            input_tokens: 1,
            output_tokens: 1,
            cache_creation_input_tokens: 0,
            cache_read_input_tokens: 0,
        },
    })
}

#[tokio::test]
async fn single_turn_no_tool_calls_returns_immediately() {
    let mock = MockProvider::new(ProviderName::OpenAi);
    mock.push_response(openai_text_response("hi"));
    let observer = Arc::new(RecordingObserver::default());
    let agent = build_agent_with(
        ProviderName::OpenAi,
        mock,
        false,
        observer.clone() as Arc<dyn Observer>,
        10,
    )
    .await;

    let run: AgentRun = agent.run("hello").await.expect("no-tool run must succeed");
    assert_eq!(run.iterations, 1);
    assert!(matches!(run.finish, FinishReason::Stop));
    let events = observer.events();
    assert_eq!(events[0], "start:a");
    assert_eq!(events[1], "iter:1");
    assert!(
        events
            .last()
            .is_some_and(|event| event.starts_with("finish:"))
    );
    assert_eq!(observer.terminal_count(), 1);
}

#[tokio::test]
async fn tool_call_then_text_terminates_after_two_iterations() {
    let mock = MockProvider::new(ProviderName::OpenAi);
    mock.push_response(openai_tool_call_response("echo", json!({"q": 1})));
    mock.push_response(openai_text_response("done"));
    let observer = Arc::new(RecordingObserver::default());
    let agent = build_agent_with(
        ProviderName::OpenAi,
        mock,
        true,
        observer.clone() as Arc<dyn Observer>,
        10,
    )
    .await;

    let run = agent.run("ask").await.expect("two-iteration run");
    assert_eq!(run.iterations, 2);
    let events = observer.events();
    assert!(events.iter().any(|event| event == "tool_call:echo"));
    assert!(events.iter().any(|event| event == "tool_result:echo:true"));
    assert_eq!(observer.terminal_count(), 1);
}

#[tokio::test]
async fn unbounded_tool_loop_caps_at_max_iterations() {
    let mock = MockProvider::new(ProviderName::OpenAi);
    for _ in 0..5 {
        mock.push_response(openai_tool_call_response("echo", json!({"q": 1})));
    }
    let observer = Arc::new(RecordingObserver::default());
    let agent = build_agent_with(
        ProviderName::OpenAi,
        mock,
        true,
        observer.clone() as Arc<dyn Observer>,
        3,
    )
    .await;

    let err = agent.run("loop").await.expect_err("must cap");
    assert_eq!(err.code(), "SKALD_AGENT_500_MAX_ITERATIONS");
    let events = observer.events();
    assert!(
        events
            .iter()
            .any(|event| event == "error:SKALD_AGENT_500_MAX_ITERATIONS"),
        "expected max-iterations terminal error, got {events:?}"
    );
    assert!(
        !events.iter().any(|event| event.starts_with("finish:")),
        "error path must not emit finish: {events:?}"
    );
    assert_eq!(observer.terminal_count(), 1);
}

#[tokio::test]
async fn unknown_tool_name_returns_404_tool() {
    let mock = MockProvider::new(ProviderName::OpenAi);
    mock.push_response(openai_tool_call_response("not_registered", json!({})));
    let observer = Arc::new(RecordingObserver::default());
    let agent = build_agent_with(
        ProviderName::OpenAi,
        mock,
        true,
        observer.clone() as Arc<dyn Observer>,
        10,
    )
    .await;

    let err = agent.run("ask").await.expect_err("unknown tool must fail");
    assert_eq!(err.code(), "SKALD_AGENT_404_TOOL");
    assert_eq!(observer.terminal_count(), 1);
}

#[tokio::test]
async fn provider_failure_surfaces_provider_code() {
    let mock = MockProvider::new(ProviderName::OpenAi);
    let observer = Arc::new(RecordingObserver::default());
    let agent = build_agent_with(
        ProviderName::OpenAi,
        mock,
        false,
        observer.clone() as Arc<dyn Observer>,
        10,
    )
    .await;

    let err = agent.run("x").await.expect_err("empty mock must fail");
    assert!(err.code().starts_with("SKALD_"));
    assert_eq!(observer.terminal_count(), 1);
}

#[tokio::test]
async fn anthropic_provider_emits_anthropic_request_shape() {
    let expected = ProviderRequest::AnthropicMessage(AnthropicMessagesRequest {
        model: "claude-3-5-sonnet-latest".to_owned(),
        messages: vec![anthropic_user("test")],
        system: Some(AnthropicSystem::Text("be concise".to_owned())),
        stream: None,
        tools: None,
        tool_choice: None,
        settings: AnthropicMessagesSettings::default(),
    });
    let mock = MockProvider::new(ProviderName::Anthropic)
        .expect_request(expected)
        .respond_with(anthropic_text_response("hi"));
    let mut providers = ProviderRegistry::new();
    providers.register(Arc::new(mock));
    let tools = ToolRegistry::new();
    let def = AgentDef {
        id: "a".to_owned(),
        provider: ProviderName::Anthropic,
        system_prompt: Some("be concise".to_owned()),
        model: Some("claude-3-5-sonnet-latest".to_owned()),
        tool_names: Vec::new(),
        run_config: RunConfig::default(),
    };
    let agent = Agent::from_def_noop(def, &providers, &tools)
        .await
        .expect("agent must bind");

    agent.run("test").await.expect("anthropic run");
}

#[tokio::test]
async fn observer_sees_lifecycle_in_order() {
    let mock = MockProvider::new(ProviderName::OpenAi);
    mock.push_response(openai_tool_call_response("echo", json!({"q": 1})));
    mock.push_response(openai_text_response("done"));
    let observer = Arc::new(RecordingObserver::default());
    let agent = build_agent_with(
        ProviderName::OpenAi,
        mock,
        true,
        observer.clone() as Arc<dyn Observer>,
        10,
    )
    .await;

    agent.run("ask").await.expect("run must succeed");
    let events = observer.events();
    assert_eq!(events[0], "start:a");
    assert_eq!(events[1], "iter:1");
    assert_eq!(events[2], "tool_call:echo");
    assert_eq!(events[3], "tool_result:echo:true");
    assert_eq!(events[4], "iter:2");
    assert!(events[5].starts_with("finish:"));
    assert_eq!(observer.terminal_count(), 1);
}

#[tokio::test]
async fn run_prompt_sends_rendered_prompt_verbatim_on_first_turn() {
    let request = ProviderRequest::OpenAiChatCompletion(OpenAiChatRequest {
        model: "gpt-4o".to_owned(),
        messages: vec![openai_system("be helpful"), openai_user("hello {{name}}")],
        response_format: None,
        stream: None,
        stream_options: None,
        tools: None,
        tool_choice: None,
        parallel_tool_calls: None,
        settings: OpenAiChatSettings::default(),
    });
    let prompt =
        Prompt::new(request, "gpt-4o", None, ResponseType::Text).expect("prompt must construct");
    let expected = ProviderRequest::OpenAiChatCompletion(OpenAiChatRequest {
        model: "gpt-4o".to_owned(),
        messages: vec![openai_system("be helpful"), openai_user("hello Ada")],
        response_format: None,
        stream: None,
        stream_options: None,
        tools: None,
        tool_choice: None,
        parallel_tool_calls: None,
        settings: OpenAiChatSettings::default(),
    });
    let mock = MockProvider::new(ProviderName::OpenAi)
        .expect_request(expected)
        .respond_with(openai_text_response("hi"));
    let observer = Arc::new(RecordingObserver::default());
    let agent = build_agent_with(
        ProviderName::OpenAi,
        mock,
        false,
        observer.clone() as Arc<dyn Observer>,
        10,
    )
    .await;

    let run = agent
        .run_prompt(&prompt, &[("name", "Ada")])
        .await
        .expect("prompt run must succeed");
    assert_eq!(run.iterations, 1);
    assert_eq!(observer.terminal_count(), 1);
}

#[tokio::test]
async fn run_prompt_provider_mismatch_returns_409_provider_mismatch() {
    let request = ProviderRequest::AnthropicMessage(AnthropicMessagesRequest {
        model: "claude".to_owned(),
        messages: vec![anthropic_user("hello")],
        system: None,
        stream: None,
        tools: None,
        tool_choice: None,
        settings: AnthropicMessagesSettings::default(),
    });
    let prompt =
        Prompt::new(request, "claude", None, ResponseType::Text).expect("prompt must construct");
    let mock = MockProvider::new(ProviderName::OpenAi);
    let observer = Arc::new(RecordingObserver::default());
    let agent = build_agent_with(
        ProviderName::OpenAi,
        mock,
        false,
        observer.clone() as Arc<dyn Observer>,
        10,
    )
    .await;

    let err = agent
        .run_prompt(&prompt, &[])
        .await
        .expect_err("provider mismatch must fail");
    assert_eq!(err.code(), "SKALD_AGENT_409_PROVIDER_MISMATCH");
    assert_eq!(observer.terminal_count(), 1);
}

#[tokio::test]
async fn run_prompt_openai_template_preserves_non_message_fields_on_second_turn() {
    let mut schema = Map::new();
    schema.insert("type".to_owned(), Value::String("object".to_owned()));
    let mut extra = Map::new();
    extra.insert("x-skald-test".to_owned(), json!(true));
    let settings = OpenAiChatSettings {
        temperature: Some(0.2),
        extra,
        ..OpenAiChatSettings::default()
    };
    let response_format = Some(OpenAiResponseFormat::JsonSchema {
        json_schema: OpenAiJsonSchema {
            name: "answer".to_owned(),
            description: Some("Answer payload.".to_owned()),
            schema: Some(schema),
            strict: Some(true),
        },
    });
    let tool = openai_tool_def();
    let first_request = ProviderRequest::OpenAiChatCompletion(OpenAiChatRequest {
        model: "gpt-4o".to_owned(),
        messages: vec![openai_user("question: {{q}}")],
        response_format: response_format.clone(),
        stream: None,
        stream_options: None,
        tools: Some(vec![tool.clone()]),
        tool_choice: None,
        parallel_tool_calls: Some(true),
        settings: settings.clone(),
    });
    let prompt = Prompt::new(first_request, "gpt-4o", None, ResponseType::Text)
        .expect("prompt must construct");
    let rendered_first = ProviderRequest::OpenAiChatCompletion(OpenAiChatRequest {
        model: "gpt-4o".to_owned(),
        messages: vec![openai_user("question: weather")],
        response_format: response_format.clone(),
        stream: None,
        stream_options: None,
        tools: Some(vec![tool.clone()]),
        tool_choice: None,
        parallel_tool_calls: Some(true),
        settings: settings.clone(),
    });
    let rendered_second = ProviderRequest::OpenAiChatCompletion(OpenAiChatRequest {
        model: "gpt-4o".to_owned(),
        messages: vec![
            openai_user("question: weather"),
            openai_assistant_tool_call("echo", json!({"q": 1})),
            openai_tool_result("echo", r#"{"q":1}"#),
        ],
        response_format,
        stream: None,
        stream_options: None,
        tools: Some(vec![tool]),
        tool_choice: None,
        parallel_tool_calls: Some(true),
        settings,
    });
    let mock = MockProvider::new(ProviderName::OpenAi)
        .expect_request(rendered_first)
        .respond_with(openai_tool_call_response("echo", json!({"q": 1})))
        .expect_request(rendered_second)
        .respond_with(openai_text_response("done"));
    let observer = Arc::new(RecordingObserver::default());
    let agent = build_agent_with(
        ProviderName::OpenAi,
        mock,
        true,
        observer.clone() as Arc<dyn Observer>,
        10,
    )
    .await;

    let run = agent
        .run_prompt(&prompt, &[("q", "weather")])
        .await
        .expect("prompt tool run must succeed");
    assert_eq!(run.iterations, 2);
    assert_eq!(observer.terminal_count(), 1);
}

#[tokio::test]
async fn run_prompt_anthropic_template_preserves_system_tools_and_settings_on_second_turn() {
    let mut extra = Map::new();
    extra.insert("x-skald-test".to_owned(), json!("kept"));
    let settings = AnthropicMessagesSettings {
        max_tokens: 123,
        temperature: Some(0.2),
        extra,
        ..AnthropicMessagesSettings::default()
    };
    let tool = anthropic_tool_def();
    let prompt_request = ProviderRequest::AnthropicMessage(AnthropicMessagesRequest {
        model: "claude-3-5-sonnet-latest".to_owned(),
        messages: vec![anthropic_user("question: {{q}}")],
        system: Some(AnthropicSystem::Text("be direct".to_owned())),
        stream: None,
        tools: Some(vec![tool.clone()]),
        tool_choice: Some(json!({"type": "auto"})),
        settings: settings.clone(),
    });
    let prompt = Prompt::new(prompt_request, "claude", None, ResponseType::Text)
        .expect("prompt must construct");
    let rendered_first = ProviderRequest::AnthropicMessage(AnthropicMessagesRequest {
        model: "claude-3-5-sonnet-latest".to_owned(),
        messages: vec![anthropic_user("question: weather")],
        system: Some(AnthropicSystem::Text("be direct".to_owned())),
        stream: None,
        tools: Some(vec![tool.clone()]),
        tool_choice: Some(json!({"type": "auto"})),
        settings: settings.clone(),
    });
    let rendered_second = ProviderRequest::AnthropicMessage(AnthropicMessagesRequest {
        model: "claude-3-5-sonnet-latest".to_owned(),
        messages: vec![
            anthropic_user("question: weather"),
            anthropic_assistant_tool_call("echo", json!({"q": 1})),
            anthropic_tool_result(r#"{"q":1}"#),
        ],
        system: Some(AnthropicSystem::Text("be direct".to_owned())),
        stream: None,
        tools: Some(vec![tool]),
        tool_choice: Some(json!({"type": "auto"})),
        settings,
    });
    let mock = MockProvider::new(ProviderName::Anthropic)
        .expect_request(rendered_first)
        .respond_with(anthropic_tool_call_response("echo", json!({"q": 1})))
        .expect_request(rendered_second)
        .respond_with(anthropic_text_response("done"));
    let observer = Arc::new(RecordingObserver::default());
    let agent = build_agent_with(
        ProviderName::Anthropic,
        mock,
        true,
        observer.clone() as Arc<dyn Observer>,
        10,
    )
    .await;

    let run = agent
        .run_prompt(&prompt, &[("q", "weather")])
        .await
        .expect("prompt tool run must succeed");
    assert_eq!(run.iterations, 2);
    assert_eq!(observer.terminal_count(), 1);
}

#[tokio::test]
async fn run_prompt_missing_variable_emits_prompt_error_terminal_event() {
    let request = ProviderRequest::OpenAiChatCompletion(OpenAiChatRequest {
        model: "gpt-4o".to_owned(),
        messages: vec![openai_user("hello {{name}}")],
        response_format: None,
        stream: None,
        stream_options: None,
        tools: None,
        tool_choice: None,
        parallel_tool_calls: None,
        settings: OpenAiChatSettings::default(),
    });
    let prompt =
        Prompt::new(request, "gpt-4o", None, ResponseType::Text).expect("prompt must construct");
    let mock = MockProvider::new(ProviderName::OpenAi);
    let observer = Arc::new(RecordingObserver::default());
    let agent = build_agent_with(
        ProviderName::OpenAi,
        mock,
        false,
        observer.clone() as Arc<dyn Observer>,
        10,
    )
    .await;

    let err = agent
        .run_prompt(&prompt, &[])
        .await
        .expect_err("missing variable must fail");
    assert_eq!(err.code(), "SKALD_AGENT_422_PROMPT");
    let events = observer.events();
    assert_eq!(events[0], "start:a");
    assert!(
        events
            .iter()
            .any(|event| event == "error:SKALD_AGENT_422_PROMPT"),
        "expected prompt terminal error, got {events:?}"
    );
    assert!(!events.iter().any(|event| event.starts_with("finish:")));
    assert_eq!(observer.terminal_count(), 1);
}
