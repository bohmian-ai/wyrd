//! Real-server CLI journey for the one credential the CLI ever prints.
//!
//! `wyrd auth issue-key` returns an API key plaintext exactly once and never
//! stores it, so nothing downstream can recover it: a renamed response field, a
//! dropped `key:` line, or a key the server will not accept on its next use all
//! ship silently. This journey closes that by running the shipped binary twice —
//! once to issue the key, once to spend it — and proves the second command
//! authenticates on nothing but what the first printed.

use std::time::Duration;

use axum::http::StatusCode;
use tokio::time::timeout;

use crate::principal_journey::{
    machine_token, run_cli_async_with_credential, start_served, stop_served, v1_status,
};

/// Read the once-printed API key out of `auth issue-key` stdout.
///
/// The command prints a labelled block rather than JSON, so the key is recovered
/// by its label. Panics with the whole output when the label is absent, because a
/// missing `key:` line is exactly the regression this journey exists to catch.
fn issued_key(stdout: &str) -> String {
    stdout
        .lines()
        .find_map(|line| line.strip_prefix("key:"))
        .map(|value| value.trim().to_owned())
        .unwrap_or_else(|| panic!("issue-key printed no key: line: {stdout}"))
}

/// An administrator issues a card-bound API key through the CLI, and the key
/// authenticates the next CLI call.
///
/// The second command receives the key in `WYRD_API_KEY` and nothing else: the
/// harness scrubs every other credential source, so it can only succeed by
/// exchanging the plaintext the first command printed. That is what makes this a
/// credential handoff rather than two independent calls — if the key were not
/// reused, or were not accepted, `wyrd list` would exit non-zero.
#[tokio::test]
async fn auth_issue_key_cli_journey() {
    if std::env::var("WYRD_CLI_E2E").as_deref() != Ok("1") {
        return;
    }

    let (server, base_url, shutdown, serve_handle) = start_served("auth issue-key").await;
    let admin = server
        .bootstrap_service("cli-issue-key-admin", &["admin"])
        .await
        .expect("admin bootstraps");
    let admin_token = machine_token(&server, &admin).await;
    let subject = server
        .bootstrap_service("cli-issue-key-subject", &["reader"])
        .await
        .expect("subject bootstraps");
    let card = subject
        .card_ref()
        .expect("a bootstrapped service principal is card-bound")
        .clone();

    let issue = run_cli_async_with_credential(
        vec![
            "auth".to_owned(),
            "issue-key".to_owned(),
            "--kind".to_owned(),
            card.kind.wire_name().to_owned(),
            "--name".to_owned(),
            card.name.to_string(),
            "--version".to_owned(),
            card.version.to_string(),
            "--space".to_owned(),
            card.space
                .as_ref()
                .expect("the fixture card names a space")
                .to_string(),
            "--label".to_owned(),
            "cli journey".to_owned(),
            "--server".to_owned(),
            base_url.clone(),
        ],
        "WYRD_ACCESS_TOKEN",
        admin_token,
    )
    .await;
    assert!(
        issue.status.success(),
        "issue-key failed: stdout={} stderr={}",
        String::from_utf8_lossy(&issue.stdout),
        String::from_utf8_lossy(&issue.stderr)
    );
    let stdout = String::from_utf8_lossy(&issue.stdout);
    let key = issued_key(&stdout);
    assert!(!key.is_empty(), "the printed key is not empty");
    let key_id = stdout
        .lines()
        .find_map(|line| line.strip_prefix("key_id:"))
        .unwrap_or_else(|| panic!("issue-key printed no key_id: line: {stdout}"));
    uuid::Uuid::parse_str(key_id.trim())
        .unwrap_or_else(|error| panic!("the printed key_id is a UUID: {error}: {stdout}"));

    // The key's own principal, reached through the CLI's ambient credential
    // chain: the client exchanges it at /auth/token and carries the resulting
    // access token on the canonical header.
    let list = timeout(
        Duration::from_secs(30),
        run_cli_async_with_credential(
            vec![
                "list".to_owned(),
                "--server".to_owned(),
                base_url.clone(),
                "--format".to_owned(),
                "json".to_owned(),
            ],
            "WYRD_API_KEY",
            key.clone(),
        ),
    )
    .await
    .expect("the second CLI call completes");
    assert!(
        list.status.success(),
        "the issued key did not authenticate the next call: stdout={} stderr={}",
        String::from_utf8_lossy(&list.stdout),
        String::from_utf8_lossy(&list.stderr)
    );

    // The subject's pre-existing token is untouched by issuing a second
    // credential for the same principal.
    let subject_token = machine_token(&server, &subject).await;
    assert_eq!(
        v1_status(&server, &subject_token).await,
        StatusCode::OK,
        "issuing a key left the principal's other credentials working"
    );

    stop_served(server, shutdown, serve_handle).await;
}
