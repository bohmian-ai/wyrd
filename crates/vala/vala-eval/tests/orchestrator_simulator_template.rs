mod orchestrator_support;

use std::sync::Arc;

use orchestrator_support::{registry_returning_texts, scenario};
use skald_agent::Agent;
use vala_eval::orchestrator::{
    OrchestratorError, SIMULATOR_PROMPT_TEMPLATE, ServerSimulatedUser, SimulatorPrompt,
};
use wyrd_spec::vala::eval::protocol::{ConversationTurn, TurnRole};

#[test]
fn simulator_template_renders_stably() {
    let scenario = scenario("template", Vec::new(), None, 3);
    let history = vec![
        ConversationTurn {
            role: TurnRole::User,
            content: "Start".to_owned(),
        },
        ConversationTurn {
            role: TurnRole::Agent,
            content: "I can help".to_owned(),
        },
    ];

    let rendered = SimulatorPrompt::render(&scenario, &history)
        .expect("template renders")
        .rendered;

    assert!(SIMULATOR_PROMPT_TEMPLATE.contains("{{persona}}"));
    assert!(rendered.contains("A concise product user."));
    assert!(rendered.contains("Your initial request was: Start"));
    assert!(rendered.contains("USER: Start"));
    assert!(rendered.contains("AGENT: I can help"));
    assert!(rendered.contains("\"goal_achieved\": boolean"));
}

#[tokio::test(flavor = "multi_thread")]
async fn server_simulated_user_parses_structured_output() {
    let scenario = scenario("round_trip", Vec::new(), None, 3);
    let prompt = SimulatorPrompt::render(&scenario, &[])
        .expect("prompt")
        .prompt;
    let agent = Arc::new(Agent::new(prompt));
    let providers =
        registry_returning_texts(&[r#"{"message":"More please","goal_achieved":false}"#]);
    let simulator = ServerSimulatedUser::new(agent, providers);

    let turn = simulator
        .next_turn(&scenario, &[])
        .await
        .expect("simulator succeeds");

    assert_eq!(turn.message, "More please");
    assert!(!turn.goal_achieved);
}

#[tokio::test(flavor = "multi_thread")]
async fn server_simulated_user_fails_after_malformed_retries() {
    let scenario = scenario("malformed", Vec::new(), None, 3);
    let prompt = SimulatorPrompt::render(&scenario, &[])
        .expect("prompt")
        .prompt;
    let agent = Arc::new(Agent::new(prompt));
    let providers = registry_returning_texts(&["not json", "also not json"]);
    let simulator = ServerSimulatedUser::new(agent, providers);

    let error = simulator
        .next_turn(&scenario, &[])
        .await
        .expect_err("malformed outputs exhaust retries");

    assert!(matches!(
        error,
        OrchestratorError::SimulatorFailed { attempts: 2, .. }
    ));
}
