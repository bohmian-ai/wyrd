//! The README support agent Rust program, run as a reader runs it.
//!
//! The journey runs the built `support_agent` binary from an empty directory
//! against a real server. One local model plays both the developer's support
//! agent, reached through `OPENAI_BASE_URL`, and the `policy-judge` behind
//! the server's `openai` deployment: it fails an answer promising a
//! guaranteed refund and passes every other.

use std::process::Command;

use secrecy::ExposeSecret;
use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};
use wyrd_testing::server::WyrdTestServer;

/// The support agent's system prompt, which the local model answers with
/// [`SUPPORT_ANSWER`].
const SUPPORT_AGENT: &str = "You are a support agent.";

/// The local model's reply as the support agent.
const SUPPORT_ANSWER: &str =
    "Refunds are possible within 30 days of purchase, so this order no longer qualifies.";

/// The judge's prompt, which the local model answers with a verdict.
const SUPPORT_JUDGE: &str = "Does this support answer follow the policy?";

/// What the program prints, as the README shows it.
const OUTPUT: &str = include_str!("../support-agent/output.txt");

/// The local model's reply to `request`: the support answer for the agent,
/// a verdict citing the answer for the judge.
fn answer(request: &Request) -> ResponseTemplate {
    let text = String::from_utf8_lossy(&request.body);
    let content = if text.contains(SUPPORT_AGENT) {
        SUPPORT_ANSWER.to_owned()
    } else if text.contains(SUPPORT_JUDGE) {
        let broken = text.contains("guaranteed refund");
        json!({
            "outcome": if broken { "failed" } else { "passed" },
            "reason": if broken {
                "the answer promises a refund the policy does not allow"
            } else {
                "the answer follows the 30-day policy"
            },
            "citations": [{ "slot": "answer", "locator": { "json_pointer": "" } }],
        })
        .to_string()
    } else {
        return ResponseTemplate::new(400);
    };
    ResponseTemplate::new(200).set_body_json(json!({
        "id": "chatcmpl-support",
        "object": "chat.completion",
        "created": 1,
        "model": "gpt-4o-mini",
        "choices": [{
            "index": 0,
            "finish_reason": "stop",
            "message": { "role": "assistant", "content": content },
        }],
        "usage": { "prompt_tokens": 1, "completion_tokens": 1, "total_tokens": 2 },
    }))
}

/// The program prints the agent's passing reply and the failing reply with
/// both requirements' reasons; the agent answers once and the judge reads
/// each reply with its question; and an unchanged rerun prints the same.
///
/// # Panics
/// Panics when the server cannot start, the program fails, or it prints
/// anything else.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn support_agent_verifies_the_agent_reply() {
    let model = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(answer)
        .mount(&model)
        .await;
    let server = WyrdTestServer::builder()
        .with_verification_runtime_for_test()
        .with_gateway_provider_root_for_test(model.uri().parse().expect("model URL parses"))
        .start_bound()
        .await
        .expect("test server starts");
    server
        .deploy_gateway_model_for_test("gpt-4o-mini", "policy_judge_invoker")
        .await
        .expect("the judge model deploys");
    let dir = tempfile::tempdir().expect("reader directory creates");
    let config = tempfile::tempdir().expect("config directory creates");
    let server_url = server
        .base_url()
        .expect("bound server has a URL")
        .to_owned();
    let grpc_url = server.grpc_url().expect("bound server has a gRPC URL");
    let key = server
        .tenant_admin_key()
        .await
        .expect("administrator key issues")
        .expose_secret()
        .to_owned();
    let openai = format!("{}/v1", model.uri());

    // The in-process server keeps serving while the binary runs.
    let (first, again) = tokio::task::spawn_blocking(move || {
        let run = || {
            let output = Command::new(env!("CARGO_BIN_EXE_support_agent"))
                .current_dir(dir.path())
                .env("WYRD_CONFIG_HOME", config.path())
                .env("WYRD_SERVER_URL", &server_url)
                .env("WYRD_GRPC_URL", &grpc_url)
                .env("WYRD_API_KEY", &key)
                .env("OPENAI_BASE_URL", &openai)
                .env("OPENAI_API_KEY", "sk-local-upstream")
                .env_remove("WYRD_ACCESS_TOKEN")
                .env_remove("WYRD_WORKLOAD_TOKEN")
                .env_remove("WYRD_TENANT")
                .output()
                .expect("the support_agent binary starts");
            assert!(
                output.status.success(),
                "the program fails: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            String::from_utf8(output.stdout).expect("the program prints UTF-8")
        };
        (run(), run())
    })
    .await
    .expect("the program task joins");
    assert_eq!(first, OUTPUT);
    assert_eq!(
        again, OUTPUT,
        "a rerun declares nothing new and judges the same"
    );

    let calls: Vec<String> = model
        .received_requests()
        .await
        .expect("the model records requests")
        .iter()
        .map(|request| String::from_utf8_lossy(&request.body).into_owned())
        .collect();
    let asked = calls
        .iter()
        .filter(|call| call.contains(SUPPORT_AGENT))
        .count();
    let judged: Vec<&String> = calls
        .iter()
        .filter(|call| call.contains(SUPPORT_JUDGE))
        .collect();
    assert_eq!(asked, 2, "the agent answers once per run");
    assert_eq!(judged.len(), 4, "the judge reads each reply");
    assert!(
        judged.iter().all(|call| call.contains("45 days ago")),
        "the judge sees the question"
    );
    server.shutdown().await.expect("test server shuts down");
}
