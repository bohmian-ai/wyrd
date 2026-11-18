//! The agent workflow guide's Rust refund agent, run as a reader runs it.
//!
//! The journey runs the built `refund_agent` binary from an empty directory
//! against a real server. The program exports its spans through
//! `start_telemetry`, so the trace check reads the traces it recorded: the
//! complete reply passes, the reply that skipped its lookup fails, and the
//! reply sent without a trace is inconclusive.

use std::process::Command;

use secrecy::{ExposeSecret, SecretString};
use wyrd_ai::WyrdClient;
use wyrd_ai::config::ClientConfig;
use wyrd_ai::transport::{GrpcConfig, HttpConfig};
use wyrd_ai::verify::Execution;
use wyrd_testing::server::WyrdTestServer;

/// What the program prints, as the guide shows it.
const OUTPUT: &str = include_str!("../refund-agent/output.txt");

/// What the investigation prints, as the guide shows it.
const INVESTIGATION: &str = include_str!("../refund-agent/investigate-output.txt");

/// The program prints the three cases' verdicts and outcomes, as the guide
/// shows them; the investigation then finds the stored failure and prints
/// its failed requirement and the trace it read; and an unknown execution's
/// details are not found.
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

    let client = WyrdClient::with_config(ClientConfig {
        http: HttpConfig {
            base_url: server_url.clone(),
            ..HttpConfig::default()
        },
        grpc: GrpcConfig {
            endpoint: grpc_url.clone(),
            ..GrpcConfig::default()
        },
        credential: Some(SecretString::from(key.clone())),
        ..ClientConfig::default()
    })
    .expect("client builds");

    // The in-process server keeps serving while the binaries run.
    let (printed, investigated) = tokio::task::spawn_blocking(move || {
        let run = |program: &str| {
            let output = Command::new(program)
                .current_dir(dir.path())
                .env("WYRD_CONFIG_HOME", config.path())
                .env("WYRD_SERVER_URL", &server_url)
                .env("WYRD_GRPC_URL", &grpc_url)
                .env("WYRD_API_KEY", &key)
                .env_remove("WYRD_ACCESS_TOKEN")
                .env_remove("WYRD_WORKLOAD_TOKEN")
                .env_remove("WYRD_TENANT")
                .output()
                .expect("the binary starts");
            assert!(
                output.status.success(),
                "{program} fails: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            String::from_utf8(output.stdout).expect("the program prints UTF-8")
        };
        (
            run(env!("CARGO_BIN_EXE_refund_agent")),
            run(env!("CARGO_BIN_EXE_investigate")),
        )
    })
    .await
    .expect("the program task joins");
    assert_eq!(printed, OUTPUT);
    assert_eq!(investigated, INVESTIGATION);

    let unknown = Execution::connect_with(
        "01a1ec29-0000-7000-8000-000000000000"
            .parse()
            .expect("execution id parses"),
        &client,
    )
    .await
    .expect("the handle connects")
    .details()
    .await
    .expect_err("an unknown execution has no details");
    assert_eq!(unknown.code(), "WYRD_VERIFICATION_404_EXECUTION_NOT_FOUND");
    server.shutdown().await.expect("test server shuts down");
}
