use std::sync::Arc;

use skald_agent::{Agent, RunConfig};
use skald_prompt::Prompt;
use skald_runtime::{MockProvider, ProviderRegistry};
use skald_spec::{
    Prompt as SpecPrompt, ProviderName, ProviderRequest, ProviderResponse, ResponseType,
    wire::openai_chat::{
        OpenAiChatChoice, OpenAiChatMessage, OpenAiChatRequest, OpenAiChatResponse,
        OpenAiChatSettings, OpenAiMessageContent, OpenAiResponseFormat, OpenAiToolCall,
        OpenAiToolFunctionCall,
    },
};

fn build_agent(
    request: ProviderRequest,
    mock: MockProvider,
    max_iterations: u32,
) -> (Agent, ProviderRegistry) {
    let mut providers = ProviderRegistry::new();
    providers.register(Arc::new(mock));

    let prompt = Arc::new(Prompt::from_native(
        SpecPrompt::new(request, "gpt-4o", None, ResponseType::Text).expect("prompt should build"),
    ));
    let agent = Agent::new("a", prompt).with_run_config(RunConfig {
        max_iterations,
        ..Default::default()
    });
    (agent, providers)
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

fn openai_tool_call_response(name: &str, args: serde_json::Value) -> ProviderResponse {
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
            },
            finish_reason: Some("tool_calls".to_owned()),
            logprobs: None,
        }],
        usage: None,
        system_fingerprint: None,
        service_tier: None,
    })
}

#[tokio::test]
async fn single_turn_no_tool_calls_returns_immediately() {
    let mock = MockProvider::new(ProviderName::OpenAi);
    mock.push_response(openai_text_response("hi"));
    let request = ProviderRequest::OpenAiChatCompletion(OpenAiChatRequest {
        model: "gpt-4o".to_owned(),
        messages: vec![],
        response_format: None,
        stream: None,
        stream_options: None,
        tools: None,
        tool_choice: None,
        parallel_tool_calls: None,
        settings: OpenAiChatSettings::default(),
    });
    let (agent, providers) = build_agent(request, mock, 10);
    let run = agent
        .run(&providers, "hello")
        .await
        .expect("single-turn run must succeed");

    assert_eq!(run.iterations, 1);
    assert_eq!(run.output, "hi");
    assert!(run.final_response.is_some());
}

#[tokio::test]
async fn tool_call_response_now_returns_tool_not_found() {
    let mock = MockProvider::new(ProviderName::OpenAi);
    mock.push_response(openai_tool_call_response(
        "echo",
        serde_json::json!({ "q": 1 }),
    ));
    let request = ProviderRequest::OpenAiChatCompletion(OpenAiChatRequest {
        model: "gpt-4o".to_owned(),
        messages: vec![],
        response_format: None,
        stream: None,
        stream_options: None,
        tools: None,
        tool_choice: None,
        parallel_tool_calls: None,
        settings: OpenAiChatSettings::default(),
    });
    let (agent, providers) = build_agent(request, mock, 10);
    let err = agent
        .run(&providers, "ask")
        .await
        .expect_err("tool invocation requires per-agent tool wiring");

    assert_eq!(err.code(), "SKALD_AGENT_404_TOOL_NOT_IN_AGENT");
    match err {
        skald_agent::AgentError::ToolNotInAgent { name } => assert_eq!(name, "echo"),
        other => panic!("expected ToolNotInAgent, got {other:?}"),
    }
}

#[tokio::test]
async fn provider_failure_surfaces_provider_code() {
    let mock = MockProvider::new(ProviderName::OpenAi);
    let request = ProviderRequest::OpenAiChatCompletion(OpenAiChatRequest {
        model: "gpt-4o".to_owned(),
        messages: vec![],
        response_format: None,
        stream: None,
        stream_options: None,
        tools: None,
        tool_choice: None,
        parallel_tool_calls: None,
        settings: OpenAiChatSettings::default(),
    });
    let (agent, providers) = build_agent(request, mock, 10);

    let err = agent
        .run(&providers, "x")
        .await
        .expect_err("empty mock queue must fail");
    assert_eq!(err.code(), "SKALD_AGENT_502_PROVIDER");
}

#[tokio::test]
async fn run_prompt_sends_rendered_prompt_verbatim_on_first_turn() {
    let template_request = ProviderRequest::OpenAiChatCompletion(OpenAiChatRequest {
        model: "gpt-4o".to_owned(),
        messages: vec![
            OpenAiChatMessage {
                role: "system".to_owned(),
                content: Some(OpenAiMessageContent::Text("be helpful".to_owned())),
                name: None,
                tool_calls: None,
                tool_call_id: None,
                refusal: None,
            },
            openai_user("hello {{name}}"),
        ],
        response_format: Some(OpenAiResponseFormat::JsonObject),
        stream: None,
        stream_options: None,
        tools: None,
        tool_choice: None,
        parallel_tool_calls: None,
        settings: OpenAiChatSettings::default(),
    });
    let expected_request = ProviderRequest::OpenAiChatCompletion(OpenAiChatRequest {
        model: "gpt-4o".to_owned(),
        messages: vec![
            OpenAiChatMessage {
                role: "system".to_owned(),
                content: Some(OpenAiMessageContent::Text("be helpful".to_owned())),
                name: None,
                tool_calls: None,
                tool_call_id: None,
                refusal: None,
            },
            openai_user("hello Ada"),
        ],
        response_format: Some(OpenAiResponseFormat::JsonObject),
        stream: None,
        stream_options: None,
        tools: None,
        tool_choice: None,
        parallel_tool_calls: None,
        settings: OpenAiChatSettings::default(),
    });
    let mock = MockProvider::new(ProviderName::OpenAi)
        .expect_request(expected_request.clone())
        .respond_with(openai_text_response("hi"));
    let mut providers = ProviderRegistry::new();
    providers.register(Arc::new(mock));

    let prompt = Arc::new(Prompt::from_native(
        SpecPrompt::new(template_request.clone(), "gpt-4o", None, ResponseType::Text).unwrap(),
    ));
    let agent = Agent::new("a", prompt).with_run_config(RunConfig {
        max_iterations: 10,
        ..Default::default()
    });

    let run = agent
        .run_prompt(
            &providers,
            &Prompt::from_native(
                SpecPrompt::new(template_request, "gpt-4o", None, ResponseType::Text).unwrap(),
            ),
            &[("name", "Ada")],
        )
        .await
        .expect("prompt run must succeed");
    assert_eq!(run.iterations, 1);
}

#[tokio::test]
async fn run_prompt_provider_mismatch_returns_409_provider_mismatch() {
    let agent_request = ProviderRequest::OpenAiChatCompletion(OpenAiChatRequest {
        model: "gpt-4o".to_owned(),
        messages: vec![],
        response_format: None,
        stream: None,
        stream_options: None,
        tools: None,
        tool_choice: None,
        parallel_tool_calls: None,
        settings: OpenAiChatSettings::default(),
    });
    let prompt_request = ProviderRequest::RawV1 {
        provider: ProviderName::Anthropic,
        body: serde_json::value::RawValue::from_string("{}".to_owned()).unwrap(),
    };
    let prompt = Prompt::from_native(
        SpecPrompt::new(prompt_request, "claude", None, ResponseType::Text).unwrap(),
    );
    let (agent, providers) =
        build_agent(agent_request, MockProvider::new(ProviderName::OpenAi), 10);

    let err = agent
        .run_prompt(&providers, &prompt, &[])
        .await
        .expect_err("provider mismatch must fail");
    assert_eq!(err.code(), "SKALD_AGENT_409_PROVIDER_MISMATCH");
}

#[tokio::test]
async fn run_prompt_missing_variable_emits_prompt_error() {
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
    let mock = MockProvider::new(ProviderName::OpenAi);
    let (agent, providers) = build_agent(request.clone(), mock, 10);
    let prompt = Arc::new(Prompt::from_native(
        SpecPrompt::new(request, "gpt-4o", None, ResponseType::Text).unwrap(),
    ));

    let err = agent
        .run_prompt(&providers, &prompt, &[])
        .await
        .expect_err("missing variables must fail");
    assert_eq!(err.code(), "SKALD_AGENT_422_PROMPT");
}
