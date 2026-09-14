//! Real-server journey for the compiled `wyrd query` command.

use assert_cmd::prelude::*;
use secrecy::SecretString;
use serde_json::Value;
use std::process::Command;
use std::sync::Arc;
use wyrd_client::auth::AuthMiddleware;
use wyrd_client::config::ClientConfig;
use wyrd_client::transport::{HttpConfig, HttpTransport, ResolvedCredential};
use wyrd_client::{Bifrost, WyrdClient};
use wyrd_spec::error::WyrdError;
use wyrd_spec::vala::api::{BifrostQueryRequest, FreshnessPolicy, VisibilityMode};
use wyrd_testing::WyrdTestServer;
use wyrd_testing::bifrost::seed_query_fixture;

/// Obtains the typed SDK error produced by the same endpoint, token, and SQL as the CLI.
///
/// # Errors
///
/// Returns client construction failures or the unexpected successful query stream.
async fn typed_query_error(
    endpoint: &str,
    token: &str,
    sql: &str,
) -> Result<WyrdError, Box<dyn std::error::Error>> {
    let config = ClientConfig {
        http: HttpConfig {
            base_url: endpoint.to_owned(),
            ..HttpConfig::default()
        },
        ..ClientConfig::default()
    };
    let auth = AuthMiddleware::new(
        &config,
        ResolvedCredential::BearerToken(SecretString::from(token.to_owned())),
    )?;
    let http = HttpTransport::new(&config.http, Arc::clone(&auth))?;
    let client = WyrdClient::from_parts(auth, http, config.grpc);
    let request = BifrostQueryRequest {
        sql: sql.to_owned(),
        visibility: VisibilityMode::PublishedOnly,
        freshness: FreshnessPolicy::Strict,
        deadline_ms: None,
    };
    match Bifrost::query_only(&client).query(&request).await {
        Err(error) => Ok(WyrdError::from(error)),
        Ok(_) => Err("query unexpectedly succeeded".into()),
    }
}

/// Asserts global CLI stderr exactly projects one typed SDK error.
fn assert_cli_problem(problem: &Value, expected: &WyrdError) {
    let canonical = expected.as_problem_json();
    assert_eq!(problem["kind"], "wyrd_cli_error");
    assert_eq!(problem["code"], expected.code());
    assert_eq!(problem["status"], expected.status());
    assert_eq!(problem["title"], expected.title());
    assert_eq!(problem["message"], canonical["detail"]);
    assert_eq!(problem["remediation"], expected.remediation());
    assert_eq!(
        problem["details"],
        canonical.get("details").cloned().unwrap_or(Value::Null)
    );
}

/// The compiled CLI reads two seeded rows and reports a successful terminal.
///
/// This journey requires the serialized Postgres-backed CLI lane; the default
/// Wyrd test lane intentionally remains database-free.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the serialized Postgres-backed CLI journey lane"]
async fn query_command_reads_seeded_table() {
    let server = WyrdTestServer::start_bound()
        .await
        .expect("test server starts");
    let fixture = seed_query_fixture(&server, "cli-success")
        .await
        .expect("query fixture seeds");
    let sql = format!("SELECT id, value FROM {}", fixture.table);
    let output = Command::cargo_bin("wyrd")
        .expect("wyrd binary")
        .args([
            "query",
            "--server",
            &fixture.endpoint,
            "--token",
            &fixture.token,
            "--sql",
            &sql,
            "--format",
            "jsonl",
        ])
        .output()
        .expect("compiled query command runs");
    assert!(
        output.status.success(),
        "query command failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let rows: Vec<Value> = String::from_utf8(output.stdout)
        .expect("stdout is UTF-8")
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).expect("CLI row is JSON"))
        .collect();
    assert_eq!(rows, fixture.expected_rows);

    let terminal: Value = String::from_utf8(output.stderr)
        .expect("stderr is UTF-8")
        .lines()
        .rfind(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).expect("CLI terminal is JSON"))
        .expect("CLI emits terminal JSON");
    assert_eq!(terminal["outcome"], "success");
    assert_eq!(terminal["freshness"], "complete");
    assert_eq!(terminal["row_count"], 2);
    assert_eq!(terminal["error"], Value::Null);
    server.shutdown().await.expect("test server shuts down");
}

/// The compiled CLI preserves a denied query's originating problem and does no Oracle work.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the serialized Postgres-backed CLI journey lane"]
async fn query_command_denial_preserves_problem_without_read_decision() {
    let server = WyrdTestServer::start_bound()
        .await
        .expect("test server starts");
    let fixture = seed_query_fixture(&server, "cli-denied")
        .await
        .expect("query fixture seeds");
    let denied = server.query_denied_token().await.expect("denied token");
    let before = server
        .bifrost_read_decision_count()
        .await
        .expect("initial decision count");
    let sql = format!("SELECT * FROM {}", fixture.table);
    let denied_expected = typed_query_error(&fixture.endpoint, &denied, &sql)
        .await
        .expect("typed denied query fails");
    assert_eq!(denied_expected.code(), "WYRD_PERMISSION_403_DENIED_RBAC");
    assert_eq!(denied_expected.status(), 403);
    let output = Command::cargo_bin("wyrd")
        .expect("wyrd binary")
        .args([
            "query",
            "--server",
            &fixture.endpoint,
            "--token",
            &denied,
            "--sql",
            &sql,
        ])
        .output()
        .expect("compiled query command runs");
    assert!(!output.status.success());
    let problem: Value = String::from_utf8(output.stderr)
        .expect("stderr is UTF-8")
        .lines()
        .find_map(|line| serde_json::from_str(line).ok())
        .expect("CLI emits structured problem JSON");
    assert_cli_problem(&problem, &denied_expected);
    assert_eq!(
        server
            .bifrost_read_decision_count()
            .await
            .expect("final decision count"),
        before
    );

    let invalid_sql = "DELETE FROM forbidden";
    let invalid_expected = typed_query_error(&fixture.endpoint, &fixture.token, invalid_sql)
        .await
        .expect("typed invalid query fails");
    assert_eq!(invalid_expected.code(), "WYRD_VALA_400_QUERY_INVALID_SQL");
    assert_eq!(invalid_expected.status(), 400);
    let invalid = Command::cargo_bin("wyrd")
        .expect("wyrd binary")
        .args([
            "query",
            "--server",
            &fixture.endpoint,
            "--token",
            &fixture.token,
            "--sql",
            invalid_sql,
        ])
        .output()
        .expect("compiled invalid query runs");
    assert!(!invalid.status.success());
    let invalid_problem: Value = String::from_utf8(invalid.stderr)
        .expect("invalid stderr is UTF-8")
        .lines()
        .find_map(|line| serde_json::from_str(line).ok())
        .expect("invalid query emits structured problem JSON");
    assert_cli_problem(&invalid_problem, &invalid_expected);
    assert_eq!(
        server
            .bifrost_read_decision_count()
            .await
            .expect("post-invalid decision count"),
        before
    );
    server.shutdown().await.expect("test server shuts down");
}
