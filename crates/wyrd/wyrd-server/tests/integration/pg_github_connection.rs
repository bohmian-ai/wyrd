//! The GitHub installation token facility through a real bound server and
//! the GitHub App mock.
//!
//! A tenant administrator connects installation `101` through a browser;
//! background work then asks the server for installation tokens. Each token
//! is minted narrowed to one repository and the App's three permissions, only
//! while the repository is selected and the installation active, and the
//! facility refuses a repository the moment its access is lost while the
//! others, and the tenant's unrelated queued verification work, carry on.

use std::collections::BTreeMap;

use axum::body::Body;
use axum::http::Request;
use secrecy::ExposeSecret as _;
use serde_json::json;
use wyrd_server::components::github::service::GithubConnections;
use wyrd_spec::auth::PrincipalId;
use wyrd_testing::WyrdTestServer;
use wyrd_testing::github::{GithubAppMock, GithubBrowser, INSTALLATION_TOKEN_PREFIX};
use wyrd_testing::verification::VerificationFixture;

/// Fail unless minting a token for `repository` is refused with `code`.
async fn refused(server: &WyrdTestServer, repository: u64, code: &str) {
    let error = GithubConnections::new(server.state())
        .installation_token(server.data_tenant_id(), repository)
        .await
        .expect_err("the token is refused");
    assert_eq!(error.code(), code, "repository {repository}");
}

/// Mint a token for `repository` and fail unless it is GitHub's.
async fn minted(server: &WyrdTestServer, repository: u64) {
    let token = GithubConnections::new(server.state())
        .installation_token(server.data_tenant_id(), repository)
        .await
        .expect("the token is minted");
    assert!(
        token.expose_secret().starts_with(INSTALLATION_TOKEN_PREFIX),
        "repository {repository}"
    );
}

/// `principal`'s retained decisions for `operation` as (permission,
/// outcome), once every decision staged so far is retained.
///
/// # Panics
/// Panics when audit retention does not settle or the rows cannot be read.
async fn decisions(
    server: &WyrdTestServer,
    principal: PrincipalId,
    operation: &str,
) -> Vec<(String, String)> {
    server
        .await_audit_retained()
        .await
        .expect("audit retention settles");
    server
        .retained_audit_records(
            server.data_tenant_id(),
            "permission, outcome",
            &format!(
                "operation = '{operation}' AND sub_principal_id = '{}'",
                principal.as_uuid()
            ),
        )
        .await
        .expect("audit rows read")
        .into_iter()
        .map(|row| {
            (
                row[0].clone().unwrap_or_default(),
                row[1].clone().unwrap_or_default(),
            )
        })
        .collect()
}

/// Installation tokens are narrowed to one selected repository and the
/// App's permissions; removal, GitHub-side deselection, suspension and
/// disconnect each refuse only what lost access, and none of them touch the
/// tenant's queued verification work.
///
/// # Panics
/// Panics when a token is minted or refused other than described.
#[tokio::test]
async fn installation_tokens_follow_repository_access() {
    let mock = GithubAppMock::start().expect("the GitHub mock starts");
    mock.add_installation(101, &[(1, "acme/api"), (2, "acme/web"), (3, "acme/vault")]);
    mock.add_user("octo", &[101]);
    mock.sign_in("octo");
    mock.install_target(101);
    let server = WyrdTestServer::builder()
        .with_github_app_for_test(&mock)
        .start_bound()
        .await
        .expect("test server starts");
    let base = server.base_url().expect("bound").to_owned();
    let admin = server
        .bootstrap_service_in_tenant(server.data_tenant_id(), "github_admin", &["admin"])
        .await
        .expect("administrator bootstraps");
    let api_key = admin.api_key().expect("a machine principal").clone();
    let mut browser = GithubBrowser::sign_in(
        &base,
        server.pg_fixture().tenant_slug(),
        api_key.expose_secret(),
    )
    .await
    .expect("browser signs in");

    refused(&server, 1, "WYRD_GITHUB_404_NOT_CONNECTED").await;
    let fixture = VerificationFixture::provision(server.state(), server.data_tenant_id())
        .await
        .expect("the tenant provisions");
    let (subject, _) = fixture.service("bystander").await.expect("service");
    let verifier = fixture.drift_verifier("unrelated").await.expect("verifier");
    let queued = fixture
        .submit(&verifier, &subject, BTreeMap::new())
        .await
        .expect("the execution is admitted");

    let answer = browser
        .open(&format!(
            "{}apps/wyrd-test-app/installations/new",
            mock.origin()
        ))
        .await
        .expect("browser navigates");
    assert_eq!(answer.status, 200, "{:?}", answer.body);

    minted(&server, 1).await;
    assert_eq!(
        mock.token_requests(),
        vec![json!({
            "repository_ids": [1],
            "permissions": {
                "contents": "read",
                "pull_requests": "write",
                "checks": "write",
            },
        })]
    );
    refused(&server, 4, "WYRD_GITHUB_403_REPOSITORY_INACCESSIBLE").await;

    // A removed repository is refused before GitHub is asked.
    let removed = mock
        .deliver(
            &base,
            "installation_repositories",
            &json!({
                "action": "removed",
                "installation": { "id": 101 },
                "repositories_removed": [{ "id": 2 }],
            }),
        )
        .await
        .expect("delivery sends");
    assert_eq!(removed, 202);
    let asked = mock.token_requests().len();
    refused(&server, 2, "WYRD_GITHUB_403_REPOSITORY_INACCESSIBLE").await;
    assert_eq!(mock.token_requests().len(), asked);
    minted(&server, 1).await;

    // GitHub's own refusal wins over a selection Wyrd has not yet heard of.
    mock.deselect_repository(101, 3);
    refused(&server, 3, "WYRD_GITHUB_403_REPOSITORY_INACCESSIBLE").await;
    minted(&server, 1).await;

    let event = |action: &str| json!({ "action": action, "installation": { "id": 101 } });
    mock.deliver(&base, "installation", &event("suspend"))
        .await
        .expect("delivery sends");
    refused(&server, 1, "WYRD_GITHUB_403_REPOSITORY_INACCESSIBLE").await;
    mock.deliver(&base, "installation", &event("unsuspend"))
        .await
        .expect("delivery sends");
    minted(&server, 1).await;

    let token = server
        .exchange_api_key(&api_key)
        .await
        .expect("api key exchanges");
    let disconnected = server
        .oneshot_authenticated(
            &token,
            Request::delete("/v1/integrations/github")
                .body(Body::empty())
                .expect("request builds"),
        )
        .await
        .expect("disconnect responds");
    assert_eq!(disconnected.status(), 200);
    let viewer = server
        .bootstrap_service_in_tenant(server.data_tenant_id(), "github_viewer", &["viewer"])
        .await
        .expect("viewer bootstraps");
    let viewer_token = server
        .exchange_api_key(viewer.api_key().expect("a machine principal"))
        .await
        .expect("api key exchanges");
    let denied = server
        .oneshot_authenticated(
            &viewer_token,
            Request::get("/v1/integrations/github")
                .body(Body::empty())
                .expect("request builds"),
        )
        .await
        .expect("read responds");
    assert_eq!(denied.status(), 403);
    let write = || {
        (
            "identity_connections:write".to_owned(),
            "allowed".to_owned(),
        )
    };
    for operation in [
        "github_connection.setup",
        "github_connection.callback",
        "github_connection.disconnect",
    ] {
        assert_eq!(
            decisions(&server, admin.id(), operation).await,
            vec![write()],
            "{operation}"
        );
    }
    assert_eq!(
        decisions(&server, viewer.id(), "github_connection.read").await,
        vec![("identity_connections:write".to_owned(), "denied".to_owned())]
    );
    refused(&server, 1, "WYRD_GITHUB_404_NOT_CONNECTED").await;

    assert_eq!(
        fixture.executions().await.expect("executions read"),
        vec![queued]
    );
    assert_eq!(
        fixture
            .execution(queued)
            .await
            .expect("execution reads")
            .status,
        "pending"
    );
    for request in mock.token_requests() {
        assert_eq!(request["repository_ids"].as_array().map(Vec::len), Some(1));
    }
}
