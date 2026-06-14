use std::sync::{Arc, MutexGuard, OnceLock};

use skald_agent::{
    Agent, RunConfig, clear_prompt_card_registry, default_prompt_resolver, register_prompt_card,
    run_config_from_agent_run_config_spec,
};
use skald_prompt::Prompt;
use skald_runtime::{MockProvider, ProviderRegistry};
use skald_spec::{
    ProviderName, ProviderRequest, ProviderResponse, ResponseType,
    wire::openai_chat::{
        OpenAiChatChoice, OpenAiChatMessage, OpenAiChatRequest, OpenAiChatResponse,
        OpenAiChatSettings, OpenAiMessageContent,
    },
};
use skald_tool::{ToolDef, ToolRegistry};
use wyrd_spec::envelope::CardKind;
use wyrd_spec::reference::{CardRef, PromptRef};
use wyrd_spec::{AgentCard, AgentRunConfigSpec};

fn registry_lock() -> MutexGuard<'static, ()> {
    static LOCK: OnceLock<std::sync::Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| std::sync::Mutex::new(()))
        .lock()
        .unwrap_or_else(|e| e.into_inner())
}

fn temp_path(name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "wyrd_agent_card_c11_{}_{}.yaml",
        std::process::id(),
        name
    ))
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

fn prompt_card_ref(name: &str) -> CardRef {
    CardRef {
        kind: CardKind::Prompt,
        name: name.parse().expect("valid prompt card name"),
        version: "0.3.0".parse().expect("valid prompt version"),
        space: "research".parse().expect("valid space"),
        uid: None,
    }
}

fn echo_tool(name: &str) -> Arc<dyn skald_tool::AgentTool> {
    Arc::new(ToolDef::function(
        name.to_owned(),
        "echoes text",
        |input: String| -> Result<String, skald_tool::ToolError> { Ok(input) },
    ))
}

#[test]
fn agent_new_with_inline_prompt_is_infallible() {
    let agent = Agent::new(Prompt::from_native(prompt()));

    assert!(agent.meta().name.is_none());
    assert!(agent.tool_names().is_empty());
    assert_eq!(agent.run_config(), &RunConfig::default());
}

#[test]
fn agent_try_from_ref_inline_succeeds_against_noop_resolver() {
    let prompt_ref = PromptRef::from(prompt());
    let agent = Agent::try_from_ref(prompt_ref, default_prompt_resolver())
        .expect("inline prompts resolve without registry lookup");

    assert!(matches!(agent.prompt_ref(), PromptRef::Inline(_)));
}

#[test]
fn agent_try_from_ref_card_succeeds_with_test_resolver() {
    let _guard = registry_lock();
    clear_prompt_card_registry();
    let card_ref = prompt_card_ref("planner-prompt");
    register_prompt_card(&card_ref, Prompt::from_native(prompt())).expect("prompt registers");

    let agent = Agent::try_from_ref(PromptRef::from(card_ref.clone()), default_prompt_resolver())
        .expect("registered prompt resolves");

    assert_eq!(agent.cascade_children(), vec![card_ref]);
}

#[test]
fn agent_try_from_ref_missing_card_errors() {
    let _guard = registry_lock();
    clear_prompt_card_registry();
    let card_ref = prompt_card_ref("no-such");

    let err = Agent::try_from_ref(PromptRef::from(card_ref), default_prompt_resolver())
        .expect_err("missing prompt card rejects");

    assert_eq!(err.code(), "WYRD_AGENT_404_PROMPT_CARD");
}

#[test]
fn agent_with_chain_matches_locked_fixture() {
    let _guard = registry_lock();
    clear_prompt_card_registry();
    let card_ref = prompt_card_ref("planner-prompt");
    register_prompt_card(&card_ref, Prompt::from_native(prompt())).expect("prompt registers");
    let agent = Agent::try_from_ref(PromptRef::from(card_ref), default_prompt_resolver())
        .expect("registered prompt resolves")
        .name("planner-agent")
        .version("0.3.0")
        .space("research")
        .with_run_config(RunConfig {
            max_iterations: 6,
            tool_concurrency_cap: Some(2),
            session_recent_limit: Some(4),
            timeout: Some(std::time::Duration::from_millis(2_500)),
        });
    let path = temp_path("chain_parity");
    agent.save(&path).expect("agent saves");

    let expected: serde_yaml::Value =
        serde_yaml::from_str(include_str!("fixtures/agent_planner_v0.3.yaml"))
            .expect("fixture parses");
    let actual: serde_yaml::Value =
        serde_yaml::from_str(&std::fs::read_to_string(&path).expect("saved yaml reads"))
            .expect("saved yaml parses");

    assert_eq!(actual, expected);
    let _ = std::fs::remove_file(path);
}

#[test]
fn agent_projects_locked_agent_envelope_shape() {
    let agent = Agent::new(Prompt::from_native(prompt()))
        .name("planner-agent")
        .version("0.3.0")
        .space("research")
        .with_run_config(RunConfig {
            max_iterations: 6,
            tool_concurrency_cap: Some(2),
            session_recent_limit: Some(4),
            timeout: Some(std::time::Duration::from_millis(2_500)),
        });

    let yaml = agent.to_yaml_string().expect("agent serializes");
    let card: AgentCard = serde_yaml::from_str(&yaml).expect("agent card deserializes");

    assert!(yaml.contains("apiVersion: wyrd/v1"));
    assert!(yaml.contains("kind: Agent"));
    assert!(yaml.contains("relationships:"));
    assert!(yaml.contains("status: null"));
    assert!(!yaml.contains("kind: inline"));
    assert!(!yaml.contains("kind: card"));
    assert!(!yaml.contains("value:"));
    assert!(!yaml.contains(&format!("{}{}", "version", "_req")));
    assert!(!yaml.contains(&format!("{}{}", "version: ", "^")));
    assert_eq!(card.name, "planner-agent");
    assert_eq!(card.version, "0.3.0");
    assert_eq!(card.space, "research");
    assert_eq!(card.spec.run_config.max_iterations, Some(6));
    assert_eq!(card.spec.run_config.tool_concurrency_cap, Some(2));
    assert_eq!(card.spec.run_config.session_recent_limit, Some(4));
    assert_eq!(card.spec.run_config.timeout_ms, Some(2_500));
}

#[test]
fn save_requires_name_and_version_without_defaulting() {
    let nameless = Agent::new(Prompt::from_native(prompt()));
    let err = nameless.to_card().expect_err("missing name rejects");
    assert_eq!(err.code(), "WYRD_AGENT_422_MISSING_NAME");

    let versionless = nameless.name("planner-agent");
    let err = versionless.to_card().expect_err("missing version rejects");
    assert_eq!(err.code(), "WYRD_AGENT_422_MISSING_VERSION");
}

#[test]
fn local_save_load_allows_tools_but_registrable_rejects_them() {
    let path = temp_path("tools");
    let tool = echo_tool("echo_text");
    let agent = Agent::new(Prompt::from_native(prompt()))
        .name("planner-agent")
        .version("0.3.0")
        .with_tool(Arc::clone(&tool));

    let err = agent
        .validate_registrable()
        .expect_err("runtime-local tools are not registrable");
    assert_eq!(
        err.code(),
        "WYRD_AGENT_422_RUNTIME_LOCAL_TOOLS_NOT_REGISTRABLE"
    );

    agent.save(&path).expect("local save allows tools");
    let registry = ToolRegistry::new();
    registry
        .register(tool)
        .expect("tool registers for local load");
    let loaded = Agent::from_yaml_path(&path, &registry, default_prompt_resolver())
        .expect("local load resolves");

    assert_eq!(loaded.tool_names(), &["echo_text".to_owned()]);
    let _ = std::fs::remove_file(path);
}

#[test]
fn card_prompt_resolution_derives_cascade_and_missing_prompt_code() {
    let _guard = registry_lock();
    clear_prompt_card_registry();
    let card_ref = prompt_card_ref("planner-prompt");
    let err = Agent::try_from_ref(PromptRef::from(card_ref.clone()), default_prompt_resolver())
        .expect_err("missing prompt card rejects");
    assert_eq!(err.code(), "WYRD_AGENT_404_PROMPT_CARD");

    register_prompt_card(&card_ref, Prompt::from_native(prompt())).expect("prompt registers");
    let agent = Agent::try_from_ref(PromptRef::from(card_ref.clone()), default_prompt_resolver())
        .expect("registered prompt resolves")
        .name("planner-agent")
        .version("0.3.0")
        .space("research");
    let card = agent.to_card().expect("agent projects");

    assert_eq!(card.cascade_children, vec![card_ref]);
}

#[test]
fn unknown_runtime_local_tool_uses_wyrd_code() {
    let card = AgentCard {
        space: "default".to_owned(),
        name: "planner-agent".to_owned(),
        version: "0.3.0".to_owned(),
        uid: String::new(),
        labels: Default::default(),
        annotations: Default::default(),
        spec: wyrd_spec::card::agent::AgentSpec {
            prompt: PromptRef::from(prompt()),
            tool_names: vec!["missing_tool".to_owned()],
            run_config: AgentRunConfigSpec::default(),
        },
        cascade_children: Vec::new(),
        created_at: chrono::Utc::now(),
    };
    let registry = ToolRegistry::new();

    let err = Agent::from_card(card, &registry, default_prompt_resolver())
        .expect_err("missing tool rejects");

    assert_eq!(err.code(), "WYRD_AGENT_404_RUNTIME_LOCAL_TOOL_NOT_FOUND");
}

#[test]
fn fixture_loads_with_single_version_field() {
    let _guard = registry_lock();
    clear_prompt_card_registry();
    let fixture = include_str!("fixtures/agent_planner_v0.3.yaml");
    assert!(fixture.contains("apiVersion: wyrd/v1"));
    assert!(fixture.contains("relationships:"));
    assert!(fixture.contains("status: null"));
    assert!(fixture.contains("kind: Prompt"));
    assert!(!fixture.contains("kind: card"));
    assert!(!fixture.contains("value:"));
    assert!(!fixture.contains(&format!("{}{}", "version", "_req")));
    assert!(!fixture.contains(&format!("{}{}", "version: ", "^")));

    register_prompt_card(
        &prompt_card_ref("planner-prompt"),
        Prompt::from_native(prompt()),
    )
    .expect("prompt registers");
    let registry = ToolRegistry::new();
    let agent =
        Agent::from_yaml_str(fixture, &registry, default_prompt_resolver()).expect("fixture loads");
    let card = agent.to_card().expect("fixture projects");

    assert_eq!(card.name, "planner-agent");
    assert_eq!(card.version, "0.3.0");
    assert_eq!(card.spec.run_config.session_recent_limit, Some(4));
}

#[test]
fn run_config_spec_converts_to_engine_config() {
    let spec = AgentRunConfigSpec {
        max_iterations: Some(3),
        tool_concurrency_cap: Some(1),
        session_recent_limit: Some(2),
        timeout_ms: Some(50),
    };
    let config = run_config_from_agent_run_config_spec(&spec);

    assert_eq!(config.max_iterations, 3);
    assert_eq!(config.tool_concurrency_cap, Some(1));
    assert_eq!(config.session_recent_limit, Some(2));
    assert_eq!(config.timeout, Some(std::time::Duration::from_millis(50)));
}

#[test]
fn clear_prompt_card_registry_removes_previously_registered_entry() {
    let _guard = registry_lock();
    let card_ref = prompt_card_ref("ephemeral-prompt");
    register_prompt_card(&card_ref, Prompt::from_native(prompt())).expect("registers ok");

    // Confirm it resolves before the clear.
    Agent::try_from_ref(PromptRef::from(card_ref.clone()), default_prompt_resolver())
        .expect("registered prompt resolves");

    clear_prompt_card_registry();

    let err = Agent::try_from_ref(PromptRef::from(card_ref), default_prompt_resolver())
        .expect_err("cleared entry must not resolve");
    assert_eq!(err.code(), "WYRD_AGENT_404_PROMPT_CARD");
}

#[test]
fn register_prompt_card_rejects_non_prompt_kind_card_ref() {
    let _guard = registry_lock();
    let mut card_ref = prompt_card_ref("some-agent");
    card_ref.kind = CardKind::Agent;

    let err = register_prompt_card(&card_ref, Prompt::from_native(prompt()))
        .expect_err("non-Prompt kind must fail");
    assert!(err.to_string().contains("Prompt Card refs"));
}

#[tokio::test]
async fn agent_run_uses_collapsed_agent_runtime() {
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
        .expect("engine run succeeds");

    assert_eq!(run.output, "done");
    assert_eq!(run.iterations, 1);
}
