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
use wyrd_server::boot::init::initialize_platform_root;

use crate::principal_journey::{
    run_cli_async_with_credential, start_served, stop_served, v1_status,
};

/// Run a `wyrd` invocation carrying the platform credential.
async fn platform_cli(arguments: Vec<String>, credential: String) -> std::process::Output {
    run_cli_async_with_credential(arguments, "WYRD_PLATFORM_CREDENTIAL", credential).await
}

/// Run a `wyrd` invocation carrying a tenant access token.
async fn tenant_cli(arguments: Vec<String>, token: String) -> std::process::Output {
    run_cli_async_with_credential(arguments, "WYRD_ACCESS_TOKEN", token).await
}

/// Assert a command succeeded and return its stdout.
fn succeeded(what: &str, output: &std::process::Output) -> String {
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

/// An operator administers a deployment end to end through the shipped binary.
#[tokio::test]
async fn operator_administers_a_deployment_through_the_cli() {
    if std::env::var("WYRD_CLI_E2E").as_deref() != Ok("1") {
        return;
    }

    let (server, base_url, shutdown, serve_handle) = start_served("operator workflow").await;
    let root = initialize_platform_root(&server.operator_pool())
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

    // 3. Inside the tenant: a principal narrower than its administrator.
    let tenant_token = server
        .exchange_api_key(&secrecy::SecretString::from(tenant_credential))
        .await
        .expect("the tenant administrator's credential exchanges");
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
        &tenant_cli(arguments, tenant_token.clone()).await,
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

    // 4. Rotate with an overlap: issue, verify, then retire the old one.
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
        &tenant_cli(arguments, tenant_token.clone()).await,
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
        &tenant_cli(arguments, tenant_token.clone()).await,
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
        &tenant_cli(arguments, tenant_token.clone()).await,
    );

    // 5. Lifecycle administration, from the platform plane.
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

    // 6. Recovery restores the tenant's existing administrator.
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
