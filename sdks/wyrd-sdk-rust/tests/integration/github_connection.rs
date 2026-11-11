//! A tenant administrator connects the deployment's GitHub App: Wyrd
//! associates an installation only after GitHub confirms it and the
//! authorizing user's access, refuses every spoofed, foreign, stale or
//! conflicting completion, follows GitHub's installation events, and never
//! returns a token, code or key.

/// Real-server and PostgreSQL cases, placed for the `pg-servers` test group.
mod pg_tests {
    use std::fmt::Debug;

    use serde_json::json;
    use wyrd_ai::cards::{Cards, ListCardsRequest};
    use wyrd_ai::github::{
        GithubConnection, GithubConnectionState, GithubConnectionView, GithubRepositoryAccess,
    };
    use wyrd_testing::WyrdTestServer;
    use wyrd_testing::github::{APP_ID, APP_SLUG, BrowserAnswer, GithubAppMock, GithubBrowser};

    use crate::support::Deployment;

    /// Installation `101`'s repositories.
    const ACME: [(u64, &str); 3] = [(1, "acme/api"), (2, "acme/web"), (3, "acme/vault")];

    /// A started GitHub mock with installations `101` (acme), `202` (beta) and
    /// another App's `303`.
    ///
    /// # Panics
    /// Panics when the mock cannot start.
    fn github() -> GithubAppMock {
        let mock = GithubAppMock::start().expect("the GitHub mock starts");
        mock.add_installation(101, &ACME);
        mock.add_installation(202, &[(9, "beta/app")]);
        mock.add_foreign_installation(303);
        mock
    }

    /// A deployment configured with `mock`'s App.
    ///
    /// # Panics
    /// Panics when the deployment cannot start.
    async fn deployment(mock: &GithubAppMock) -> Deployment {
        Deployment::start_with(WyrdTestServer::builder().with_github_app_for_test(mock)).await
    }

    /// One tenant principal: its SDK handle and its signed-in browser.
    struct Actor {
        /// The SDK handle.
        sdk: GithubConnection,
        /// The browser signed in to Wyrd as the same principal.
        browser: GithubBrowser,
    }

    /// Sign principal `key` of tenant `tenant` in through both surfaces.
    ///
    /// # Panics
    /// Panics when the handle cannot connect or the browser cannot sign in.
    async fn actor(deployment: &Deployment, tenant: &str, key: &str) -> Actor {
        Actor {
            sdk: GithubConnection::connect_with(&deployment.client(key))
                .await
                .expect("handle connects"),
            browser: GithubBrowser::sign_in(
                deployment.server().base_url().expect("bound"),
                tenant,
                key,
            )
            .await
            .expect("browser signs in"),
        }
    }

    /// The fixture tenant's slug.
    fn tenant(deployment: &Deployment) -> String {
        deployment.server().pg_fixture().tenant_slug().to_owned()
    }

    impl Actor {
        /// Begin from Wyrd and follow GitHub's install and authorization pages
        /// back to Wyrd's callback.
        ///
        /// # Panics
        /// Panics when the connection cannot begin or the browser cannot navigate.
        async fn connect(&mut self) -> BrowserAnswer {
            let receipt = self.sdk.begin().await.expect("connection begins");
            self.browser
                .open(receipt.authorization_url.as_str())
                .await
                .expect("browser navigates")
        }
    }

    /// The view a successful callback answered.
    ///
    /// # Panics
    /// Panics when the callback did not associate.
    fn associated(answer: &BrowserAnswer) -> GithubConnectionView {
        assert_eq!(answer.status, 200, "{:?}", answer.body);
        serde_json::from_value(answer.body.clone()).expect("callback answers a connection view")
    }

    /// Fail when the callback did not refuse with `status`, `code` and `reason`.
    ///
    /// # Panics
    /// Panics when the answer differs.
    fn refused(answer: &BrowserAnswer, status: u16, code: &str, reason: &str) {
        assert_eq!(
            (
                answer.status,
                answer.problem_code(),
                answer.problem_reason()
            ),
            (status, Some(code), Some(reason)),
            "{:?}",
            answer.body
        );
    }

    /// `ACME` repositories `ids`, as Wyrd reports them.
    fn repositories(ids: &[u64]) -> Vec<GithubRepositoryAccess> {
        ACME.iter()
            .filter(|(id, _)| ids.contains(id))
            .map(|(id, name)| GithubRepositoryAccess {
                repository_id: id.to_string(),
                full_name: (*name).to_owned(),
            })
            .collect()
    }

    /// Fail when any code, token or App secret the mock issued appears in
    /// `value`'s debug form.
    ///
    /// # Panics
    /// Panics when a secret appears.
    fn assert_redacted(mock: &GithubAppMock, value: &impl Debug) {
        let text = format!("{value:?}");
        for secret in mock.secrets() {
            assert!(!text.contains(&secret), "secret leaked: {text}");
        }
    }

    /// How many Cards the tenant holds.
    ///
    /// # Panics
    /// Panics when the list fails.
    async fn card_count(cards: &Cards) -> usize {
        cards
            .list(ListCardsRequest {
                kind: None,
                name: None,
                version_range: None,
                status: None,
                filter: None,
                include_prerelease: false,
                limit: None,
                cursor: None,
            })
            .await
            .expect("cards list")
            .items
            .len()
    }

    /// Begun from Wyrd and from GitHub, a connection associates only at the
    /// callback, with the repositories the authorizing user can reach; the
    /// callback is one-use, views carry no secret, no Card is created, and
    /// disconnect releases it.
    ///
    /// # Panics
    /// Panics when a flow does not associate as described or a secret leaks.
    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "requires the repository-managed Postgres journey lifecycle"]
    async fn connections_associate_only_after_github_confirms_them() {
        let mock = github();
        let deployment = deployment(&mock).await;
        let key = deployment.key("github_admin", &["admin"]).await;
        let mut admin = actor(&deployment, &tenant(&deployment), &key).await;
        let cards = deployment.cards().await;
        let cards_before = card_count(&cards).await;
        mock.add_user("octo", &[101, 202]);
        mock.hide_repository("octo", 3);
        mock.install_target(101);

        // Setup alone never associates: GitHub stops at authorization.
        mock.sign_out();
        let begun_at = chrono::Utc::now();
        let receipt = admin.sdk.begin().await.expect("connection begins");
        let ttl = receipt.expires_at - begun_at;
        assert!(
            ttl > chrono::TimeDelta::minutes(4) && ttl <= chrono::TimeDelta::minutes(5),
            "the transaction expires with the login-state lifetime: {ttl}"
        );
        let stopped = admin
            .browser
            .open(receipt.authorization_url.as_str())
            .await
            .expect("browser navigates");
        assert_eq!(stopped.status, 401, "{:?}", stopped.body);
        assert_eq!(
            admin.sdk.get().await.expect("view reads").state,
            GithubConnectionState::NotConnected
        );

        // Authorizing completes the same transaction.
        mock.sign_in("octo");
        let authorize = stopped
            .visited
            .last()
            .expect("authorization visited")
            .clone();
        let answer = admin
            .browser
            .open(authorize.as_str())
            .await
            .expect("browser navigates");
        let view = associated(&answer);
        assert_eq!(view.host, mock.host());
        assert_eq!((view.app.id, view.app.slug.as_str()), (APP_ID, APP_SLUG));
        assert_eq!(view.state, GithubConnectionState::Active);
        assert_eq!(view.installation_id, Some(101));
        assert_eq!(view.repositories, repositories(&[1, 2]));
        assert!(view.connected_at.is_some() && view.updated_at.is_some());
        assert_eq!(admin.sdk.get().await.expect("view reads"), view);
        assert!(!admin.browser.has_cookie("wyrd_github_binding"));

        // The callback is one-use.
        let callback = answer.callback_url().expect("callback visited").clone();
        let replay = admin
            .browser
            .open(callback.as_str())
            .await
            .expect("browser navigates");
        refused(
            &replay,
            403,
            "WYRD_GITHUB_403_CONNECTION_REFUSED",
            "transaction_invalid",
        );

        let released = admin.sdk.disconnect().await.expect("disconnects");
        assert_eq!(released.state, GithubConnectionState::NotConnected);
        assert_eq!(released.installation_id, None);
        assert!(released.repositories.is_empty());
        let again = admin
            .sdk
            .disconnect()
            .await
            .expect_err("nothing left to disconnect");
        assert_eq!(again.code(), "WYRD_GITHUB_404_NOT_CONNECTED");

        // Begun on GitHub: setup without state starts the transaction.
        let begun = admin
            .browser
            .open(&format!(
                "{}apps/{APP_SLUG}/installations/new",
                mock.origin()
            ))
            .await
            .expect("browser navigates");
        let github_begun = associated(&begun);
        assert_eq!(github_begun.installation_id, Some(101));
        assert_eq!(github_begun.repositories, repositories(&[1, 2]));

        assert_redacted(
            &mock,
            &(
                &stopped.body,
                &answer.body,
                &view,
                &replay.body,
                &released,
                &again,
                &begun.body,
            ),
        );
        assert_eq!(
            card_count(&cards).await,
            cards_before,
            "a connection creates no Card"
        );
        deployment.shutdown().await;
    }

    /// Spoofed and foreign installations, foreign principals and tenants,
    /// stale transactions and both directions of uniqueness are refused, and
    /// none of them changes an association.
    ///
    /// # Panics
    /// Panics when a refusal differs or a refused flow associates.
    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "requires the repository-managed Postgres journey lifecycle"]
    async fn spoofed_foreign_stale_and_conflicting_connections_are_refused() {
        let mock = github();
        let deployment = deployment(&mock).await;
        let slug = tenant(&deployment);
        let key = deployment.key("github_admin", &["admin"]).await;
        let mut admin = actor(&deployment, &slug, &key).await;
        let other_key = deployment.key("github_other_admin", &["admin"]).await;
        let mut other_principal = actor(&deployment, &slug, &other_key).await;
        let tenant_b_key = deployment.other_tenant_admin("github-b").await;
        let mut tenant_b = actor(&deployment, "github-b", &tenant_b_key).await;
        let viewer = GithubConnection::connect_with(
            &deployment.client(&deployment.key("github_viewer", &["viewer"]).await),
        )
        .await
        .expect("handle connects");

        let denied = viewer.begin().await.expect_err("a viewer cannot connect");
        assert_eq!(denied.code(), "WYRD_PERMISSION_403_DENIED_RBAC");
        let denied_read = viewer.get().await.expect_err("a viewer cannot read");
        assert_eq!(denied_read.code(), "WYRD_PERMISSION_403_DENIED_RBAC");

        // A hint the user cannot access, and another App's installation.
        mock.add_user("mallory", &[202, 303]);
        mock.sign_in("mallory");
        for target in [101, 303] {
            mock.install_target(target);
            refused(
                &admin.connect().await,
                403,
                "WYRD_GITHUB_403_CONNECTION_REFUSED",
                "installation_inaccessible",
            );
        }

        // Another principal's, or another tenant's, browser cannot bind it.
        mock.add_user("octo", &[101, 202]);
        mock.sign_in("octo");
        mock.install_target(101);
        for intruder in [&mut other_principal, &mut tenant_b] {
            let receipt = admin.sdk.begin().await.expect("connection begins");
            refused(
                &intruder
                    .browser
                    .open(receipt.authorization_url.as_str())
                    .await
                    .expect("browser navigates"),
                403,
                "WYRD_GITHUB_403_CONNECTION_REFUSED",
                "transaction_invalid",
            );
        }
        let base = deployment.server().base_url().expect("bound").to_owned();
        refused(
            &admin
                .browser
                .open(&format!(
                    "{base}/v1/integrations/github/setup?installation_id=101&state=unknown"
                ))
                .await
                .expect("browser navigates"),
            403,
            "WYRD_GITHUB_403_CONNECTION_REFUSED",
            "transaction_invalid",
        );
        assert_eq!(
            admin.sdk.get().await.expect("view reads").state,
            GithubConnectionState::NotConnected
        );

        // A code GitHub never issued is refused, and the transaction is spent.
        mock.sign_out();
        let stopped = admin.connect().await;
        assert_eq!(stopped.status, 401, "{:?}", stopped.body);
        let authorize = stopped.visited.last().expect("authorization visited");
        let forged = authorize
            .query_pairs()
            .find(|(name, _)| name == "state")
            .map(|(_, state)| {
                format!("{base}/v1/integrations/github/callback?code=forged&state={state}")
            })
            .expect("authorization carries the state");
        refused(
            &admin
                .browser
                .open(&forged)
                .await
                .expect("browser navigates"),
            403,
            "WYRD_GITHUB_403_CONNECTION_REFUSED",
            "authorization_refused",
        );
        mock.sign_in("octo");
        refused(
            &admin
                .browser
                .open(authorize.as_str())
                .await
                .expect("browser navigates"),
            403,
            "WYRD_GITHUB_403_CONNECTION_REFUSED",
            "transaction_invalid",
        );

        // The OIDC sign-in callback cannot consume a GitHub transaction.
        let receipt = admin.sdk.begin().await.expect("connection begins");
        let state = receipt
            .authorization_url
            .as_str()
            .split_once("state=")
            .map(|(_, state)| state.split('&').next().unwrap_or_default().to_owned())
            .expect("the install URL carries the state");
        let cross_flow = admin
            .browser
            .open(&format!("{base}/auth/callback?state={state}&code=forged"))
            .await
            .expect("browser navigates");
        assert_eq!(
            (cross_flow.status, cross_flow.problem_code()),
            (400, Some("WYRD_AUTH_400_INVALID_STATE")),
            "{:?}",
            cross_flow.body
        );

        // One installation per tenant and one tenant per installation.
        let connected = associated(
            &admin
                .browser
                .open(receipt.authorization_url.as_str())
                .await
                .expect("browser navigates"),
        );
        assert_eq!(connected.installation_id, Some(101));
        refused(
            &tenant_b.connect().await,
            409,
            "WYRD_GITHUB_409_CONNECTION_CONFLICT",
            "installation_claimed",
        );
        mock.install_target(202);
        refused(
            &admin.connect().await,
            409,
            "WYRD_GITHUB_409_CONNECTION_CONFLICT",
            "tenant_connected",
        );

        // A transaction begun before a disconnect and reconnect is stale.
        mock.install_target(101);
        let stale = admin.sdk.begin().await.expect("connection begins");
        admin.sdk.disconnect().await.expect("disconnects");
        let reconnected = associated(&admin.connect().await);
        refused(
            &admin
                .browser
                .open(stale.authorization_url.as_str())
                .await
                .expect("browser navigates"),
            409,
            "WYRD_GITHUB_409_CONNECTION_CONFLICT",
            "connection_changed",
        );
        assert_eq!(admin.sdk.get().await.expect("view reads"), reconnected);
        assert_eq!(
            tenant_b.sdk.get().await.expect("view reads").state,
            GithubConnectionState::NotConnected
        );

        // Reconnecting the same installation refreshes its repositories.
        mock.hide_repository("octo", 2);
        let refreshed = associated(&admin.connect().await);
        assert_eq!(refreshed.installation_id, Some(101));
        assert_eq!(refreshed.repositories, repositories(&[1, 3]));

        // After a disconnect the installation is free for another tenant.
        admin.sdk.disconnect().await.expect("disconnects");
        let claimed = associated(&tenant_b.connect().await);
        assert_eq!(claimed.installation_id, Some(101));
        assert_eq!(
            admin.sdk.get().await.expect("view reads").state,
            GithubConnectionState::NotConnected
        );
        deployment.shutdown().await;
    }

    /// Signed installation events suspend and resume the connection, drop
    /// removed repositories and release a deleted installation, which another
    /// tenant may then connect; unsigned and missigned deliveries are refused.
    ///
    /// # Panics
    /// Panics when an event is not applied as described.
    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "requires the repository-managed Postgres journey lifecycle"]
    async fn installation_events_synchronize_the_connection() {
        let mock = github();
        let deployment = deployment(&mock).await;
        let key = deployment.key("github_admin", &["admin"]).await;
        let mut admin = actor(&deployment, &tenant(&deployment), &key).await;
        let tenant_b_key = deployment.other_tenant_admin("github-b").await;
        let mut tenant_b = actor(&deployment, "github-b", &tenant_b_key).await;
        let base = deployment.server().base_url().expect("bound").to_owned();
        mock.add_user("octo", &[101]);
        mock.sign_in("octo");
        mock.install_target(101);
        let connected = associated(&admin.connect().await);
        assert_eq!(connected.repositories, repositories(&[1, 2, 3]));
        let event = |action: &str| json!({ "action": action, "installation": { "id": 101 } });

        for signature in [None, Some("sha256=00"), Some("not-a-signature")] {
            let status = mock
                .deliver_with_signature(&base, "installation", &event("deleted"), signature)
                .await
                .expect("delivery sends");
            assert_eq!(status, 401, "{signature:?}");
        }
        assert_eq!(admin.sdk.get().await.expect("view reads"), connected);

        mock.set_suspended(101, true);
        assert_eq!(
            mock.deliver(&base, "installation", &event("suspend"))
                .await
                .expect("delivery sends"),
            202
        );
        let suspended = admin.sdk.get().await.expect("view reads");
        assert_eq!(suspended.state, GithubConnectionState::Suspended);
        assert_eq!(suspended.installation_id, Some(101));
        refused(
            &tenant_b.connect().await,
            403,
            "WYRD_GITHUB_403_CONNECTION_REFUSED",
            "installation_suspended",
        );
        mock.set_suspended(101, false);
        mock.deliver(&base, "installation", &event("unsuspend"))
            .await
            .expect("delivery sends");
        assert_eq!(
            admin.sdk.get().await.expect("view reads").state,
            GithubConnectionState::Active
        );

        mock.deliver(
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
        mock.deliver(&base, "push", &json!({ "installation": { "id": 101 } }))
            .await
            .expect("delivery sends");
        assert_eq!(
            admin.sdk.get().await.expect("view reads").repositories,
            repositories(&[1, 3])
        );

        mock.deliver(&base, "installation", &event("deleted"))
            .await
            .expect("delivery sends");
        assert_eq!(
            admin.sdk.get().await.expect("view reads").state,
            GithubConnectionState::NotConnected
        );
        let claimed = associated(&tenant_b.connect().await);
        assert_eq!(claimed.installation_id, Some(101));
        deployment.shutdown().await;
    }

    /// Without a configured App only GitHub is unavailable: every GitHub
    /// operation and the webhook refuse with a typed error and other reads work.
    ///
    /// # Panics
    /// Panics when a GitHub operation succeeds or another read fails.
    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "requires the repository-managed Postgres journey lifecycle"]
    async fn an_unconfigured_deployment_refuses_only_github() {
        let deployment = Deployment::start().await;
        let admin = GithubConnection::connect_with(&deployment.admin())
            .await
            .expect("handle connects");
        for refusal in [
            admin.begin().await.map(|_| ()),
            admin.get().await.map(|_| ()),
            admin.disconnect().await.map(|_| ()),
        ] {
            assert_eq!(
                refusal.expect_err("GitHub is not configured").code(),
                "WYRD_GITHUB_503_NOT_CONFIGURED"
            );
        }
        let status = reqwest::Client::new()
            .post(format!(
                "{}/v1/integrations/github/webhook",
                deployment.server().base_url().expect("bound")
            ))
            .body("{}")
            .send()
            .await
            .expect("delivery sends")
            .status();
        assert_eq!(status, 503);
        card_count(&deployment.cards().await).await;
        deployment.shutdown().await;
    }
}
