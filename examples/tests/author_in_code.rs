//! The Author in code guide's Rust program, run as a reader runs it.
//!
//! The journey runs the built `author_in_code` binary from an empty directory
//! against a real server, twice, then reads its rows back with the guide's
//! `replies.sql`.

use std::process::Command;

use secrecy::{ExposeSecret, SecretString};
use serde::Deserialize;
use wyrd_ai::bifrost::BifrostOptions;
use wyrd_ai::config::ClientConfig;
use wyrd_ai::transport::{GrpcConfig, HttpConfig};
use wyrd_ai::{Bifrost, WyrdClient};
use wyrd_testing::server::WyrdTestServer;

/// The guide's readback query.
const REPLIES: &str = include_str!("../author-in-code/replies.sql");

/// One `refund_replies` row, as the readback query returns it.
#[derive(Debug, Deserialize)]
struct ReplyRow {
    /// The answer the program verified.
    answer: String,
    /// Its verdict.
    verdict: String,
    /// The Card of the view that wrote it.
    subject_card_uid: String,
    /// The Run that wrote it.
    run_id: String,
}

/// The program prints the exact Service and Agent versions declaration
/// selected and a passing and a failing verdict; an unchanged rerun prints
/// the same; and each written row reads back under the Agent and its own Run.
///
/// # Panics
/// Panics when the server cannot start, the program fails or prints anything
/// else, or the rows differ.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn author_in_code_declares_verifies_and_writes() {
    let server = WyrdTestServer::builder()
        .with_verification_runtime_for_test()
        .start_bound()
        .await
        .expect("test server starts");
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

    // The in-process server keeps serving while the binary runs.
    let (env_url, env_grpc, env_key) = (server_url.clone(), grpc_url.clone(), key.clone());
    let (first, again) = tokio::task::spawn_blocking(move || {
        let run = || {
            let output = Command::new(env!("CARGO_BIN_EXE_author_in_code"))
                .current_dir(dir.path())
                .env("WYRD_CONFIG_HOME", config.path())
                .env("WYRD_SERVER_URL", &env_url)
                .env("WYRD_GRPC_URL", &env_grpc)
                .env("WYRD_API_KEY", &env_key)
                .env_remove("WYRD_ACCESS_TOKEN")
                .env_remove("WYRD_WORKLOAD_TOKEN")
                .env_remove("WYRD_TENANT")
                .output()
                .expect("the author_in_code binary starts");
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

    let lines: Vec<&str> = first.lines().collect();
    let [service, agent, passed, failed] = lines[..] else {
        panic!("unexpected output: {first}");
    };
    assert!(
        service.starts_with("service: Service/refund-replies@0.1.0#"),
        "{first}"
    );
    assert!(
        agent.starts_with("agent:   Agent/refund-agent@0.1.0#"),
        "{first}"
    );
    assert_eq!(passed, "passed: Your refund request is approved.");
    assert_eq!(failed, "failed: You have a guaranteed refund.");
    assert_eq!(again, first, "an unchanged declaration keeps every version");

    server.flush_bifrost().await.expect("rows publish");
    let admin = WyrdClient::with_config(ClientConfig {
        http: HttpConfig {
            base_url: server_url,
            ..HttpConfig::default()
        },
        grpc: GrpcConfig {
            endpoint: grpc_url,
            ..GrpcConfig::default()
        },
        credential: Some(SecretString::from(key)),
        ..ClientConfig::default()
    })
    .expect("client builds");
    let rows: Vec<ReplyRow> = Bifrost::connect_with(&admin, BifrostOptions::default())
        .await
        .expect("Bifrost connects")
        .sql_as(REPLIES, &[])
        .await
        .expect("rows read back");
    let agent_uid = agent.rsplit_once('#').expect("agent carries a uid").1;
    assert_eq!(rows.len(), 4, "{rows:?}");
    for row in &rows {
        assert_eq!(row.subject_card_uid, agent_uid, "{row:?}");
        let expected = match row.answer.as_str() {
            "Your refund request is approved." => "passed",
            "You have a guaranteed refund." => "failed",
            other => panic!("unexpected answer {other}"),
        };
        assert_eq!(row.verdict, expected, "{row:?}");
    }
    let mut runs: Vec<&str> = rows.iter().map(|row| row.run_id.as_str()).collect();
    runs.sort_unstable();
    runs.dedup();
    assert_eq!(runs.len(), 4, "each answer has its own Run");
    server.shutdown().await.expect("test server shuts down");
}
