//! Real-server journey for the compiled `wyrd query` command.

use assert_cmd::prelude::*;
use serde_json::Value;
use std::process::Command;
use wyrd_testing::WyrdTestServer;
use wyrd_testing::bifrost::seed_query_fixture;

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
