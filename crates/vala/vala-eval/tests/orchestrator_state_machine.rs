mod orchestrator_support;

use orchestrator_support::{eval_ref, record, scenario, sid};
use vala_eval::orchestrator::{OrchestratorError, RunState};
use wyrd_spec::vala::eval::protocol::{
    AgentTurnSubmission, SimulatedUserMode, TurnDirective, UserTurnSubmission,
};

fn agent_submission(scenario_id: &str, turn: u32, response: &str) -> AgentTurnSubmission {
    AgentTurnSubmission {
        scenario_id: sid(scenario_id),
        turn,
        response: response.to_owned(),
        records: Vec::new(),
    }
}

#[test]
fn scripted_walk_full() {
    let mut state = RunState::open(
        eval_ref(),
        SimulatedUserMode::Client,
        vec![scenario("scripted", vec!["Again"], None, 2)],
    );

    let TurnDirective::AgentTurn { turn, message, .. } = state.next().expect("next").0 else {
        panic!("first directive must be agent turn");
    };
    assert_eq!(turn, 0);
    assert_eq!(message, "Start");
    state
        .submit_agent_turn(agent_submission("scripted", 0, "First"))
        .expect("submit first");

    let TurnDirective::AgentTurn { turn, message, .. } = state.next().expect("next").0 else {
        panic!("second directive must be agent turn");
    };
    assert_eq!(turn, 1);
    assert_eq!(message, "Again");
    state
        .submit_agent_turn(agent_submission("scripted", 1, "Second"))
        .expect("submit second");

    assert!(matches!(
        state.next().expect("complete").0,
        TurnDirective::ScenarioComplete { .. }
    ));
    state.ack_scenario_complete();
    assert!(matches!(
        state.next().expect("run complete").0,
        TurnDirective::RunComplete
    ));
}

#[test]
fn server_simulated_walk_uses_user_turn_needed_internally() {
    let mut state = RunState::open(
        eval_ref(),
        SimulatedUserMode::Server,
        vec![scenario("server_sim", Vec::new(), None, 2)],
    );

    let TurnDirective::AgentTurn { turn, .. } = state.next().expect("next").0 else {
        panic!("first directive must be agent turn");
    };
    state
        .submit_agent_turn(agent_submission("server_sim", turn, "Need more"))
        .expect("submit agent");

    let TurnDirective::UserTurnNeeded { turn, .. } = state.next().expect("next").0 else {
        panic!("server mode asks the caller to simulate internally");
    };
    assert!(state.wants_server_simulated_turn());
    state
        .submit_simulated_user(
            sid("server_sim"),
            turn,
            "Here is more context".to_owned(),
            false,
        )
        .expect("submit simulated user");

    let TurnDirective::AgentTurn { turn, message, .. } = state.next().expect("next").0 else {
        panic!("simulated user should advance to agent turn");
    };
    assert_eq!(turn, 1);
    assert_eq!(message, "Here is more context");
}

#[test]
fn client_delegated_walk_returns_user_turn_needed() {
    let mut state = RunState::open(
        eval_ref(),
        SimulatedUserMode::Client,
        vec![scenario("client_sim", Vec::new(), None, 2)],
    );

    state.next().expect("first directive");
    state
        .submit_agent_turn(agent_submission("client_sim", 0, "Question"))
        .expect("submit agent");

    let TurnDirective::UserTurnNeeded { turn, .. } = state.next().expect("next").0 else {
        panic!("client mode must return UserTurnNeeded");
    };
    assert!(!state.wants_server_simulated_turn());
    state
        .submit_user_turn(UserTurnSubmission {
            scenario_id: sid("client_sim"),
            turn,
            message: "Answer".to_owned(),
        })
        .expect("submit user");
    assert!(matches!(
        state.next().expect("agent turn").0,
        TurnDirective::AgentTurn { turn: 1, .. }
    ));
}

#[test]
fn termination_by_goal_achieved() {
    let mut state = RunState::open(
        eval_ref(),
        SimulatedUserMode::Server,
        vec![scenario("goal", Vec::new(), None, 4)],
    );
    state.next().expect("first directive");
    state
        .submit_agent_turn(agent_submission("goal", 0, "Working"))
        .expect("submit agent");
    let TurnDirective::UserTurnNeeded { turn, .. } = state.next().expect("next").0 else {
        panic!("needs simulated user");
    };
    state
        .submit_simulated_user(sid("goal"), turn, "Thanks".to_owned(), true)
        .expect("goal achieved");
    assert!(matches!(
        state.next().expect("complete").0,
        TurnDirective::ScenarioComplete { .. }
    ));
}

#[test]
fn termination_by_signal_substring() {
    let mut state = RunState::open(
        eval_ref(),
        SimulatedUserMode::Client,
        vec![scenario("signal", Vec::new(), Some("DONE"), 8)],
    );
    state.next().expect("first directive");
    state
        .submit_agent_turn(agent_submission("signal", 0, "All DONE now"))
        .expect("submit agent");
    assert!(matches!(
        state.next().expect("complete").0,
        TurnDirective::ScenarioComplete { .. }
    ));
}

#[test]
fn termination_by_max_turns() {
    let mut state = RunState::open(
        eval_ref(),
        SimulatedUserMode::Client,
        vec![scenario("max_turns", Vec::new(), None, 1)],
    );
    state.next().expect("first directive");
    state
        .submit_agent_turn(agent_submission("max_turns", 0, "Only turn"))
        .expect("submit agent");
    assert!(matches!(
        state.next().expect("complete").0,
        TurnDirective::ScenarioComplete { .. }
    ));
}

#[test]
fn turn_cursor_is_monotonic() {
    let mut state = RunState::open(
        eval_ref(),
        SimulatedUserMode::Client,
        vec![scenario("cursor", Vec::new(), None, 1)],
    );
    state.next().expect("first directive");
    let error = state
        .submit_agent_turn(agent_submission("cursor", 9, "wrong turn"))
        .expect_err("turn mismatch rejected");
    assert!(matches!(
        error,
        OrchestratorError::TurnMismatch {
            got: 9,
            expected: 0
        }
    ));
}

/// Records captured in one scenario never leak into a sibling scenario.
#[test]
fn cross_scenario_record_isolation() {
    let mut state = RunState::open(
        eval_ref(),
        SimulatedUserMode::Client,
        vec![
            scenario("one", Vec::new(), None, 1),
            scenario("two", Vec::new(), None, 1),
        ],
    );
    state.next().expect("first directive");
    let mut sub = agent_submission("one", 0, "First");
    sub.records.push(record(true));
    state.submit_agent_turn(sub).expect("submit first");
    let TurnDirective::ScenarioComplete { scenario_id } = state.next().expect("complete").0 else {
        panic!("scenario one complete");
    };
    let completed = state
        .take_completed_scenario(&scenario_id)
        .expect("completed cursor");
    assert_eq!(completed.emitted_records.len(), 1);
    state.ack_scenario_complete();

    let TurnDirective::AgentTurn { scenario_id, .. } = state.next().expect("second").0 else {
        panic!("scenario two starts");
    };
    assert_eq!(scenario_id, sid("two"));
    assert_eq!(
        state
            .scenarios
            .front()
            .expect("active second scenario")
            .emitted_records
            .len(),
        0
    );
}
