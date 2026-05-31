use std::sync::Arc;

use async_trait::async_trait;
use serde_json::value::RawValue;
use serde_json::{Value, json};
use skald_agent::{
    Agent, AgentBuilder, AgentDef, AgentError, AgentTool, AgentToolError, RunConfig, ToolRegistry,
};
use skald_runtime::{MockProvider, ProviderRegistry};
use skald_spec::{MessageNum, ProviderName};
use skald_tool::ToolDef;

struct EchoTool {
    def: ToolDef,
}

impl EchoTool {
    fn new() -> Self {
        Self {
            def: ToolDef::new(
                "echo",
                "Echoes back the supplied JSON.",
                json!({ "type": "object", "additionalProperties": true }),
            )
            .expect("static tool def must validate"),
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

fn registry_with_mock(name: ProviderName) -> (ProviderRegistry, MockProvider) {
    let mock = MockProvider::new(name);
    let mut registry = ProviderRegistry::new();
    registry.register(Arc::new(mock.clone()));
    (registry, mock)
}

#[tokio::test]
async fn from_def_binds_provider_by_name() {
    let (providers, _mock) = registry_with_mock(ProviderName::OpenAi);
    let mut tools = ToolRegistry::new();
    tools.register(Arc::new(EchoTool::new()));

    let def = AgentDef {
        id: "writer".into(),
        provider: ProviderName::OpenAi,
        system_prompt: Some("You are a helpful assistant.".into()),
        model: Some("gpt-4o".into()),
        tool_names: vec!["echo".into()],
        run_config: RunConfig::default(),
    };
    let agent = Agent::from_def_noop(def, &providers, &tools)
        .await
        .expect("from_def must bind");

    assert_eq!(agent.id, "writer");
    assert_eq!(agent.provider_name, ProviderName::OpenAi);
    assert_eq!(agent.tools().len(), 1);
    assert_eq!(agent.system_instruction().len(), 1);
}

#[tokio::test]
async fn from_def_unknown_provider_returns_404_provider() {
    let providers = ProviderRegistry::new();
    let tools = ToolRegistry::new();
    let def = AgentDef::new("a", ProviderName::Anthropic);

    let err = Agent::from_def_noop(def, &providers, &tools)
        .await
        .expect_err("missing provider must fail");

    assert_eq!(err.code(), "SKALD_AGENT_404_PROVIDER");
}

#[tokio::test]
async fn from_def_unknown_tool_returns_404_tool() {
    let (providers, _mock) = registry_with_mock(ProviderName::OpenAi);
    let tools = ToolRegistry::new();
    let def = AgentDef {
        id: "a".into(),
        provider: ProviderName::OpenAi,
        system_prompt: None,
        model: None,
        tool_names: vec!["search".into()],
        run_config: RunConfig::default(),
    };

    let err = Agent::from_def_noop(def, &providers, &tools)
        .await
        .expect_err("missing tool must fail");

    assert_eq!(err.code(), "SKALD_AGENT_404_TOOL");
    match err {
        AgentError::ToolNotFound { name } => assert_eq!(name, "search"),
        other => panic!("expected ToolNotFound, got {other:?}"),
    }
}

#[tokio::test]
async fn from_def_custom_provider_with_system_prompt_errors() {
    let mock = MockProvider::new(ProviderName::Custom("internal".into()));
    let mut providers = ProviderRegistry::new();
    providers.register(Arc::new(mock));
    let tools = ToolRegistry::new();
    let def = AgentDef {
        id: "a".into(),
        provider: ProviderName::Custom("internal".into()),
        system_prompt: Some("custom system".into()),
        model: None,
        tool_names: Vec::new(),
        run_config: RunConfig::default(),
    };

    let err = Agent::from_def_noop(def, &providers, &tools)
        .await
        .expect_err("custom + system_prompt must fail");

    assert_eq!(err.code(), "SKALD_AGENT_422_SYSTEM_PROMPT");
}

#[tokio::test]
async fn from_def_no_system_prompt_yields_empty_instruction() {
    let (providers, _mock) = registry_with_mock(ProviderName::OpenAi);
    let tools = ToolRegistry::new();
    let def = AgentDef::new("a", ProviderName::OpenAi);

    let agent = Agent::from_def_noop(def, &providers, &tools)
        .await
        .expect("bind");

    assert!(agent.system_instruction().is_empty());
}

#[tokio::test]
async fn system_prompt_shapes_per_provider() {
    let (openai_providers, _) = registry_with_mock(ProviderName::OpenAi);
    let (anthropic_providers, _) = registry_with_mock(ProviderName::Anthropic);
    let (google_providers, _) = registry_with_mock(ProviderName::Google);
    let tools = ToolRegistry::new();

    let mk = |provider: ProviderName| AgentDef {
        id: "a".into(),
        provider,
        system_prompt: Some("be concise".into()),
        model: None,
        tool_names: Vec::new(),
        run_config: RunConfig::default(),
    };

    let openai_agent = Agent::from_def_noop(mk(ProviderName::OpenAi), &openai_providers, &tools)
        .await
        .expect("openai bind");
    let anthropic_agent =
        Agent::from_def_noop(mk(ProviderName::Anthropic), &anthropic_providers, &tools)
            .await
            .expect("anthropic bind");
    let google_agent = Agent::from_def_noop(mk(ProviderName::Google), &google_providers, &tools)
        .await
        .expect("google bind");

    assert!(matches!(
        openai_agent.system_instruction()[0],
        MessageNum::OpenAi(_)
    ));
    assert!(matches!(
        anthropic_agent.system_instruction()[0],
        MessageNum::Anthropic(_)
    ));
    assert!(matches!(
        google_agent.system_instruction()[0],
        MessageNum::Gemini(_)
    ));
}

#[tokio::test]
async fn agent_builder_constructs_without_registry() {
    let provider = Arc::new(MockProvider::new(ProviderName::OpenAi));

    let agent = AgentBuilder::new("writer", ProviderName::OpenAi, provider)
        .with_system_prompt("be helpful")
        .with_model("gpt-4o")
        .with_tool(Arc::new(EchoTool::new()))
        .build()
        .expect("builder must succeed");

    assert_eq!(agent.id, "writer");
    assert_eq!(agent.system_instruction().len(), 1);
    assert_eq!(agent.tools().len(), 1);
}
