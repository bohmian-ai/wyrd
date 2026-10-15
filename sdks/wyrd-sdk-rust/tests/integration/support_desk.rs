//! The canonical support desk, run from the checked-in example against a
//! real server: deploy the declared table and Agent, judge one answer
//! immediately, answer 100 requests in correlated Agent Runs that each submit
//! a background policy check, resume every check by its execution ID, and
//! explain one passing and one failing request through MCP.

use std::collections::BTreeSet;
use std::time::Duration;
use wyrd_ai::bifrost::BifrostOptions;

use serde_json::json;
use wiremock::matchers::{body_string_contains, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};
use wyrd_ai::gateway::{
    DeploymentAccess, GatewayOperation, ProviderAdapter, ProviderAuth, ProviderDeployment,
    ProviderDeploymentName,
};
use wyrd_ai::state::WyrdState;
use wyrd_ai::{Bifrost, Gateway, RequirementOutcome, VerificationVerdict};
use wyrd_testing::server::WyrdTestServer;

use crate::local_development::configure_gateway;
use crate::support::{Deployment, fixture};

/// The checked-in example under test.
#[path = "../../../../examples/support-desk/rust/support_desk.rs"]
mod example;

use example::{REQUESTS, check, deploy, explain, question, serve, wait_for_verdicts};

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

/// The support desk refuses to deploy while only another provider's `gpt-4o`
/// is deployed, then deploys, refuses a conflicting table and an
/// under-privileged caller, fails a refund promise immediately, answers every
/// request, resumes each background `policy-check` by ID to 90 passes and 10
/// refund-promise failures that the judge, citing the policy, still passes,
/// and explains a passing and a failing request from joined evidence.
///
/// # Panics
/// Panics when a step fails or any count, refusal, or joined value differs.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn support_desk_answers_verifies_and_explains_every_request() {
    let upstream = MockServer::start().await;
    answer(
        &upstream,
        "list its refund rules",
        r#"{"findings":[{"rule":"Refunds of up to 100 dollars are approved within 30 days.","page":1}]}"#,
        1,
    )
    .await;
    answer(
        &upstream,
        "Claim:",
        r#"{"outcome":"passed","reason":"consistent with the policy","citations":[{"slot":"policy","locator":{"page":1}}]}"#,
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
    let bundle = tempfile::tempdir().expect("bundle directory creates");
    Gateway::connect_with(&admin)
        .await
        .expect("handle connects")
        .put_deployment(&ProviderDeployment {
            name: ProviderDeploymentName::new("anthropic").expect("deployment name"),
            adapter: ProviderAdapter::Anthropic,
            auth: ProviderAuth::None,
            models: None,
            capabilities: Some(BTreeSet::from([GatewayOperation::ChatCompletions])),
            access: DeploymentAccess::Open,
        })
        .await
        .expect("the foreign deployment puts");
    let Err(missing) = deploy(&admin, &bundle.path().join("refused")).await else {
        panic!("a deployment with another name does not serve the Agent");
    };
    let missing = missing
        .downcast_ref::<wyrd_ai::WyrdError>()
        .expect("the refusal is a Wyrd error");
    assert_eq!(missing.code(), "WYRD_GATEWAY_404_MODEL_UNAVAILABLE");
    assert!(
        missing
            .to_string()
            .contains("configure the gateway deployment openai"),
        "{missing}"
    );
    configure_gateway(&admin).await;
    let bundle = bundle.path().join("bundle");

    let desk = deploy(&admin, &bundle).await.expect("the desk deploys");

    let tickets = Bifrost::connect_with(&admin, BifrostOptions::default())
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
        .await
        .register_from_path(&fixture("cards/support_desk/conflicting-desk.yaml"))
        .await
        .expect_err("a conflicting tickets schema is refused");
    assert_eq!(
        conflict.code(),
        "WYRD_VALA_409_BIFROST_FINGERPRINT_MISMATCH",
        "{conflict}"
    );
    let viewer = deployment.client(&deployment.key("desk_viewer", &["viewer"]).await);
    let denied = WyrdState::from_path_with(&bundle, viewer)
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

    let promise = check(&desk, "You have a guaranteed refund.")
        .await
        .expect("the answer is judged now");
    assert_eq!(promise.outcome.verdict, VerificationVerdict::Failed);
    let outcomes: Vec<(&str, RequirementOutcome)> = promise
        .outcome
        .requirements
        .iter()
        .map(|requirement| (requirement.id.as_str(), requirement.outcome))
        .collect();
    assert_eq!(
        outcomes,
        [
            ("no-refund-promise", RequirementOutcome::Failed),
            ("follows-policy", RequirementOutcome::Passed),
        ]
    );
    let judged = promise
        .outcome
        .requirements
        .iter()
        .find(|requirement| requirement.id.as_str() == "follows-policy")
        .expect("the judge requirement is reported");
    assert_eq!(
        serde_json::to_value(&judged.citations).expect("citations serialize"),
        json!([{ "slot": "policy", "locator": { "page": 1 } }])
    );

    let served = serve(&desk).await.expect("every request is served");
    assert_eq!(served.len(), REQUESTS);
    let results = wait_for_verdicts(&admin, &served, WAIT)
        .await
        .expect("every answer is judged");
    for (index, result) in results.iter().enumerate() {
        let expected = if question(index).contains("refund") {
            VerificationVerdict::Failed
        } else {
            VerificationVerdict::Passed
        };
        assert_eq!(result.outcome.verdict, expected, "{index}");
        assert_eq!(
            result.run_id.as_ref().map(ToString::to_string).as_deref(),
            Some(served[index].run_id.as_str()),
            "{index}"
        );
    }

    let agent = desk.uid("support-agent");
    for (index, verdict) in [(1, "passed"), (0, "failed")] {
        let explained = explain(&admin, &served[index].run_id)
            .await
            .expect("the Run's evidence joins");
        assert_eq!(explained["run_id"], served[index].run_id.as_str());
        assert_eq!(explained["ticket_id"], format!("T-{index}").as_str());
        assert_eq!(explained["span"], "support-desk.request");
        assert_eq!(explained["call_card_uid"], agent.as_str());
        assert_eq!(explained["verdict"], verdict, "{explained:?}");
    }
    deployment.shutdown().await;
}
