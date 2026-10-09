//! The canonical support desk, run from the checked-in example against a
//! real server: deploy the declared table and Agent, answer 100 requests in
//! correlated Agent Runs, wait for continuous and real-time verdicts, and
//! explain one passing and one failing request through MCP.

use std::collections::BTreeMap;
use std::time::Duration;

use serde_json::json;
use wiremock::matchers::{body_string_contains, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};
use wyrd_sdk::Bifrost;
use wyrd_sdk::state::WyrdState;
use wyrd_testing::server::WyrdTestServer;

use crate::local_development::configure_gateway;
use crate::support::{Deployment, fixture};

/// The checked-in example under test.
#[path = "../../../../examples/support-desk/rust/support_desk.rs"]
mod example;

use example::{REQUESTS, deploy, explain, question, serve, wait_for_verdicts};

/// How long the story waits for every verdict.
const WAIT: Duration = Duration::from_mins(3);

/// Mount one Chat Completions answer `content` for requests whose body
/// contains `marker`, ranked by `priority` (lower wins).
async fn answer(upstream: &MockServer, marker: &str, content: &str, priority: u8) {
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .and(body_string_contains(marker))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "chatcmpl-desk",
            "object": "chat.completion",
            "created": 1,
            "model": "gpt-4o",
            "choices": [{
                "index": 0,
                "message": { "role": "assistant", "content": content },
                "finish_reason": "stop",
            }],
            "usage": { "prompt_tokens": 1, "completion_tokens": 1, "total_tokens": 2 },
        })))
        .with_priority(priority)
        .mount(upstream)
        .await;
}

/// The support desk deploys, refuses a conflicting table and an
/// under-privileged caller, answers every request, earns 100 `answer-quality`
/// passes and 90 `no-refund-promise` passes with 10 failures, and explains a
/// passing and a failing request from joined evidence.
///
/// # Panics
/// Panics when a step fails or any count, refusal, or joined value differs.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn support_desk_answers_verifies_and_explains_every_request() {
    let upstream = MockServer::start().await;
    answer(
        &upstream,
        "Grade the support answer",
        r#"{"passed":true}"#,
        1,
    )
    .await;
    answer(&upstream, "refund", "You have a guaranteed refund.", 2).await;
    answer(
        &upstream,
        "Answer the customer",
        "Your order is on its way.",
        3,
    )
    .await;
    let deployment = Deployment::start_with(
        WyrdTestServer::builder()
            .without_process_telemetry_for_test()
            .with_verification_runtime_for_test()
            .with_gateway_provider_root_for_test(
                upstream.uri().parse().expect("upstream URL parses"),
            ),
    )
    .await;
    let key = deployment
        .server()
        .tenant_admin_key()
        .await
        .expect("the setup administrator key");
    let admin = deployment.client(secrecy::ExposeSecret::expose_secret(&key));
    configure_gateway(&admin).await;
    let bundle = tempfile::tempdir().expect("bundle directory creates");
    let bundle = bundle.path().join("bundle");

    let desk = deploy(&admin, &bundle).await.expect("the desk deploys");

    let tickets = Bifrost::connect(&admin)
        .await
        .expect("Bifrost connects")
        .describe_table("vala.datasets", "tickets")
        .await
        .expect("the declared table exists before Bifrost starts");
    let fields: Vec<&str> = tickets
        .user_fields
        .iter()
        .map(|field| field.name.as_str())
        .collect();
    assert_eq!(fields, ["ticket_id", "question", "answer", "refund"]);
    let conflict = deployment
        .cards()
        .register_from_path(&fixture("cards/support_desk/conflicting-desk.yaml"))
        .await
        .expect_err("a conflicting tickets schema is refused");
    assert_eq!(
        conflict.code(),
        "WYRD_VALA_409_BIFROST_FINGERPRINT_MISMATCH",
        "{conflict}"
    );
    let viewer = deployment.client(&deployment.key("desk_viewer", &["viewer"]).await);
    let denied = WyrdState::from_path_with_client(&bundle, viewer)
        .expect("the bundle loads")
        .run_for_card("agent")
        .expect("agent view resolves")
        .invoke(&[("question", "Where is order 1?")])
        .await
        .expect_err("a viewer may not invoke the Agent");
    assert_eq!(denied.code(), "WYRD_PERMISSION_403_DENIED_RBAC", "{denied}");
    assert!(
        upstream
            .received_requests()
            .await
            .expect("request recording is on")
            .is_empty(),
        "the refusal precedes upstream IO"
    );

    let served = serve(&desk).await.expect("every request is served");
    assert_eq!(served.len(), REQUESTS);
    for (index, request) in served.iter().enumerate() {
        assert_eq!(
            request.passed,
            !question(index).contains("refund"),
            "{index}"
        );
    }
    let verdicts = wait_for_verdicts(&admin, &desk, WAIT)
        .await
        .expect("every answer is judged");
    assert_eq!(
        verdicts,
        BTreeMap::from([
            ("answer-quality".to_owned(), (100, 0)),
            ("no-refund-promise".to_owned(), (90, 10)),
        ])
    );

    let agent = desk.uid("support-agent");
    for (index, realtime) in [(1, "passed"), (0, "failed")] {
        let explained = explain(&admin, &served[index].run_id)
            .await
            .expect("the Run's evidence joins");
        assert_eq!(explained["run_id"], served[index].run_id.as_str());
        assert_eq!(explained["ticket_id"], format!("T-{index}").as_str());
        assert_eq!(explained["span"], "support-desk.request");
        assert_eq!(explained["call_card_uid"], agent.as_str());
        assert_eq!(explained["continuous"], "passed");
        assert_eq!(explained["realtime"], realtime, "{explained:?}");
    }
    deployment.shutdown().await;
}
