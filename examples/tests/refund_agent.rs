//! The agent workflow guide's Rust refund agent, run as a reader runs it.
//!
//! The journey runs the built `refund_agent` binary from an empty directory
//! against a real server. The program exports its spans through
//! `start_telemetry`, so the trace check reads the traces it recorded: the
//! complete reply passes, the reply that skipped its lookup fails, and the
//! reply sent without a trace is inconclusive.

use std::process::Command;

use secrecy::ExposeSecret;
use wyrd_testing::server::WyrdTestServer;

/// What the program prints, as the guide shows it.
const OUTPUT: &str = include_str!("../refund-agent/output.txt");

/// The program prints the three cases' verdicts and outcomes, as the guide
/// shows them.
///
/// # Panics
/// Panics when the server cannot start, the program fails, or it prints
/// anything else.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn refund_agent_fails_the_skipped_lookup() {
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
    let output = tokio::task::spawn_blocking(move || {
        Command::new(env!("CARGO_BIN_EXE_refund_agent"))
            .current_dir(dir.path())
            .env("WYRD_CONFIG_HOME", config.path())
            .env("WYRD_SERVER_URL", &server_url)
            .env("WYRD_GRPC_URL", &grpc_url)
            .env("WYRD_API_KEY", &key)
            .env_remove("WYRD_ACCESS_TOKEN")
            .env_remove("WYRD_WORKLOAD_TOKEN")
            .env_remove("WYRD_TENANT")
            .output()
            .expect("the refund_agent binary starts")
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
        OUTPUT
    );
    server.shutdown().await.expect("test server shuts down");
}
