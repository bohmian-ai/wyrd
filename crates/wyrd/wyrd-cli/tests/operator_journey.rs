//! Real-server CLI journey for the whole operator workflow.
//!
//! The operator documentation promises a deployment can be administered without
//! SQL. That promise is only true if the shipped binary actually completes the
//! path, so this drives `wyrd` over real HTTP from the platform credential
//! through tenant creation, lifecycle administration, restricted-principal
//! creation, an overlapping credential rotation, and recovery — reading every
//! identifier it needs out of the command output before it, exactly as an
//! operator following the page would.

use secrecy::ExposeSecret as _;
use std::process::Output;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use crate::principal_journey::{
    run_cli_async_with_credential, start_served, stop_served, v1_status,
};

/// Run a `wyrd` invocation carrying the platform credential.
async fn platform_cli(arguments: Vec<String>, credential: String) -> Output {
    run_cli_async_with_credential(arguments, "WYRD_PLATFORM_CREDENTIAL", credential).await
}

/// Run a `wyrd` invocation carrying a tenant credential.
///
/// The value is whatever the operator was handed — the tenant administrator's
/// API key, printed once at tenant creation — passed through the CLI's one
/// explicit credential input. The shared client classifies it and exchanges a
/// key for a token itself, so the journey never has to know which kind it
/// holds, and never has to mint one out of band.
async fn tenant_cli(arguments: Vec<String>, credential: String) -> Output {
    run_cli_async_with_credential(arguments, "WYRD_ACCESS_TOKEN", credential).await
}

/// Assert a command succeeded and return its stdout.
fn succeeded(what: &str, output: &Output) -> String {
    assert!(
        output.status.success(),
        "{what} failed: stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// Read one `key: value` line out of a command's output.
fn field(stdout: &str, key: &str) -> String {
    stdout
        .lines()
        .find_map(|line| line.strip_prefix(&format!("{key}: ")))
        .unwrap_or_else(|| panic!("output carries {key}: {stdout}"))
        .trim()
        .to_owned()
}

/// Stand up an OIDC issuer the server can actually discover.
///
/// Tenant issuer configuration resolves the JWKS endpoint from the issuer's
/// own discovery document, so the journey has to serve one rather than assert
/// against a URL nothing answers.
async fn discovery_server() -> (MockServer, String) {
    let server = MockServer::start().await;
    let issuer = server.uri();
    Mock::given(method("GET"))
        .and(path("/.well-known/openid-configuration"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "issuer": issuer,
            "authorization_endpoint": format!("{issuer}/authorize"),
            "token_endpoint": format!("{issuer}/token"),
            "jwks_uri": format!("{issuer}/jwks"),
            "id_token_signing_alg_values_supported": ["RS256", "EdDSA"],
        })))
        .mount(&server)
        .await;
    (server, issuer)
}

/// An operator administers a deployment end to end through the shipped binary.
#[tokio::test]
async fn operator_administers_a_deployment_through_the_cli() {
    if std::env::var("WYRD_CLI_E2E").as_deref() != Ok("1") {
        return;
    }

    let (server, base_url, shutdown, serve_handle) = start_served("operator workflow").await;
    let root = server
        .initialize_platform_root()
        .await
        .expect("deployment initializes");
    let platform = root.expose_secret().to_owned();
    let endpoint = || vec!["--server".to_owned(), base_url.clone()];

    // 1. Create a tenant. Its administrative credential is printed once.
    let mut arguments = vec![
        "platform".to_owned(),
        "tenant".to_owned(),
        "create".to_owned(),
        "--slug".to_owned(),
        "cli-operator".to_owned(),
        "--display-name".to_owned(),
        "CLI Operator".to_owned(),
    ];
    arguments.extend(endpoint());
    let created = succeeded(
        "tenant create",
        &platform_cli(arguments, platform.clone()).await,
    );
    let tenant_id = field(&created, "tenant_id");
    let tenant_credential = field(&created, "admin_credential");

    // 2. The directory is readable, and names the tenant just created.
    let mut arguments = vec![
        "platform".to_owned(),
        "tenant".to_owned(),
        "inspect".to_owned(),
        "--tenant".to_owned(),
        tenant_id.clone(),
    ];
    arguments.extend(endpoint());
    let inspected = succeeded(
        "tenant inspect",
        &platform_cli(arguments, platform.clone()).await,
    );
    assert_eq!(field(&inspected, "status"), "active");
    assert_eq!(field(&inspected, "slug"), "cli-operator");

    // 3. Inside the tenant: configure the issuer its workloads federate from.
    //
    // This is the step that makes the tenant usable by anything other than the
    // credential just printed, and it is the shipped command an operator runs
    // to do it — not a SQL insert and not a server-side seed.
    let (idp, issuer) = discovery_server().await;
    let mut arguments = vec![
        "auth".to_owned(),
        "trusted-issuer".to_owned(),
        "add".to_owned(),
        "--issuer".to_owned(),
        issuer.clone(),
        "--expected-audience".to_owned(),
        "wyrd-cli-operator".to_owned(),
        "--client-id".to_owned(),
        "wyrd".to_owned(),
        "--client-auth".to_owned(),
        "Public".to_owned(),
        "--claim-subject".to_owned(),
        "sub".to_owned(),
        "--principal-kind".to_owned(),
        "Workload".to_owned(),
        "--default-role".to_owned(),
        "reader".to_owned(),
    ];
    arguments.extend(endpoint());
    let configured = succeeded(
        "trusted issuer add",
        &tenant_cli(arguments, tenant_credential.clone()).await,
    );
    assert_eq!(field(&configured, "issuer"), issuer);
    assert_eq!(field(&configured, "jwks_uri"), format!("{issuer}/jwks"));

    let mut arguments = vec![
        "auth".to_owned(),
        "trusted-issuer".to_owned(),
        "list".to_owned(),
    ];
    arguments.extend(endpoint());
    let listed = succeeded(
        "trusted issuer list",
        &tenant_cli(arguments, tenant_credential.clone()).await,
    );
    assert!(
        listed.contains(&issuer),
        "the configured issuer is readable back: {listed}"
    );
    drop(idp);

    // 4. Inside the tenant: a principal narrower than its administrator.
    let mut arguments = vec![
        "principal".to_owned(),
        "credential".to_owned(),
        "create-principal".to_owned(),
        "--name".to_owned(),
        "cli-runner".to_owned(),
        "--role".to_owned(),
        "reader".to_owned(),
    ];
    arguments.extend(endpoint());
    let principal = succeeded(
        "principal create",
        &tenant_cli(arguments, tenant_credential.clone()).await,
    );
    let principal_id = field(&principal, "principal_id");
    let first_credential = field(&principal, "credential");
    let first_token = server
        .exchange_api_key(&secrecy::SecretString::from(first_credential))
        .await
        .expect("the new principal's credential exchanges");
    assert_eq!(
        v1_status(&server, &first_token).await,
        axum::http::StatusCode::OK,
        "the restricted principal works before rotation"
    );

    // 5. Rotate with an overlap: issue, verify, then retire the old one.
    let mut arguments = vec![
        "principal".to_owned(),
        "credential".to_owned(),
        "issue".to_owned(),
        "--principal".to_owned(),
        principal_id.clone(),
    ];
    arguments.extend(endpoint());
    let issued = succeeded(
        "credential issue",
        &tenant_cli(arguments, tenant_credential.clone()).await,
    );
    let replacement = server
        .exchange_api_key(&secrecy::SecretString::from(field(&issued, "credential")))
        .await
        .expect("the replacement credential exchanges");
    assert_eq!(
        v1_status(&server, &replacement).await,
        axum::http::StatusCode::OK,
        "the replacement works while the original is still live"
    );

    let mut arguments = vec![
        "principal".to_owned(),
        "credential".to_owned(),
        "list".to_owned(),
        "--principal".to_owned(),
        principal_id.clone(),
    ];
    arguments.extend(endpoint());
    let listed = succeeded(
        "credential list",
        &tenant_cli(arguments, tenant_credential.clone()).await,
    );
    let superseded = listed
        .lines()
        .rfind(|line| !line.is_empty())
        .and_then(|line| line.split_whitespace().next())
        .expect("the listing carries the oldest credential")
        .to_owned();

    let mut arguments = vec![
        "principal".to_owned(),
        "credential".to_owned(),
        "revoke".to_owned(),
        "--principal".to_owned(),
        principal_id.clone(),
        "--credential-id".to_owned(),
        superseded,
    ];
    arguments.extend(endpoint());
    succeeded(
        "credential revoke",
        &tenant_cli(arguments, tenant_credential.clone()).await,
    );

    // 6. Lifecycle administration, from the platform plane.
    for verb in ["suspend", "resume"] {
        let mut arguments = vec![
            "platform".to_owned(),
            "tenant".to_owned(),
            verb.to_owned(),
            "--tenant".to_owned(),
            tenant_id.clone(),
        ];
        arguments.extend(endpoint());
        succeeded(verb, &platform_cli(arguments, platform.clone()).await);
    }

    // 7. Recovery restores the tenant's existing administrator.
    let mut arguments = vec![
        "platform".to_owned(),
        "tenant".to_owned(),
        "recover-admin".to_owned(),
        "--tenant".to_owned(),
        tenant_id.clone(),
    ];
    arguments.extend(endpoint());
    let recovered = succeeded(
        "recover-admin",
        &platform_cli(arguments, platform.clone()).await,
    );
    let restored = server
        .exchange_api_key(&secrecy::SecretString::from(field(
            &recovered,
            "credential",
        )))
        .await
        .expect("the recovery credential exchanges");
    assert_eq!(
        v1_status(&server, &restored).await,
        axum::http::StatusCode::OK,
        "the recovered administrator administers the tenant again"
    );

    stop_served(server, shutdown, serve_handle).await;
}
