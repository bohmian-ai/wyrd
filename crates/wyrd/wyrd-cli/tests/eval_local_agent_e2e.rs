mod eval_support;

use assert_cmd::prelude::*;
use predicates::prelude::*;
use serde_json::json;
use wiremock::matchers::method;
use wiremock::{Mock, MockServer, ResponseTemplate};

use eval_support::{scenario_with, spec, write_eval_card, write_scenarios, write_scenarios_many};

#[tokio::test]
async fn local_agent_run_writes_summary() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let eval_path = tmp.path().join("eval.json");
    let scenarios_path = tmp.path().join("scenarios.json");
    let out_dir = tmp.path().join("out");
    write_eval_card(&eval_path, spec(Vec::new(), None));
    write_scenarios(&scenarios_path);

    let agent = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "response": "DONE",
            "records": []
        })))
        .mount(&agent)
        .await;

    std::process::Command::cargo_bin("wyrd")
        .expect("wyrd binary")
        .args([
            "eval",
            "run",
            "--eval",
            eval_path.to_str().expect("utf8 path"),
            "--scenarios",
            scenarios_path.to_str().expect("utf8 path"),
            "--agent-url",
            &agent.uri(),
            "--judge-mock",
            "--out",
            out_dir.to_str().expect("utf8 path"),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("pass_gate"));

    assert!(out_dir.join("results.json").exists());
}

#[tokio::test]
async fn local_agent_scripted_user_selects_message_by_scenario_id_and_turn() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let eval_path = tmp.path().join("eval.json");
    let scenarios_path = tmp.path().join("scenarios.json");
    let script_path = tmp.path().join("script.jsonl");
    let out_dir = tmp.path().join("out");
    write_eval_card(&eval_path, spec(Vec::new(), None));
    write_scenarios_many(
        &scenarios_path,
        vec![
            scenario_with("scenario_a", 2),
            scenario_with("scenario_b", 2),
        ],
    );
    std::fs::write(
        &script_path,
        [
            r#"{"scenario_id":"scenario_a","turn":1,"message":"user message for A"}"#,
            r#"{"scenario_id":"scenario_b","turn":1,"message":"user message for B"}"#,
            "",
        ]
        .join("\n"),
    )
    .expect("script fixture writes");

    let agent = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "response": "ack",
            "records": []
        })))
        .mount(&agent)
        .await;

    std::process::Command::cargo_bin("wyrd")
        .expect("wyrd binary")
        .args([
            "eval",
            "run",
            "--eval",
            eval_path.to_str().expect("utf8 path"),
            "--scenarios",
            scenarios_path.to_str().expect("utf8 path"),
            "--agent-url",
            &agent.uri(),
            "--simulated-user",
            "client",
            "--simulated-user-script",
            script_path.to_str().expect("utf8 path"),
            "--judge-mock",
            "--out",
            out_dir.to_str().expect("utf8 path"),
        ])
        .assert()
        .success();

    let requests = agent.received_requests().await.expect("requests captured");
    let messages = requests
        .iter()
        .map(|request| {
            let body: serde_json::Value =
                serde_json::from_slice(&request.body).expect("agent request body is json");
            body["message"]
                .as_str()
                .expect("message is string")
                .to_owned()
        })
        .collect::<Vec<_>>();
    assert!(messages.contains(&"user message for A".to_owned()));
    assert!(messages.contains(&"user message for B".to_owned()));
}
