use serde_json::json;
use skald_agent::{AgentDef, AgentError, NoopObserver, Observer, RunConfig};
use skald_spec::ProviderName;

#[test]
fn agent_def_round_trips_through_serde() {
    let def = AgentDef {
        id: "writer".into(),
        provider: ProviderName::OpenAi,
        system_prompt: Some("You write copy.".into()),
        model: Some("gpt-4o".into()),
        tool_names: vec!["search".into(), "fetch".into()],
        run_config: RunConfig { max_iterations: 7 },
    };
    let text = serde_json::to_string(&def).unwrap();
    let parsed: AgentDef = serde_json::from_str(&text).unwrap();
    assert_eq!(parsed, def);
}

#[test]
fn run_config_default_matches_agentic_default() {
    assert_eq!(RunConfig::default().max_iterations, 10);
}

#[test]
fn agent_def_omits_unset_optional_fields_in_json() {
    let def = AgentDef::new("a", ProviderName::Anthropic);
    let value = serde_json::to_value(&def).unwrap();
    let object = value.as_object().unwrap();
    assert!(!object.contains_key("system_prompt"));
    assert!(!object.contains_key("model"));
    assert_eq!(object.get("tool_names"), Some(&json!([])));
}

#[test]
fn agent_def_with_custom_provider_round_trips() {
    let def = AgentDef::new("a", ProviderName::Custom("internal".into()));
    let text = serde_json::to_string(&def).unwrap();
    let parsed: AgentDef = serde_json::from_str(&text).unwrap();
    assert_eq!(parsed, def);
}

#[test]
fn agent_error_codes_match_catalog() {
    assert_eq!(
        AgentError::ProviderNotFound {
            provider: ProviderName::OpenAi,
        }
        .code(),
        "SKALD_AGENT_404_PROVIDER"
    );
    assert_eq!(
        AgentError::ToolNotFound {
            name: "search".into(),
        }
        .code(),
        "SKALD_AGENT_404_TOOL"
    );
    assert_eq!(
        AgentError::InvalidToolArgs {
            tool: "search".into(),
            detail: "missing 'query' field".into(),
        }
        .code(),
        "SKALD_AGENT_422_TOOL_ARGS"
    );
    assert_eq!(
        AgentError::SystemPrompt {
            provider: ProviderName::OpenAi,
            detail: "unsupported".into(),
        }
        .code(),
        "SKALD_AGENT_422_SYSTEM_PROMPT"
    );
    assert_eq!(
        AgentError::max_iterations("a", 5).code(),
        "SKALD_AGENT_500_MAX_ITERATIONS"
    );
    assert_eq!(
        AgentError::Prompt {
            agent: "a".into(),
            detail: "render failed".into(),
        }
        .code(),
        "SKALD_AGENT_422_PROMPT"
    );
    assert_eq!(
        AgentError::ProviderMismatch {
            agent: "a".into(),
            agent_provider: ProviderName::OpenAi,
            prompt_provider: ProviderName::Anthropic,
        }
        .code(),
        "SKALD_AGENT_409_PROVIDER_MISMATCH"
    );
}

#[test]
fn noop_observer_compiles_against_trait_object() {
    let observer: Box<dyn Observer> = Box::new(NoopObserver);
    observer.on_agent_start("a", 10);
    observer.on_iteration("a", 1);
    observer.on_agent_finish("a", skald_agent::FinishReason::Stop, 1);
    observer.on_agent_error("a", "SKALD_AGENT_500_MAX_ITERATIONS", "exhausted");
}
