use std::collections::VecDeque;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;

use skald_agent::{
    Agent, FinishReason, RunConfig, WyrdGatewayCall, WyrdGatewayCaller, clear_prompt_card_registry,
    default_prompt_resolver, register_prompt_card, run_config_from_agent_run_config_spec,
};
use skald_providers::ProviderError;
use tokio_util::sync::CancellationToken;
use wyrd_spec::gateway::ModelRef;
use wyrd_spec::ids::ProviderDeploymentName;

use crate::callbacks::{
    RecordingTool, openai_text_response as callback_text_response, openai_tool_call_response,
    test_prompt as callback_prompt, tool_call,
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
use skald_spec::{ProviderName, ProviderRequest, ProviderResponse};
use skald_tool::{AgentTool, ToolDef, ToolRegistry};
use wyrd_spec::card::verifier::VerificationBinding;
use wyrd_spec::envelope::{CardKind, Spec};
use wyrd_spec::reference::{CardRef, InlineableRef, Ref};
use wyrd_spec::{AgentCard, AgentRunConfigSpec, AgentSpec};

/// An Agent authored in code saves, loads, and runs; then every loaded run
/// configuration shape and a verification binding survive export unchanged.
///
/// # Panics
/// Panics when a save, load, run, or [`assert_loaded_export_is_faithful`]
/// check fails.
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

    let card = loaded.to_card().expect("loaded agent exports");
    for run_config in [
        AgentRunConfigSpec::default(),
        AgentRunConfigSpec {
            timeout_ms: Some(5_000),
            ..AgentRunConfigSpec::default()
        },
        AgentRunConfigSpec {
            max_iterations: Some(10),
            tool_concurrency_cap: Some(8),
            ..AgentRunConfigSpec::default()
        },
        AgentRunConfigSpec {
            max_iterations: Some(3),
            tool_concurrency_cap: Some(2),
            session_recent_limit: Some(20),
            timeout_ms: Some(9_000),
        },
    ] {
        let mut authored = card.clone();
        authored.spec.run_config = run_config;
        authored.spec.verified_by = vec![VerificationBinding {
            verifier: Ref::Ref(CardRef {
                kind: CardKind::Verifier,
                ..prompt_card_ref("answers-grounded")
            }),
            runs_on: None,
            on_failure: Vec::new(),
        }];
        assert_loaded_export_is_faithful(&authored, &tools, &path);
    }
    let _ = std::fs::remove_file(path);
}

/// The canonical hash of an Agent spec, as registration computes it.
///
/// # Panics
/// Panics when the spec does not canonicalize.
fn spec_hash(spec: &AgentSpec) -> String {
    Spec::Agent(spec.clone())
        .canonical_hash()
        .expect("spec hashes")
        .to_string()
}

/// Proves an Agent loaded from `card` exports `card`'s exact spec and hash
/// through `to_card`, `to_wire`, and a YAML save and reload at `path`, keeps
/// its identity, Prompt ref and route, and runs with the defaults the authored
/// configuration resolves to. Edits through `with_run_config` and the public
/// `run_config` field then export only the edited field, keep every other
/// authored value and the bindings, and change the hash.
///
/// # Panics
/// Panics when any load, export, save, or equality check fails.
fn assert_loaded_export_is_faithful(card: &AgentCard, tools: &ToolRegistry, path: &std::path::Path) {
    let expected = spec_hash(&card.spec);
    let loaded =
        Agent::from_card(card.clone(), tools, default_prompt_resolver()).expect("card loads");
    let config = &card.spec.run_config;
    assert_eq!(loaded.run_config, run_config_from_agent_run_config_spec(config));
    assert_eq!(loaded.to_spec(), card.spec, "{config:?} exports as authored");
    let exported = loaded.to_card().expect("exports");
    assert_eq!(exported.spec, card.spec, "{config:?} to_card");
    assert_eq!(spec_hash(&exported.spec), expected, "{config:?} to_card hash");
    assert_eq!(loaded.to_wire().expect("wire").spec, card.spec, "{config:?} to_wire");
    assert_eq!(
        (exported.name.as_str(), exported.version.as_str()),
        (card.name.as_str(), card.version.as_str())
    );
    assert_eq!(exported.spec.prompt, card.spec.prompt);
    assert_eq!(exported.spec.execution_route, card.spec.execution_route);
    loaded.save(path).expect("saves");
    let reloaded = Agent::from_yaml_path(path, tools, default_prompt_resolver()).expect("reloads");
    assert_eq!(reloaded.to_spec(), card.spec, "{config:?} YAML round trip");
    assert_eq!(spec_hash(&reloaded.to_spec()), expected);

    let edited = reloaded.clone().with_run_config(RunConfig {
        max_iterations: 4,
        ..reloaded.run_config.clone()
    });
    let spec = edited.to_spec();
    assert_eq!(spec.run_config.max_iterations, Some(4), "{config:?} builder edit");
    assert_eq!(
        (spec.run_config.tool_concurrency_cap, spec.run_config.session_recent_limit, spec.run_config.timeout_ms),
        (config.tool_concurrency_cap, config.session_recent_limit, config.timeout_ms),
        "{config:?} unedited fields stay authored"
    );
    assert_eq!(spec.verified_by, card.spec.verified_by);
    assert_ne!(spec_hash(&spec), expected, "{config:?} an edit changes the hash");
    edited.save(path).expect("edited saves");
    let edited = Agent::from_yaml_path(path, tools, default_prompt_resolver()).expect("reloads");
    assert_eq!(edited.to_spec(), spec, "{config:?} the edit persists");

    let mut direct = reloaded;
    direct.run_config.timeout = Some(Duration::from_millis(7_000));
    let spec = direct.to_spec();
    assert_eq!(
        spec.run_config,
        AgentRunConfigSpec {
            timeout_ms: Some(7_000),
            ..config.clone()
        },
        "direct field edit"
    );
    assert_eq!(spec.verified_by, card.spec.verified_by);
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
///
/// # Panics
/// Panics when the Prompt does not round-trip with its custom provider or the
/// run fails or answers from another client.
#[tokio::test]
async fn custom_provider_prompt_round_trips_and_dispatches_to_its_client() {
    let mut native = test_prompt().into_native();
    native.provider = Some(ProviderName::Custom("local".to_owned()));
    let json = serde_json::to_value(&native).expect("prompt serializes");
    assert_eq!(json["provider"], "local");
    assert_eq!(json["format"], "chat_completion");
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
///
/// # Panics
/// Panics when the Prompt does not build or round-trip with the Vertex
/// provider, or the run fails or answers from another client.
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
    )
    .expect("static prompt is valid");
    native.provider = Some(ProviderName::Vertex);
    let json = serde_json::to_value(&native).expect("prompt serializes");
    assert_eq!(json["provider"], "vertex");
    assert_eq!(json["format"], "generate_content");
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

/// The native OpenAI Chat Prompt the Agent Card journey tests save and
/// reload.
///
/// # Panics
/// Panics when the static request does not build a Prompt.
fn test_prompt() -> Prompt {
    Prompt::from_native(
        skald_spec::Prompt::new(openai_request(), "gpt-4o-mini").expect("static prompt is valid"),
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

/// Gateway caller answering each call from a script and recording the
/// address every call targeted.
struct ScriptedGateway {
    /// Replies returned in order.
    replies: std::sync::Mutex<VecDeque<ProviderResponse>>,
    /// Gateway address of every call, in order.
    targets: std::sync::Mutex<Vec<ModelRef>>,
}

#[async_trait]
impl WyrdGatewayCaller for ScriptedGateway {
    /// Record the call's address and return the next scripted reply.
    ///
    /// # Errors
    /// Never fails.
    ///
    /// # Panics
    /// Panics when a lock is poisoned or the script has no reply left.
    async fn call(
        &self,
        call: WyrdGatewayCall,
        _cancellation: &CancellationToken,
    ) -> Result<ProviderResponse, ProviderError> {
        self.targets.lock().expect("targets lock").push(call.model);
        Ok(self
            .replies
            .lock()
            .expect("replies lock")
            .pop_front()
            .expect("a scripted reply remains"))
    }
}

/// A gateway Agent loaded from its Card keeps its declared route through
/// save and load, and that route — not the host's native providers — serves
/// every call of the bounded tool loop. A concurrent run overriding the route
/// on a clone targets only its own deployment while the loaded Agent and its
/// Card keep theirs, and the host's native fixture receives no call.
///
/// # Panics
/// Panics when the route is lost on save/load, a run fails, a model call
/// targets another deployment, or the native fixture is called.
#[tokio::test]
async fn execution_route_isolated_through_tool_loop() {
    let tool = Arc::new(RecordingTool::new(
        "lookup",
        serde_json::json!({"found": true}),
    ));
    let declared = wyrd_spec::ExecutionRoute::WyrdGateway {
        deployment: ProviderDeploymentName::new("primary").expect("deployment name"),
    };
    let yaml = Agent::from_resolved("routed", callback_prompt())
        .name("routed")
        .version("1.0.0")
        .add_tool(tool.clone())
        .with_run_config(RunConfig {
            max_iterations: 3,
            ..Default::default()
        })
        .with_execution_route(declared.clone())
        .to_yaml_string()
        .expect("agent serializes");
    let tools = ToolRegistry::new();
    tools.register(tool.clone()).expect("tool registers");
    let loaded =
        Agent::from_yaml_str(&yaml, &tools, default_prompt_resolver()).expect("agent loads");
    assert_eq!(loaded.execution_route(), &declared);
    assert_eq!(loaded.to_yaml_string().expect("agent re-serializes"), yaml);

    let gateway = |order: &str| {
        Arc::new(ScriptedGateway {
            replies: std::sync::Mutex::new(VecDeque::from([
                openai_tool_call_response(vec![tool_call(
                    "c1",
                    "lookup",
                    serde_json::json!({"order": order}),
                )]),
                callback_text_response("done"),
            ])),
            targets: std::sync::Mutex::new(Vec::new()),
        })
    };
    let (primary, backup) = (gateway("primary"), gateway("backup"));
    let primary_agent = loaded.clone().with_gateway_caller(primary.clone());
    let backup_agent = loaded
        .clone()
        .with_execution_route(wyrd_spec::ExecutionRoute::WyrdGateway {
            deployment: ProviderDeploymentName::new("backup").expect("deployment name"),
        })
        .with_gateway_caller(backup.clone());
    let native = MockProvider::new(ProviderName::OpenAi);
    native.push_response(callback_text_response("native"));
    let mut host = ProviderRegistry::new();
    host.register(Arc::new(native.clone()));

    let (primary_run, backup_run) = tokio::join!(
        primary_agent.run_with(&host, None, "where is my order?"),
        backup_agent.run_with(&host, None, "where is my order?")
    );

    assert_eq!(primary_run.expect("primary run").output, "done");
    assert_eq!(backup_run.expect("backup run").output, "done");
    let targets = |caller: &ScriptedGateway| {
        caller
            .targets
            .lock()
            .expect("targets lock")
            .iter()
            .map(|target| format!("{}/{}", target.deployment, target.model))
            .collect::<Vec<_>>()
    };
    assert_eq!(targets(&primary), ["primary/gpt-4o", "primary/gpt-4o"]);
    assert_eq!(targets(&backup), ["backup/gpt-4o", "backup/gpt-4o"]);
    assert_eq!(tool.calls().len(), 2, "each run called the tool once");
    assert_eq!(native.remaining(), 1, "the native fixture was never called");
    assert_eq!(
        loaded.execution_route(),
        &declared,
        "the override was local"
    );
}

/// A gateway Agent with no gateway caller refuses before any IO instead of
/// reaching the host's native provider credentials.
///
/// # Panics
/// Panics when the run does not fail with `SKALD_AGENT_422_INVALID_ARGUMENT`
/// or the native fixture is called.
#[tokio::test]
async fn gateway_route_without_a_caller_refuses_before_native_io() {
    let native = MockProvider::new(ProviderName::OpenAi);
    native.push_response(callback_text_response("native"));
    let mut host = ProviderRegistry::new();
    host.register(Arc::new(native.clone()));
    let agent = Agent::from_resolved("routed", callback_prompt()).with_execution_route(
        wyrd_spec::ExecutionRoute::WyrdGateway {
            deployment: ProviderDeploymentName::new("primary").expect("deployment name"),
        },
    );

    let error = agent
        .run_with(&host, None, "hello")
        .await
        .expect_err("a gateway route without a caller refuses");

    assert_eq!(error.code(), "SKALD_AGENT_422_INVALID_ARGUMENT");
    assert_eq!(native.remaining(), 1, "the native fixture was never called");
}
