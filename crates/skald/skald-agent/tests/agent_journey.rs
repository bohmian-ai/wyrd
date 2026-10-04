use std::sync::Arc;
use std::time::Duration;

use skald_agent::{
    Agent, FinishReason, RunConfig, clear_prompt_card_registry, default_prompt_resolver,
    register_prompt_card,
};
use skald_prompt::Prompt;
use skald_runtime::{MockProvider, ProviderRegistry};
use skald_spec::wire::google_generate::{
    GoogleContent, GoogleGenerateContentRequest, GoogleGenerateSettings, GooglePart,
};
use skald_spec::wire::openai_chat::{
    OpenAiChatChoice, OpenAiChatMessage, OpenAiChatRequest, OpenAiChatResponse, OpenAiChatSettings,
    OpenAiMessageContent,
};
use skald_spec::{ProviderName, ProviderRequest, ProviderResponse, ResponseType};
use skald_tool::{AgentTool, ToolDef, ToolRegistry};
use wyrd_spec::envelope::CardKind;
use wyrd_spec::reference::{CardRef, InlineableRef};

#[tokio::test]
async fn journey_author_save_load_run() {
    let path = temp_path("author_save_load_run");
    let tool = echo_tool("t");
    let agent = Agent::new(test_prompt())
        .name("planner")
        .version("0.3.0")
        .with_tool(Arc::clone(&tool))
        .with_run_config(RunConfig {
            max_iterations: 3,
            timeout: Some(Duration::from_secs(5)),
            ..Default::default()
        });

    agent.save(&path).expect("agent saves");

    let tools = ToolRegistry::new();
    tools.register(tool).expect("tool registers");
    let loaded =
        Agent::from_yaml_path(&path, &tools, default_prompt_resolver()).expect("agent loads");

    assert_eq!(loaded.name_str(), Some("planner"));
    assert_eq!(loaded.version_str(), Some("0.3.0"));
    assert_eq!(loaded.tool_names(), &["t".to_owned()]);

    let providers = mock_providers("done");
    let run = loaded
        .run_with(&providers, None, "draft the doc")
        .await
        .expect("agent runs");

    assert_eq!(run.finish_reason, FinishReason::ModelStopped);
    assert_eq!(run.output, "done");
    let _ = std::fs::remove_file(path);
}

#[tokio::test]
async fn journey_try_from_ref_resolves_card_prompt() {
    clear_prompt_card_registry();
    let card_ref = prompt_card_ref("planner-prompt");
    register_prompt_card(&card_ref, test_prompt()).expect("prompt registers");

    let agent = Agent::try_from_ref(
        InlineableRef::Ref(card_ref.clone()),
        default_prompt_resolver(),
    )
    .expect("prompt card resolves")
    .name("planner")
    .version("0.3.0");

    assert_eq!(agent.cascade_children(), vec![card_ref]);

    let providers = mock_providers("resolved");
    let run = agent
        .run_with(&providers, None, "draft the doc")
        .await
        .expect("agent runs");

    assert_eq!(run.finish_reason, FinishReason::ModelStopped);
    assert_eq!(run.output, "resolved");
    clear_prompt_card_registry();
}

/// An OpenAI Chat Prompt naming a custom provider keeps that provider across
/// a JSON round trip, and the Agent loop dispatches it to the client
/// registered under that name rather than to OpenAI.
#[tokio::test]
async fn custom_provider_prompt_round_trips_and_dispatches_to_its_client() {
    let mut native = test_prompt().into_native();
    native.provider = Some(ProviderName::Custom("local".to_owned()));
    let json = serde_json::to_value(&native).expect("prompt serializes");
    assert_eq!(json["provider"], serde_json::json!({"custom": "local"}));
    assert_eq!(json["request"]["provider"], "open_ai_chat_completion");
    let reparsed: skald_spec::Prompt = serde_json::from_value(json).expect("prompt reads back");
    assert_eq!(reparsed, native);

    let local = MockProvider::new(ProviderName::Custom("local".to_owned()));
    local.push_response(openai_text_response("local answer"));
    let mut providers = ProviderRegistry::new();
    providers.register(Arc::new(local));
    let run = Agent::new(Prompt::from_native(reparsed))
        .run_with(&providers, None, "hello")
        .await
        .expect("the local client answers");

    assert_eq!(run.output, "local answer");
}

/// A Vertex Prompt is a Google GenerateContent body naming the Vertex
/// provider; it keeps that provider across a JSON round trip, and the Agent
/// loop dispatches it to the Vertex client rather than to Google AI Studio.
#[tokio::test]
async fn vertex_prompt_round_trips_and_dispatches_to_the_vertex_client() {
    let mut native = skald_spec::Prompt::new(
        ProviderRequest::GeminiGenerateContent(GoogleGenerateContentRequest {
            contents: vec![GoogleContent {
                role: "user".to_owned(),
                parts: vec![GooglePart::Text {
                    text: "hello".to_owned(),
                }],
            }],
            system_instruction: None,
            tools: None,
            tool_config: None,
            settings: GoogleGenerateSettings::default(),
        }),
        "gemini-2.5-pro",
        None,
        ResponseType::Text,
    )
    .expect("static prompt is valid");
    native.provider = Some(ProviderName::Vertex);
    let json = serde_json::to_value(&native).expect("prompt serializes");
    assert_eq!(json["provider"], "vertex");
    assert_eq!(json["request"]["provider"], "gemini_generate_content");
    let reparsed: skald_spec::Prompt = serde_json::from_value(json).expect("prompt reads back");
    assert_eq!(reparsed, native);

    let vertex = MockProvider::new(ProviderName::Vertex);
    vertex.push_response(ProviderResponse::GeminiGenerateContent(
        serde_json::from_value(serde_json::json!({
            "candidates": [{
                "content": {"role": "model", "parts": [{"text": "vertex answer"}]},
                "finishReason": "STOP"
            }]
        }))
        .expect("static Google answer decodes"),
    ));
    let mut providers = ProviderRegistry::new();
    providers.register(Arc::new(vertex));
    let run = Agent::new(Prompt::from_native(reparsed))
        .run_with(&providers, None, "hello")
        .await
        .expect("the Vertex client answers");

    assert_eq!(run.output, "vertex answer");
}

fn temp_path(name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "wyrd_agent_journey_{}_{}.yaml",
        std::process::id(),
        name
    ))
}

fn test_prompt() -> Prompt {
    Prompt::from_native(
        skald_spec::Prompt::new(openai_request(), "gpt-4o-mini", None, ResponseType::Text)
            .expect("static prompt is valid"),
    )
}

fn openai_request() -> ProviderRequest {
    ProviderRequest::OpenAiChatCompletion(OpenAiChatRequest {
        model: "gpt-4o-mini".to_owned(),
        messages: vec![OpenAiChatMessage {
            role: "system".to_owned(),
            content: Some(OpenAiMessageContent::Text("be helpful".to_owned())),
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

fn mock_providers(text: &str) -> Arc<ProviderRegistry> {
    let mock = MockProvider::new(ProviderName::OpenAi);
    mock.push_response(openai_text_response(text));
    let mut providers = ProviderRegistry::new();
    providers.register(Arc::new(mock));
    Arc::new(providers)
}

fn echo_tool(name: &str) -> Arc<dyn AgentTool> {
    Arc::new(ToolDef::function(
        name.to_owned(),
        "echoes text",
        |input: String| -> Result<String, skald_tool::ToolError> { Ok(input) },
    ))
}

fn prompt_card_ref(name: &str) -> CardRef {
    CardRef {
        kind: CardKind::Prompt,
        name: name.parse().expect("valid prompt card name"),
        version: "1.0.0".parse().expect("valid prompt version"),
        space: Some("default".parse().expect("valid space")),
        uid: None,
    }
}
