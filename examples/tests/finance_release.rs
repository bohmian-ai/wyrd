//! The finance release Rust program, run as a reader runs it.
//!
//! The journey copies the policy PDF into an empty directory and runs the
//! built `finance_release` binary there against a real server. A local model
//! stands in for the policy judge and approves every release, so only the
//! bank account assertion decides the second release.

use std::path::Path;
use std::process::Command;

use secrecy::ExposeSecret;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};
use wyrd_testing::server::WyrdTestServer;

/// The finance release example's directory.
const EXAMPLE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/finance-release");

/// The policy judge's reply: the release follows the policy, citing the
/// report text.
const APPROVES: &str = r#"{"outcome":"passed","reason":"an external audit may receive this report","citations":[{"slot":"report","locator":{"json_pointer":"/text"}}]}"#;

/// The program prints a passing `release-check` for the permitted release,
/// and a failing one, on its bank account assertion, for the release that
/// names one.
///
/// # Panics
/// Panics when the server cannot start, the program fails, or it prints
/// anything else.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn finance_release_passes_and_fails() {
    let model = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": "chatcmpl-judge",
            "object": "chat.completion",
            "created": 1,
            "model": "gpt-4o",
            "choices": [{
                "index": 0,
                "finish_reason": "stop",
                "message": { "role": "assistant", "content": APPROVES },
            }],
        })))
        .mount(&model)
        .await;
    let server = WyrdTestServer::builder()
        .with_verification_runtime_for_test()
        .with_gateway_provider_root_for_test(model.uri().parse().expect("model URL parses"))
        .start_bound()
        .await
        .expect("test server starts");
    server
        .deploy_gateway_model_for_test("gpt-4o", "policy_judge_invoker")
        .await
        .expect("the judge model deploys");

    let dir = tempfile::tempdir().expect("reader directory creates");
    let config = tempfile::tempdir().expect("config directory creates");
    std::fs::copy(
        Path::new(EXAMPLE).join("company-sharing-policy.pdf"),
        dir.path().join("company-sharing-policy.pdf"),
    )
    .expect("policy copies");
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

    // The in-process server keeps serving while the binary runs.
    let output = tokio::task::spawn_blocking(move || {
        Command::new(env!("CARGO_BIN_EXE_finance_release"))
            .current_dir(dir.path())
            .env("WYRD_CONFIG_HOME", config.path())
            .env("WYRD_SERVER_URL", server_url)
            .env("WYRD_GRPC_URL", grpc_url)
            .env("WYRD_API_KEY", key)
            .env_remove("WYRD_ACCESS_TOKEN")
            .env_remove("WYRD_WORKLOAD_TOKEN")
            .env_remove("WYRD_TENANT")
            .output()
            .expect("the finance_release binary starts")
    })
    .await
    .expect("the program task joins");
    assert!(
        output.status.success(),
        "the program fails: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    assert_eq!(
        String::from_utf8(output.stdout).expect("the program prints UTF-8"),
        "Quarterly total $120.\n\
         \x20 release-check: passed\n\
         Quarterly total $120. Paid from BANK-ACCOUNT-DEMO-001.\n\
         \x20 release-check: failed\n\
         \x20   not passed: The report names no bank account.\n",
    );
    server.shutdown().await.expect("test server shuts down");
}
