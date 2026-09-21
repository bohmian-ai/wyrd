//! The CLI reads every secret from its environment or a file, never argv.
//!
//! These drive the shipped binary against a mock server, so they prove the
//! sources a command actually reads — the ambient tenant credential, the
//! refresh token, and the issuer client secret — and that a missing one fails
//! with its stable code before any request is sent.

use std::process::Output;

use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use crate::principal_journey::run_cli_with_env;

/// Sentinel issuer client secret; it must reach the request body and nowhere
/// else.
const CLIENT_SECRET: &str = "issuer-client-secret-sentinel";

/// Run the CLI off the async runtime with only `sources` in its environment.
async fn run(arguments: Vec<String>, sources: Vec<(&'static str, String)>) -> Output {
    tokio::task::spawn_blocking(move || {
        let arguments = arguments.iter().map(String::as_str).collect::<Vec<_>>();
        let sources = sources
            .iter()
            .map(|(name, value)| (*name, value.as_str()))
            .collect::<Vec<_>>();
        run_cli_with_env(&arguments, &sources)
    })
    .await
    .expect("CLI subprocess joins")
}

/// Assert a command failed with one stable CLI error code.
fn failed_with(output: &Output, code: &str) {
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !output.status.success() && stderr.contains(code),
        "expected {code}: stderr={stderr}"
    );
}

/// Mount a trusted-issuer creation response on `server`.
async fn mount_trusted_issuer(server: &MockServer) {
    Mock::given(method("POST"))
        .and(path("/v1/admin/trusted-issuers"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "issuer": "https://idp.example.com",
            "jwks_uri": "https://idp.example.com/jwks",
            "expected_audience": "wyrd",
            "client_id": "myapp",
            "client_auth": "SecretPost",
            "principal_kind": "Workload",
            "jwks_ttl_secs": 3600,
            "claim_mapping": { "subject": "sub" },
            "group_role_map": { "dev": ["reader"] },
            "default_roles": ["reader"]
        })))
        .mount(server)
        .await;
}

/// `wyrd auth trusted-issuer add` arguments against `server`, with `extra`.
fn trusted_issuer_add(server: &MockServer, extra: &[&str]) -> Vec<String> {
    let mut arguments = [
        "auth",
        "trusted-issuer",
        "add",
        "--issuer",
        "https://idp.example.com",
        "--expected-audience",
        "wyrd",
        "--client-id",
        "myapp",
        "--client-auth",
        "SecretPost",
        "--claim-subject",
        "sub",
        "--principal-kind",
        "Workload",
        "--default-role",
        "reader",
        "--group-role",
        "dev=reader",
        "--server",
    ]
    .map(str::to_owned)
    .to_vec();
    arguments.push(server.uri());
    arguments.extend(extra.iter().map(|value| (*value).to_owned()));
    arguments
}

/// Assert the one recorded request carried the ambient token and the secret.
async fn assert_issuer_posted(server: &MockServer) {
    let requests = server.received_requests().await.expect("requests recorded");
    assert_eq!(requests.len(), 1, "exactly one POST expected");
    let body: serde_json::Value = serde_json::from_slice(&requests[0].body).expect("body is JSON");
    assert_eq!(body["client_secret"], CLIENT_SECRET);
    assert_eq!(body["client_auth"], "SecretPost");
    assert_eq!(body["default_roles"], serde_json::json!(["reader"]));
    assert_eq!(body["group_role_map"]["dev"], serde_json::json!(["reader"]));
    assert_eq!(
        requests[0]
            .headers
            .get("x-wyrd-access-token")
            .expect("access token header present"),
        "Bearer ambient-access-token",
        "the tenant credential comes from the ambient chain"
    );
}

/// The issuer client secret is read from the environment, and the tenant
/// credential from the ambient chain.
#[tokio::test]
async fn trusted_issuer_add_reads_the_client_secret_from_the_environment() {
    let server = MockServer::start().await;
    mount_trusted_issuer(&server).await;

    let output = run(
        trusted_issuer_add(&server, &[]),
        vec![
            ("WYRD_ACCESS_TOKEN", "ambient-access-token".to_owned()),
            ("WYRD_ISSUER_CLIENT_SECRET", CLIENT_SECRET.to_owned()),
        ],
    )
    .await;

    assert!(
        output.status.success(),
        "add failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!String::from_utf8_lossy(&output.stdout).contains(CLIENT_SECRET));
    assert_issuer_posted(&server).await;
}

/// The issuer client secret is read from a file, trailing newline trimmed.
#[tokio::test]
async fn trusted_issuer_add_reads_the_client_secret_from_a_file() {
    let server = MockServer::start().await;
    mount_trusted_issuer(&server).await;
    let file = tempfile::NamedTempFile::new().expect("secret file creates");
    std::fs::write(file.path(), format!("{CLIENT_SECRET}\n")).expect("secret file writes");
    let secret_path = file.path().to_str().expect("utf8 path").to_owned();

    let output = run(
        trusted_issuer_add(&server, &["--client-secret-file", &secret_path]),
        vec![("WYRD_ACCESS_TOKEN", "ambient-access-token".to_owned())],
    )
    .await;

    assert!(
        output.status.success(),
        "add failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_issuer_posted(&server).await;
}

/// A tenant command with no ambient credential fails with the stable code and
/// sends nothing.
#[tokio::test]
async fn a_tenant_command_without_an_ambient_credential_fails_clearly() {
    let server = MockServer::start().await;

    let output = run(
        vec![
            "auth".to_owned(),
            "trusted-issuer".to_owned(),
            "list".to_owned(),
            "--server".to_owned(),
            server.uri(),
        ],
        Vec::new(),
    )
    .await;

    failed_with(&output, "WYRD_CLI_401_NO_CREDENTIALS");
    assert!(
        server
            .received_requests()
            .await
            .expect("requests recorded")
            .is_empty()
    );
}

/// `wyrd auth refresh` rotates the refresh token read from the environment.
#[tokio::test]
async fn refresh_reads_the_refresh_token_from_the_environment() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/auth/token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "access_token": "rotated-access",
            "refresh_token": "rotated-refresh",
            "token_type": "Bearer",
            "expires_at": "2030-01-01T00:00:00Z"
        })))
        .mount(&server)
        .await;
    let arguments = vec![
        "auth".to_owned(),
        "refresh".to_owned(),
        "--server".to_owned(),
        server.uri(),
    ];

    let output = run(
        arguments.clone(),
        vec![("WYRD_REFRESH_TOKEN", "refresh-sentinel".to_owned())],
    )
    .await;

    assert!(
        output.status.success(),
        "refresh failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("rotated-refresh"));
    let requests = server.received_requests().await.expect("requests recorded");
    let body: serde_json::Value = serde_json::from_slice(&requests[0].body).expect("body is JSON");
    assert_eq!(body["refresh_token"], "refresh-sentinel");

    let missing = run(arguments, Vec::new()).await;
    failed_with(&missing, "WYRD_CLI_401_NO_REFRESH_TOKEN");
    assert_eq!(
        server
            .received_requests()
            .await
            .expect("requests recorded")
            .len(),
        1,
        "a missing refresh token sends nothing"
    );
}
