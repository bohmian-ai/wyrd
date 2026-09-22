//! Real-server CLI journey for principal administration.
//!
//! The revocation defect this covers — the command authenticated on a header no
//! Wyrd route reads — survived because nothing drove the CLI's transport against
//! a live server. So this journey runs the shipped `wyrd` binary over real HTTP:
//! a principal works, an administrator revokes it through the CLI, and the same
//! principal stops working.

use std::process::{Command, Output};

use assert_cmd::prelude::*;
use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use wyrd_testing::{Bootstrap, WyrdTestServer};

/// Run the shipped CLI with one named credential and a scrubbed ambient
/// environment.
///
/// The credential travels in its environment variable rather than on the command
/// line so nothing lands in the process table.
pub(crate) fn run_cli_with_credential(
    arguments: &[&str],
    variable: &str,
    credential: &str,
) -> Output {
    run_cli_with_env(arguments, &[(variable, credential)])
}

/// Run the shipped CLI with only the named environment sources set.
///
/// Every `WYRD_*` secret source is cleared first, so a journey proves the
/// command works from the sources it was given and cannot pass by picking up a
/// developer's ambient login.
///
/// # Panics
/// Panics when the `wyrd` binary cannot be built or located, or the process
/// fails to spawn.
pub(crate) fn run_cli_with_env(arguments: &[&str], sources: &[(&str, &str)]) -> Output {
    let mut command = Command::cargo_bin("wyrd").expect("wyrd binary builds");
    command
        .env(
            "WYRD_CONFIG_HOME",
            std::env::temp_dir().join(format!("wyrd-cli-empty-config-{}", std::process::id())),
        )
        .env_remove("WYRD_ACCESS_TOKEN")
        .env_remove("WYRD_WORKLOAD_TOKEN")
        .env_remove("WYRD_TENANT")
        .env_remove("WYRD_API_KEY")
        .env_remove("WYRD_PLATFORM_CREDENTIAL")
        .env_remove("WYRD_REFRESH_TOKEN")
        .env_remove("WYRD_ISSUER_CLIENT_SECRET")
        .envs(sources.iter().copied());
    command.args(arguments).output().expect("wyrd command runs")
}

/// Run the CLI off the async runtime, which a subprocess call would otherwise
/// block.
pub(crate) async fn run_cli_async_with_credential(
    arguments: Vec<String>,
    variable: &'static str,
    credential: String,
) -> Output {
    tokio::task::spawn_blocking(move || {
        let arguments = arguments.iter().map(String::as_str).collect::<Vec<_>>();
        run_cli_with_credential(&arguments, variable, &credential)
    })
    .await
    .expect("CLI subprocess joins")
}

/// Run the CLI off the async runtime with an access token.
async fn run_cli_async(arguments: Vec<String>, token: String) -> Output {
    run_cli_async_with_credential(arguments, "WYRD_ACCESS_TOKEN", token).await
}

/// Start a test server behind a real loopback listener.
///
/// The CLI is a separate process, so an in-process router is not reachable: the
/// journey needs a socket. Returns the server for bootstrapping and in-process
/// probes, its base URL for the CLI, and the handles that stop it.
pub(crate) async fn start_served(
    server_name: &str,
) -> (WyrdTestServer, String, CancellationToken, JoinHandle<()>) {
    let socket = std::net::TcpListener::bind("127.0.0.1:0").expect("journey listener binds");
    socket
        .set_nonblocking(true)
        .expect("journey listener becomes nonblocking");
    let address = socket
        .local_addr()
        .expect("journey listener has an address");
    let base_url = format!("http://{address}");
    let server = WyrdTestServer::start_in_process()
        .await
        .unwrap_or_else(|error| panic!("{server_name} test server starts: {error}"));
    let router = wyrd_server::build_router(server.state().clone());
    let listener =
        tokio::net::TcpListener::from_std(socket).expect("journey listener converts to Tokio");
    let shutdown = CancellationToken::new();
    let serve_shutdown = shutdown.clone();
    let serve_handle = tokio::spawn(async move {
        let _ = wyrd_server::app::serve::serve(router, listener, serve_shutdown).await;
    });
    (server, base_url, shutdown, serve_handle)
}

/// Stop the served router and the test server behind it.
pub(crate) async fn stop_served(
    server: WyrdTestServer,
    shutdown: CancellationToken,
    serve_handle: JoinHandle<()>,
) {
    shutdown.cancel();
    serve_handle.await.expect("journey server joins");
    server.shutdown().await.expect("test server shuts down");
}

/// Probe whether one token still reaches an authenticated `/v1` route.
///
/// Uses the in-process router rather than the CLI, so the assertion is about the
/// token's standing and not about a second command's behavior.
pub(crate) async fn v1_status(server: &WyrdTestServer, token: &str) -> StatusCode {
    server
        .oneshot_authenticated(
            token,
            Request::builder()
                .method(Method::GET)
                .uri("/v1/cards")
                .body(Body::empty())
                .expect("probe request builds"),
        )
        .await
        .expect("probe call completes")
        .status()
}

/// Exchange a bootstrapped machine principal's API key for an access token.
pub(crate) async fn machine_token(server: &WyrdTestServer, bootstrap: &Bootstrap) -> String {
    let key = bootstrap
        .api_key()
        .expect("machine principal has an api key");
    server
        .exchange_api_key(key)
        .await
        .expect("api key exchange succeeds")
}

/// An administrator revokes a principal through the CLI, against a real server.
///
/// Revocation suspends the principal: its API key can no longer mint a token,
/// while the token it already holds is a five-minute snapshot that keeps
/// working until expiry. The pre-revocation probe proves the CLI really
/// authenticated, so the refused exchange is the revocation's doing.
#[tokio::test]
async fn principal_revoke_cli_journey() {
    let (server, base_url, shutdown, serve_handle) = start_served("principal revoke").await;
    let admin = server
        .bootstrap_service("cli-revoke-admin", &["admin"])
        .await
        .expect("admin bootstraps");
    let admin_token = machine_token(&server, &admin).await;
    let target = server
        .bootstrap_service("cli-revoke-target", &["reader"])
        .await
        .expect("target bootstraps");
    let target_token = machine_token(&server, &target).await;

    assert_eq!(
        v1_status(&server, &target_token).await,
        StatusCode::OK,
        "the target principal's token works before revocation"
    );

    let revoke = run_cli_async(
        vec![
            "principal".to_owned(),
            "revoke".to_owned(),
            target.id().to_string(),
            "--kind".to_owned(),
            "service".to_owned(),
            "--reason".to_owned(),
            "cli journey".to_owned(),
            "--server".to_owned(),
            base_url.clone(),
        ],
        admin_token.clone(),
    )
    .await;
    assert!(
        revoke.status.success(),
        "revoke command failed: stdout={} stderr={}",
        String::from_utf8_lossy(&revoke.stdout),
        String::from_utf8_lossy(&revoke.stderr)
    );

    assert!(
        server
            .exchange_api_key(target.api_key().expect("target has an api key"))
            .await
            .is_err(),
        "the revoked principal cannot mint another token"
    );
    assert_eq!(
        v1_status(&server, &target_token).await,
        StatusCode::OK,
        "a token minted before revocation is a snapshot that lapses at expiry"
    );

    stop_served(server, shutdown, serve_handle).await;
}

/// A principal without administrative permission cannot revoke through the CLI.
///
/// The refusal must be the server's stable authorization error, not a local
/// guess: a CLI that decided this itself would be a second, driftable policy.
#[tokio::test]
async fn principal_revoke_cli_journey_refuses_an_unprivileged_caller() {
    let (server, base_url, shutdown, serve_handle) = start_served("principal revoke denial").await;
    let caller = server
        .bootstrap_service("cli-revoke-unprivileged", &["reader"])
        .await
        .expect("unprivileged caller bootstraps");
    let caller_token = machine_token(&server, &caller).await;
    let target = server
        .bootstrap_service("cli-revoke-denied-target", &["reader"])
        .await
        .expect("target bootstraps");
    let target_token = machine_token(&server, &target).await;

    let revoke = run_cli_async(
        vec![
            "principal".to_owned(),
            "revoke".to_owned(),
            target.id().to_string(),
            "--kind".to_owned(),
            "service".to_owned(),
            "--reason".to_owned(),
            "not allowed".to_owned(),
            "--server".to_owned(),
            base_url.clone(),
        ],
        caller_token,
    )
    .await;
    assert!(
        !revoke.status.success(),
        "an unprivileged caller must not revoke"
    );
    let stderr = String::from_utf8_lossy(&revoke.stderr);
    assert!(
        stderr.contains("WYRD_PERMISSION_403_DENIED_RBAC"),
        "the CLI reports the server's stable authorization error: {stderr}"
    );

    assert_eq!(
        v1_status(&server, &target_token).await,
        StatusCode::OK,
        "the refused revocation left the target principal working"
    );

    stop_served(server, shutdown, serve_handle).await;
}
