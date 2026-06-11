use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Request, StatusCode, header},
};
use serde_json::{Value, json};
use tower::ServiceExt;
use vala_eval::orchestrator::OrchestratorError;
use vala_http::eval::{AppState, router};
use wyrd_spec::envelope::CardKind;
use wyrd_spec::ids::{CardName, SpaceName};
use wyrd_spec::reference::CardRef;
use wyrd_spec::vala::eval::{
    EvalScenario, ScenarioId, ScenarioTask,
    protocol::{
        AgentTurnSubmission, EvalRunOpenRequest, EvalRunOpenResponse, SimulatedUserMode,
        TurnDirective, UserTurnSubmission,
    },
};
use wyrd_spec::vala::ids::{LeaseToken, RunId};
use wyrd_spec::version::VersionBlock;

fn card_ref(kind: CardKind, name: &str) -> CardRef {
    CardRef {
        kind,
        name: CardName::new(name).expect("static card name is valid"),
        version: VersionBlock::parse("1.0.0").expect("static version is valid"),
        space: Some(SpaceName::new("tests").expect("static space is valid")),
        uid: None,
    }
}

fn scenario_id() -> ScenarioId {
    ScenarioId::new("happy_path").expect("static scenario id is valid")
}

fn scenario() -> EvalScenario {
    EvalScenario {
        id: scenario_id(),
        initial_query: "Start".to_owned(),
        expected_outcome: Some("Agent says DONE.".to_owned()),
        predefined_turns: vec!["Continue".to_owned()],
        simulated_user_persona: None,
        termination_signal: Some("DONE".to_owned()),
        max_turns: 2,
        tasks: Vec::<ScenarioTask>::new(),
    }
}

fn fixed_token() -> LeaseToken {
    LeaseToken::new("test-lease-abc-123").expect("static lease token is valid")
}

fn app_with_fixed_lease() -> Router {
    let eval_ref = card_ref(CardKind::Eval, "eval-rubric");
    let loader_ref = eval_ref.clone();
    let state = AppState::new(
        Arc::new(move |got| {
            if got == &loader_ref {
                Ok(vec![scenario()])
            } else {
                Err(OrchestratorError::EmbeddedCallback {
                    reason: "unexpected eval_ref".to_owned(),
                })
            }
        }),
        Arc::new(|| Ok(fixed_token())),
    );
    Router::new().nest("/api/v1/eval", router(state))
}

fn app_with_numbered_leases() -> Router {
    let next = Arc::new(AtomicUsize::new(1));
    let state = AppState::new(
        Arc::new(|_| Ok(vec![scenario()])),
        Arc::new(move || {
            let number = next.fetch_add(1, Ordering::Relaxed);
            LeaseToken::new(format!("lease-{number}")).map_err(HttpErrorBridge::from_source)
        }),
    );
    Router::new().nest("/api/v1/eval", router(state))
}

struct HttpErrorBridge;

impl HttpErrorBridge {
    fn from_source(source: wyrd_spec::error::WyrdError) -> vala_http::eval::HttpError {
        vala_http::eval::HttpError::Engine {
            source: OrchestratorError::Invariant {
                reason: source.to_string(),
            },
        }
    }
}

async fn request(
    app: Router,
    method: &str,
    path: &str,
    bearer: Option<&str>,
    body: Value,
) -> (StatusCode, Value) {
    let mut builder = Request::builder().method(method).uri(path);
    if let Some(token) = bearer {
        builder = builder.header(header::AUTHORIZATION, token);
    }
    let req = builder
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body.to_string()))
        .expect("test request builds");
    let response = app.oneshot(req).await.expect("router handles request");
    let status = response.status();
    let bytes = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body bytes read");
    let value = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).expect("response is JSON")
    };
    (status, value)
}

async fn open_run(app: Router) -> EvalRunOpenResponse {
    let req = EvalRunOpenRequest {
        eval_ref: card_ref(CardKind::Eval, "eval-rubric"),
        simulated_user: SimulatedUserMode::Client,
    };
    let (status, body) = request(app, "POST", "/api/v1/eval/runs", None, json!(req)).await;
    assert_eq!(status, StatusCode::OK);
    let keys = body.as_object().expect("open response is object");
    assert!(keys.contains_key("run_id"));
    assert!(keys.contains_key("lease_token"));
    assert!(!keys.contains_key("eval_run_id"));
    assert_eq!(keys.len(), 2);
    serde_json::from_value(body).expect("open response matches wyrd-spec")
}

#[tokio::test]
async fn client_pull_protocol_round_trips_lease_token() {
    let app = app_with_fixed_lease();
    let open = open_run(app.clone()).await;
    assert_eq!(open.lease_token, fixed_token());
    let bearer = format!("Bearer {}", open.lease_token.as_str());
    let base = format!("/api/v1/eval/runs/{}", open.run_id);

    let (status, body) = request(
        app.clone(),
        "POST",
        &format!("{base}/next"),
        Some(&bearer),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let directive: TurnDirective = serde_json::from_value(body).expect("directive parses");
    let TurnDirective::AgentTurn {
        scenario_id, turn, ..
    } = directive
    else {
        panic!("expected first agent turn");
    };
    let agent = AgentTurnSubmission {
        scenario_id,
        turn,
        response: "ack".to_owned(),
        records: Vec::new(),
    };
    let (status, _) = request(
        app.clone(),
        "POST",
        &format!("{base}/agent-turn"),
        Some(&bearer),
        json!(agent),
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED);

    let (status, body) = request(
        app.clone(),
        "POST",
        &format!("{base}/next"),
        Some(&bearer),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let directive: TurnDirective = serde_json::from_value(body).expect("directive parses");
    let TurnDirective::AgentTurn {
        scenario_id, turn, ..
    } = directive
    else {
        panic!("expected second agent turn");
    };
    let agent = AgentTurnSubmission {
        scenario_id,
        turn,
        response: "DONE".to_owned(),
        records: Vec::new(),
    };
    let (status, _) = request(
        app.clone(),
        "POST",
        &format!("{base}/agent-turn"),
        Some(&bearer),
        json!(agent),
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED);

    let (status, body) = request(
        app.clone(),
        "POST",
        &format!("{base}/next"),
        Some(&bearer),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(matches!(
        serde_json::from_value::<TurnDirective>(body).expect("directive parses"),
        TurnDirective::ScenarioComplete { .. }
    ));

    let (status, body) = request(
        app,
        "POST",
        &format!("{base}/next"),
        Some(&bearer),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        serde_json::from_value::<TurnDirective>(body).expect("directive parses"),
        TurnDirective::RunComplete
    );
}

#[tokio::test]
async fn protected_routes_reject_missing_lease() {
    let app = app_with_fixed_lease();
    let open = open_run(app.clone()).await;
    let base = format!("/api/v1/eval/runs/{}", open.run_id);
    let agent = AgentTurnSubmission {
        scenario_id: scenario_id(),
        turn: 0,
        response: "ack".to_owned(),
        records: Vec::new(),
    };
    let user = UserTurnSubmission {
        scenario_id: scenario_id(),
        turn: 1,
        message: "next".to_owned(),
    };

    for (path, body) in [
        (format!("{base}/next"), json!({})),
        (format!("{base}/agent-turn"), json!(agent)),
        (format!("{base}/user-turn"), json!(user)),
    ] {
        let (status, body) = request(app.clone(), "POST", &path, None, body).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
        assert_eq!(body["code"], "WYRD_EVAL_401_MISSING_LEASE");
    }
}

#[tokio::test]
async fn protected_routes_reject_mismatched_lease() {
    let app = app_with_fixed_lease();
    let open = open_run(app.clone()).await;
    let base = format!("/api/v1/eval/runs/{}", open.run_id);
    let agent = AgentTurnSubmission {
        scenario_id: scenario_id(),
        turn: 0,
        response: "ack".to_owned(),
        records: Vec::new(),
    };
    let user = UserTurnSubmission {
        scenario_id: scenario_id(),
        turn: 1,
        message: "next".to_owned(),
    };

    for (path, body) in [
        (format!("{base}/next"), json!({})),
        (format!("{base}/agent-turn"), json!(agent)),
        (format!("{base}/user-turn"), json!(user)),
    ] {
        let (status, body) = request(
            app.clone(),
            "POST",
            &path,
            Some("Bearer not-the-real-lease"),
            body,
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN);
        assert_eq!(body["code"], "WYRD_EVAL_403_INVALID_LEASE");
    }
}

#[tokio::test]
async fn next_rejects_malformed_bearer() {
    let app = app_with_fixed_lease();
    let open = open_run(app.clone()).await;
    let path = format!("/api/v1/eval/runs/{}/next", open.run_id);

    for bearer in ["Basic dGVzdA==", "Bearer "] {
        let (status, body) = request(app.clone(), "POST", &path, Some(bearer), json!({})).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
        assert_eq!(body["code"], "WYRD_EVAL_401_MISSING_LEASE");
    }
}

#[tokio::test]
async fn next_rejects_lease_from_other_run() {
    let app = app_with_numbered_leases();
    let run_a = open_run(app.clone()).await;
    let run_b = open_run(app.clone()).await;
    let path = format!("/api/v1/eval/runs/{}/next", run_a.run_id);
    let bearer = format!("Bearer {}", run_b.lease_token.as_str());

    let (status, body) = request(app, "POST", &path, Some(&bearer), json!({})).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["code"], "WYRD_EVAL_403_INVALID_LEASE");
}

#[tokio::test]
async fn unknown_run_rejects_with_stable_code() {
    let app = app_with_fixed_lease();
    let run_id = RunId::new();
    let path = format!("/api/v1/eval/runs/{run_id}/next");
    let bearer = format!("Bearer {}", fixed_token().as_str());

    let (status, body) = request(app, "POST", &path, Some(&bearer), json!({})).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["code"], "WYRD_EVAL_404_RUN_NOT_FOUND");
}
