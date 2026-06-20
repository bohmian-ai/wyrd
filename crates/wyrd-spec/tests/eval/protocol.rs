use wyrd_semver::VersionBlock;
use wyrd_spec::envelope::CardKind;
use wyrd_spec::ids::{CardName, SpaceName};
use wyrd_spec::reference::CardRef;
use wyrd_spec::vala::eval::ids::ScenarioId;
use wyrd_spec::vala::eval::protocol::{
    AgentTurnSubmission, ConversationTurn, EvalRunOpenRequest, EvalRunOpenResponse,
    MAX_HISTORY_TURNS, SimulatedUserMode, SimulatedUserTurn, TurnDirective, TurnRole,
    UserTurnSubmission,
};
use wyrd_spec::vala::ids::{LeaseToken, RunId};

fn eval_ref() -> CardRef {
    CardRef {
        kind: CardKind::Eval,
        name: CardName::new("rubric").unwrap(),
        version: VersionBlock::parse("1.0.0").unwrap(),
        space: SpaceName::new("default").unwrap(),
        uid: None,
    }
}

#[test]
fn open_request_round_trips() {
    let req = EvalRunOpenRequest {
        eval_ref: eval_ref(),
        simulated_user: SimulatedUserMode::Server,
    };
    let json = serde_json::to_string(&req).unwrap();
    let back: EvalRunOpenRequest = serde_json::from_str(&json).unwrap();
    assert_eq!(req, back);
}

#[test]
fn open_response_round_trips() {
    let resp = EvalRunOpenResponse {
        run_id: RunId::from_string("00000000-0000-7000-8000-000000000000".to_string()),
        lease_token: LeaseToken::new("abc-123").unwrap(),
    };
    let json = serde_json::to_string(&resp).unwrap();
    let back: EvalRunOpenResponse = serde_json::from_str(&json).unwrap();
    assert_eq!(resp, back);
}

#[test]
fn open_response_wire_field_is_run_id() {
    let resp = EvalRunOpenResponse {
        run_id: RunId::from_string("00000000-0000-7000-8000-000000000000".to_string()),
        lease_token: LeaseToken::new("abc-123").unwrap(),
    };
    let v = serde_json::to_value(&resp).unwrap();
    assert!(v.get("run_id").is_some());
    let forbidden_field = ["eval", "run_id"].join("_");
    assert!(v.get(&forbidden_field).is_none());
    assert!(v.get("lease_token").is_some());
}

#[test]
fn lease_token_rejects_empty_long_and_control() {
    assert!(LeaseToken::new("").is_err());
    assert!(LeaseToken::new("a".repeat(257)).is_err());
    assert!(LeaseToken::new("a\x00b").is_err());
}

#[test]
fn simulated_user_mode_serialises_snake_case() {
    let s = serde_json::to_string(&SimulatedUserMode::Client).unwrap();
    assert_eq!(s, "\"client\"");
    let s = serde_json::to_string(&SimulatedUserMode::Server).unwrap();
    assert_eq!(s, "\"server\"");
}

#[test]
fn turn_role_serialises_snake_case() {
    let s = serde_json::to_string(&TurnRole::Agent).unwrap();
    assert_eq!(s, "\"agent\"");
    let s = serde_json::to_string(&TurnRole::User).unwrap();
    assert_eq!(s, "\"user\"");
}

#[test]
fn directive_agent_turn_wire_shape() {
    let directive = TurnDirective::AgentTurn {
        scenario_id: ScenarioId::new("happy_path").unwrap(),
        turn: 0,
        message: "Hi".to_string(),
        history: vec![],
    };
    let json = serde_json::to_value(&directive).unwrap();
    assert_eq!(json["kind"], "agent_turn");
    assert_eq!(json["turn"], 0);
}

#[test]
fn directive_user_turn_needed_wire_shape() {
    let directive = TurnDirective::UserTurnNeeded {
        scenario_id: ScenarioId::new("happy_path").unwrap(),
        turn: 1,
        history: vec![ConversationTurn {
            role: TurnRole::Agent,
            content: "Sure".to_string(),
        }],
    };
    let json = serde_json::to_value(&directive).unwrap();
    assert_eq!(json["kind"], "user_turn_needed");
    assert_eq!(json["history"][0]["role"], "agent");
}

#[test]
fn directive_scenario_complete_wire_shape() {
    let directive = TurnDirective::ScenarioComplete {
        scenario_id: ScenarioId::new("happy_path").unwrap(),
    };
    let json = serde_json::to_value(&directive).unwrap();
    assert_eq!(json["kind"], "scenario_complete");
}

#[test]
fn directive_run_complete_wire_shape() {
    let directive = TurnDirective::RunComplete;
    let json = serde_json::to_value(&directive).unwrap();
    assert_eq!(json, serde_json::json!({"kind": "run_complete"}));
}

#[test]
fn directive_round_trips_for_each_variant() {
    let cases = vec![
        TurnDirective::AgentTurn {
            scenario_id: ScenarioId::new("a").unwrap(),
            turn: 0,
            message: "m".to_string(),
            history: vec![],
        },
        TurnDirective::UserTurnNeeded {
            scenario_id: ScenarioId::new("a").unwrap(),
            turn: 1,
            history: vec![],
        },
        TurnDirective::ScenarioComplete {
            scenario_id: ScenarioId::new("a").unwrap(),
        },
        TurnDirective::RunComplete,
    ];
    for directive in cases {
        let json = serde_json::to_string(&directive).unwrap();
        let back: TurnDirective = serde_json::from_str(&json).unwrap();
        assert_eq!(directive, back);
    }
}

#[test]
fn directive_validate_rejects_too_long_history() {
    let directive = TurnDirective::AgentTurn {
        scenario_id: ScenarioId::new("a").unwrap(),
        turn: 0,
        message: "m".to_string(),
        history: (0..MAX_HISTORY_TURNS + 1)
            .map(|_| ConversationTurn {
                role: TurnRole::User,
                content: "x".to_string(),
            })
            .collect(),
    };
    let err = directive.validate().unwrap_err();
    assert!(err.to_string().contains("MAX_HISTORY_TURNS"));
}

#[test]
fn agent_turn_submission_round_trips() {
    let submission = AgentTurnSubmission {
        scenario_id: ScenarioId::new("a").unwrap(),
        turn: 0,
        response: "ok".to_string(),
        records: vec![],
    };
    let json = serde_json::to_string(&submission).unwrap();
    let back: AgentTurnSubmission = serde_json::from_str(&json).unwrap();
    assert_eq!(submission, back);
}

#[test]
fn user_turn_submission_round_trips() {
    let submission = UserTurnSubmission {
        scenario_id: ScenarioId::new("a").unwrap(),
        turn: 1,
        message: "and another thing".to_string(),
    };
    let json = serde_json::to_string(&submission).unwrap();
    let back: UserTurnSubmission = serde_json::from_str(&json).unwrap();
    assert_eq!(submission, back);
}

#[test]
fn simulated_user_turn_round_trips() {
    let turn = SimulatedUserTurn {
        message: "ok".to_string(),
        goal_achieved: true,
    };
    let json = serde_json::to_string(&turn).unwrap();
    let back: SimulatedUserTurn = serde_json::from_str(&json).unwrap();
    assert_eq!(turn, back);
}

#[test]
fn deny_unknown_fields_on_submissions() {
    let payload = serde_json::json!({
        "scenario_id": "a",
        "turn": 0,
        "response": "ok",
        "records": [],
        "rogue": true
    });
    let err = serde_json::from_value::<AgentTurnSubmission>(payload).unwrap_err();
    assert!(err.to_string().contains("unknown field"));
}
