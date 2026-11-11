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
///
/// # Panics
/// Panics when a token is minted or the refusal has another code.
async fn refused(server: &WyrdTestServer, repository: u64, code: &str) {
    let error = GithubConnections::new(server.state())
        .installation_token(server.data_tenant_id(), repository)
        .await
        .expect_err("the token is refused");
    assert_eq!(error.code(), code, "repository {repository}");
}

/// Mint a token for `repository` and fail unless it is GitHub's.
///
/// # Panics
/// Panics when the token is refused or is not GitHub's.
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
    mock.set_suspended(101, true);
    mock.deliver(&base, "installation", &event("suspend"))
        .await
        .expect("delivery sends");
    refused(&server, 1, "WYRD_GITHUB_403_REPOSITORY_INACCESSIBLE").await;
    mock.set_suspended(101, false);
    mock.deliver(&base, "installation", &event("unsuspend"))
        .await
        .expect("delivery sends");
    minted(&server, 1).await;

    // No key, code or token is stored, and connecting created no binding.
    let mut conn = server
        .tenant_conn_for(server.data_tenant_id())
        .await
        .expect("tenant connection opens");
    let stored: String = sqlx::query_scalar(
        "SELECT coalesce((SELECT string_agg(row_to_json(g)::text, ' ') \
                          FROM wyrd.github_installations g), '') \
             || coalesce((SELECT string_agg(row_to_json(s)::text, ' ') \
                          FROM wyrd.auth_login_state s), '')",
    )
    .fetch_one(&mut **conn.transaction())
    .await
    .expect("rows read");
    let bindings: i64 = sqlx::query_scalar("SELECT count(*) FROM wyrd.verification_bindings")
        .fetch_one(&mut **conn.transaction())
        .await
        .expect("bindings count");
    conn.commit().await.expect("read commits");
    assert!(stored.contains("101"), "the slot is read: {stored}");
    for secret in mock.secrets() {
        assert!(!stored.contains(&secret), "stored secret: {stored}");
    }
    assert_eq!(bindings, 0);

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
    for (request, status) in [
        (
            Request::post("/v1/integrations/github/connect")
                .header("content-type", "application/json")
                .body(Body::from("{}"))
                .expect("request builds"),
            201,
        ),
        (
            Request::get("/v1/integrations/github")
                .body(Body::empty())
                .expect("request builds"),
            200,
        ),
    ] {
        let answered = server
            .oneshot_authenticated(&token, request)
            .await
            .expect("request responds");
        assert_eq!(answered.status(), status);
    }
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
        "github_connection.connect",
        "github_connection.read",
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

/// A tenant connected to installation `101` (repositories 1, 2 and 3)
/// through its administrator's browser, with the administrator's token.
struct Connected {
    /// GitHub's side.
    mock: GithubAppMock,
    /// The bound server.
    server: WyrdTestServer,
    /// The server's base URL.
    base: String,
    /// The administrator's access token.
    token: String,
}

impl Connected {
    /// Start GitHub and the server and connect the tenant, returning the
    /// administrator's signed-in browser beside it.
    ///
    /// # Panics
    /// Panics when the server does not start or the connection fails.
    async fn start() -> (Self, GithubBrowser) {
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
        let token = server
            .exchange_api_key(&api_key)
            .await
            .expect("api key exchanges");
        let mut browser = GithubBrowser::sign_in(
            &base,
            server.pg_fixture().tenant_slug(),
            api_key.expose_secret(),
        )
        .await
        .expect("browser signs in");
        let answer = browser
            .open(&format!(
                "{}apps/wyrd-test-app/installations/new",
                mock.origin()
            ))
            .await
            .expect("browser navigates");
        assert_eq!(answer.status, 200, "{:?}", answer.body);
        (
            Self {
                mock,
                server,
                base,
                token,
            },
            browser,
        )
    }

    /// Send `request` as the administrator.
    ///
    /// # Panics
    /// Panics when the request cannot be served.
    async fn send(&self, request: Request<Body>) -> axum::http::Response<Body> {
        self.server
            .oneshot_authenticated(&self.token, request)
            .await
            .expect("request responds")
    }

    /// The tenant's connection view.
    ///
    /// # Panics
    /// Panics when the read fails.
    async fn view(&self) -> serde_json::Value {
        let response = self
            .send(
                Request::get("/v1/integrations/github")
                    .body(Body::empty())
                    .expect("request builds"),
            )
            .await;
        assert_eq!(response.status(), 200);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body reads");
        serde_json::from_slice(&body).expect("the view is JSON")
    }

    /// Begin a connection and return GitHub's install URL for it.
    ///
    /// # Panics
    /// Panics when connect is refused.
    async fn connect(&self) -> String {
        let response = self
            .send(
                Request::post("/v1/integrations/github/connect")
                    .header("content-type", "application/json")
                    .body(Body::from("{}"))
                    .expect("request builds"),
            )
            .await;
        assert_eq!(response.status(), 201);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body reads");
        let receipt: serde_json::Value = serde_json::from_slice(&body).expect("JSON");
        receipt["authorization_url"]
            .as_str()
            .expect("an install URL")
            .to_owned()
    }

    /// Disconnect the tenant and fail unless it was released.
    ///
    /// # Panics
    /// Panics when disconnect is refused.
    async fn disconnect(&self) {
        let response = self
            .send(
                Request::delete("/v1/integrations/github")
                    .body(Body::empty())
                    .expect("request builds"),
            )
            .await;
        assert_eq!(response.status(), 200);
    }

    /// Deliver an `installation` webhook with `action` for installation 101
    /// and return its status.
    ///
    /// # Panics
    /// Panics when the delivery cannot be sent.
    async fn installation_event(&self, action: &str) -> u16 {
        self.mock
            .deliver(
                &self.base,
                "installation",
                &json!({ "action": action, "installation": { "id": 101 } }),
            )
            .await
            .expect("delivery sends")
            .as_u16()
    }

    /// Deliver GitHub's removal of `repository` from installation 101.
    ///
    /// # Panics
    /// Panics when the delivery is not accepted.
    async fn remove(&self, repository: u64) {
        let status = self
            .mock
            .deliver(
                &self.base,
                "installation_repositories",
                &json!({
                    "action": "removed",
                    "installation": { "id": 101 },
                    "repositories_removed": [{ "id": repository }],
                }),
            )
            .await
            .expect("delivery sends");
        assert_eq!(status, 202);
    }

    /// Run `sql` on the tenant's connection and commit, standing in for state
    /// saved under earlier App settings.
    ///
    /// # Panics
    /// Panics when the statement fails.
    async fn rewrite(&self, sql: &'static str) {
        let mut conn = self
            .server
            .tenant_conn_for(self.server.data_tenant_id())
            .await
            .expect("tenant connection opens");
        sqlx::query(sql)
            .execute(&mut **conn.transaction())
            .await
            .expect("rewrite runs");
        conn.commit().await.expect("rewrite commits");
    }

    /// Mint a token for `repository` while `during` runs against GitHub's
    /// held answer, and return the minting result.
    async fn mint_while<F: std::future::Future<Output = ()>>(
        &self,
        repository: u64,
        during: F,
    ) -> Result<secrecy::SecretString, wyrd_spec::error::WyrdError> {
        let hold = self.mock.hold_next_token();
        let connections = GithubConnections::new(self.server.state());
        let (minted, ()) = tokio::join!(
            connections.installation_token(self.server.data_tenant_id(), repository),
            async {
                hold.arrived().await;
                during.await;
                hold.release();
            }
        );
        minted
    }
}

/// A token GitHub mints while access changes is discarded: an unrelated
/// removal, the repository's own removal, a suspension, a disconnect
/// followed by a reconnect, and a disconnect each refuse the old request, and
/// a fresh request is decided on current access.
///
/// # Panics
/// Panics when a discarded token is handed out or a fresh request answers
/// differently.
#[tokio::test]
async fn tokens_minted_across_an_access_change_are_discarded() {
    let (connected, mut browser) = Connected::start().await;
    let server = &connected.server;
    let conflict = "WYRD_GITHUB_409_CONNECTION_CONFLICT";

    let unrelated = connected.mint_while(1, connected.remove(3)).await;
    assert_eq!(
        unrelated.expect_err("the token is discarded").code(),
        conflict
    );
    minted(server, 1).await;

    let lost = connected.mint_while(2, connected.remove(2)).await;
    assert_eq!(lost.expect_err("the token is discarded").code(), conflict);
    refused(server, 2, "WYRD_GITHUB_403_REPOSITORY_INACCESSIBLE").await;

    let suspended = connected
        .mint_while(1, async {
            connected.mock.set_suspended(101, true);
            assert_eq!(connected.installation_event("suspend").await, 202);
        })
        .await;
    assert_eq!(
        suspended.expect_err("the token is discarded").code(),
        conflict
    );
    refused(server, 1, "WYRD_GITHUB_403_REPOSITORY_INACCESSIBLE").await;
    connected.mock.set_suspended(101, false);
    assert_eq!(connected.installation_event("unsuspend").await, 202);
    minted(server, 1).await;

    let install = format!(
        "{}apps/wyrd-test-app/installations/new",
        connected.mock.origin()
    );
    let reclaimed = connected
        .mint_while(1, async {
            connected.disconnect().await;
            let answer = browser.open(&install).await.expect("browser navigates");
            assert_eq!(answer.status, 200, "{:?}", answer.body);
        })
        .await;
    assert_eq!(
        reclaimed.expect_err("the token is discarded").code(),
        conflict
    );
    assert_eq!(connected.view().await["state"], "active");
    minted(server, 1).await;

    let disconnected = connected.mint_while(1, connected.disconnect()).await;
    assert_eq!(
        disconnected.expect_err("the token is discarded").code(),
        conflict
    );
    refused(server, 1, "WYRD_GITHUB_404_NOT_CONNECTED").await;
}

/// Suspension follows GitHub's current state whatever order deliveries
/// arrive in: an older delivery after a newer one changes nothing, and an
/// observation GitHub answered before a newer applied one is refused for
/// redelivery. The view and token decisions match GitHub each time, and an
/// unchanged state keeps its public timestamp.
///
/// # Panics
/// Panics when the applied state or token decision differs from GitHub's.
#[tokio::test]
async fn suspension_follows_github_whatever_the_delivery_order() {
    let (connected, _browser) = Connected::start().await;
    let (mock, server) = (&connected.mock, &connected.server);
    let active = connected.view().await;

    assert_eq!(connected.installation_event("unsuspend").await, 202);
    assert_eq!(connected.installation_event("suspend").await, 202);
    let view = connected.view().await;
    assert_eq!(view["state"], "active", "GitHub never suspended it");
    assert_eq!(view["updated_at"], active["updated_at"]);
    minted(server, 1).await;

    mock.set_suspended(101, true);
    assert_eq!(connected.installation_event("suspend").await, 202);
    assert_eq!(connected.installation_event("unsuspend").await, 202);
    assert_eq!(connected.view().await["state"], "suspended");
    refused(server, 1, "WYRD_GITHUB_403_REPOSITORY_INACCESSIBLE").await;

    let hold = mock.hold_next_installation_read();
    let (older, ()) = tokio::join!(connected.installation_event("suspend"), async {
        hold.arrived().await;
        mock.set_suspended(101, false);
        assert_eq!(connected.installation_event("unsuspend").await, 202);
        hold.release();
    });
    assert_eq!(older, 409, "the overtaken observation is refused");
    assert_eq!(connected.view().await["state"], "active");
    minted(server, 1).await;
}

/// State saved under other App settings grants nothing: a slot saved for
/// another host projects no active access and asks GitHub for no token yet
/// still releases, a transaction begun under another OAuth client never
/// redirects to GitHub, and one whose callback changed is refused before
/// its code is exchanged. The served setup response is never cached, and a
/// fresh connection then succeeds.
///
/// # Panics
/// Panics when saved state is honored under replacement settings or the
/// setup response may be cached.
#[tokio::test]
async fn state_saved_under_other_app_settings_grants_nothing() {
    let (connected, mut browser) = Connected::start().await;
    let (mock, server) = (&connected.mock, &connected.server);

    connected
        .rewrite("UPDATE wyrd.github_installations SET host = 'github.old.example'")
        .await;
    let view = connected.view().await;
    assert_eq!(view["state"], "not_connected");
    assert_eq!(view["installation_id"], serde_json::Value::Null);
    let asked = mock.token_requests().len();
    refused(server, 1, "WYRD_GITHUB_404_NOT_CONNECTED").await;
    assert_eq!(mock.token_requests().len(), asked, "GitHub was not asked");
    connected.disconnect().await;

    let url = connected.connect().await;
    connected
        .rewrite(
            "UPDATE wyrd.auth_login_state SET client_id = 'Iv1.replaced' WHERE flow = 'github'",
        )
        .await;
    let answer = browser.open(&url).await.expect("browser navigates");
    assert_eq!(
        answer.problem_reason(),
        Some("transaction_invalid"),
        "{:?}",
        answer.body
    );
    assert!(
        !answer
            .visited
            .iter()
            .any(|hop| hop.path() == "/login/oauth/authorize"),
        "setup never redirected to GitHub: {:?}",
        answer.visited
    );

    let url = connected.connect().await;
    mock.sign_out();
    let stopped = browser.open(&url).await.expect("browser navigates");
    let authorize = stopped
        .visited
        .last()
        .filter(|hop| hop.path() == "/login/oauth/authorize")
        .expect("the browser stopped at GitHub's authorization")
        .clone();
    connected
        .rewrite(
            "UPDATE wyrd.auth_login_state \
                SET redirect_uri = 'https://old.example/v1/integrations/github/callback' \
              WHERE flow = 'github'",
        )
        .await;
    mock.sign_in("octo");
    let exchanges = mock.exchanges();
    let answer = browser
        .open(authorize.as_str())
        .await
        .expect("browser navigates");
    assert_eq!(
        answer.problem_reason(),
        Some("transaction_invalid"),
        "{:?}",
        answer.body
    );
    assert_eq!(mock.exchanges(), exchanges, "the code was never exchanged");

    let url = connected.connect().await;
    let state = url::Url::parse(&url)
        .expect("the install URL parses")
        .query_pairs()
        .find(|(name, _)| name == "state")
        .map(|(_, value)| value.into_owned())
        .expect("the install URL carries state");
    let setup = connected
        .send(
            Request::get(format!(
                "/v1/integrations/github/setup?installation_id=101&setup_action=install&state={state}"
            ))
            .body(Body::empty())
            .expect("request builds"),
        )
        .await;
    assert_eq!(setup.status(), 302);
    let header = |name: axum::http::HeaderName| {
        setup
            .headers()
            .get(name)
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default()
            .to_owned()
    };
    assert_eq!(header(axum::http::header::CACHE_CONTROL), "no-store");
    assert!(header(axum::http::header::SET_COOKIE).starts_with("wyrd_github_binding="));
    assert!(
        header(axum::http::header::LOCATION)
            .starts_with(&format!("{}login/oauth/authorize?", mock.origin())),
        "{}",
        header(axum::http::header::LOCATION)
    );

    let url = connected.connect().await;
    let answer = browser.open(&url).await.expect("browser navigates");
    assert_eq!(answer.status, 200, "{:?}", answer.body);
    assert_eq!(answer.body["state"], "active");
    minted(server, 1).await;
}
