use skald_agent::{AgentError, RunConfig};
use skald_spec::ProviderName;

#[test]
fn run_config_default_matches_agentic_default() {
    assert_eq!(RunConfig::default().max_iterations, 10);
}

#[test]
fn agent_error_codes_match_catalog() {
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
    assert_eq!(
        AgentError::max_iterations("a", 5).code(),
        "SKALD_AGENT_500_MAX_ITERATIONS"
    );
}
