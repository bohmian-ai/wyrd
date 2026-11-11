//! A tenant declares its connected repository's review policy from
//! `wyrd.toml` or a Repository Card: Wyrd verifies the repository against the
//! GitHub connection, registers only the listed Verifiers, and selects the
//! declared version only once the declaration completes.

/// Real-server and PostgreSQL cases, placed for the `pg-servers` test group.
mod pg_tests {
    use std::path::Path;

    use wyrd_ai::cards::{CardKind, CardRef, CardSelector, Cards, DeclarationReceipt};
    use wyrd_ai::cli;
    use wyrd_ai::github::{GithubConnection, GithubConnectionState};
    use wyrd_spec::envelope::RepositoryStatus;
    use wyrd_testing::WyrdTestServer;
    use wyrd_testing::github::{GithubAppMock, GithubBrowser};

    use crate::support::{Deployment, declare, declared, fixture};

    /// A deployment whose fixture tenant is connected to installation `101`
    /// with `acme/api` (1) and `acme/web` (2) selected, its connection
    /// handle, and the connecting administrator's key.
    ///
    /// # Panics
    /// Panics when the deployment cannot start or the connection fails.
    async fn connected() -> (GithubAppMock, Deployment, GithubConnection, String) {
        let mock = GithubAppMock::start().expect("the GitHub mock starts");
        mock.add_installation(101, &[(1, "acme/api"), (2, "acme/web")]);
        mock.add_user("octo", &[101]);
        mock.sign_in("octo");
        mock.install_target(101);
        let deployment =
            Deployment::start_with(WyrdTestServer::builder().with_github_app_for_test(&mock)).await;
        let key = deployment.key("github_admin", &["admin"]).await;
        let connection = GithubConnection::connect_with(&deployment.client(&key))
            .await
            .expect("handle connects");
        associate(&deployment, &connection, &key).await;
        (mock, deployment, connection, key)
    }

    /// Connect the fixture tenant through GitHub's pages as principal `key`.
    ///
    /// # Panics
    /// Panics when the callback does not associate.
    async fn associate(deployment: &Deployment, connection: &GithubConnection, key: &str) {
        let receipt = connection.begin().await.expect("connection begins");
        let mut browser = GithubBrowser::sign_in(
            deployment.server().base_url().expect("bound"),
            deployment.server().pg_fixture().tenant_slug(),
            key,
        )
        .await
        .expect("browser signs in");
        let answer = browser
            .open(receipt.authorization_url.as_str())
            .await
            .expect("browser navigates");
        assert_eq!(answer.status, 200, "{:?}", answer.body);
    }

    /// A checkout of the checked-in `repository_declaration` fixtures whose
    /// origin is `full_name` on the mock's host.
    ///
    /// # Panics
    /// Panics when the copy cannot be written.
    fn checkout(mock: &GithubAppMock, full_name: &str) -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("checkout directory creates");
        copy(&fixture("cards/repository_declaration"), dir.path());
        std::fs::create_dir(dir.path().join(".git")).expect(".git creates");
        std::fs::write(
            dir.path().join(".git/config"),
            format!(
                "[remote \"origin\"]\n\turl = {}{full_name}.git\n",
                mock.origin()
            ),
        )
        .expect("origin writes");
        dir
    }

    /// Copy the regular files of `from` into `to`, recursively.
    ///
    /// # Panics
    /// Panics when a file cannot be copied.
    fn copy(from: &Path, to: &Path) {
        for entry in std::fs::read_dir(from).expect("fixture reads") {
            let entry = entry.expect("fixture entry reads");
            let target = to.join(entry.file_name());
            if entry.path().is_dir() {
                std::fs::create_dir(&target).expect("fixture directory copies");
                copy(&entry.path(), &target);
            } else {
                std::fs::copy(entry.path(), target).expect("fixture file copies");
            }
        }
    }

    /// Make `policies/<variant>.toml` the checkout's `wyrd.toml` and declare it.
    ///
    /// # Errors
    /// Returns the declaration's refusal.
    async fn declare_toml(
        cards: &Cards,
        checkout: &tempfile::TempDir,
        variant: &str,
    ) -> Result<DeclarationReceipt, wyrd_ai::WyrdError> {
        cards.declare_from_path(policy(checkout, variant)).await
    }

    /// Make `policies/<variant>.toml` the checkout's `wyrd.toml` and return
    /// its path.
    ///
    /// # Panics
    /// Panics when the policy cannot be copied.
    fn policy(checkout: &tempfile::TempDir, variant: &str) -> std::path::PathBuf {
        let toml = checkout.path().join("wyrd.toml");
        std::fs::copy(
            checkout
                .path()
                .join("policies")
                .join(format!("{variant}.toml")),
            &toml,
        )
        .expect("policy copies");
        toml
    }

    /// The served Repository status and binding count of `card`.
    ///
    /// # Panics
    /// Panics when the read fails or `card` is not a Repository.
    async fn repository(cards: &Cards, card: &CardRef) -> (RepositoryStatus, usize) {
        let status = cards
            .get(CardSelector::exact(card.clone()))
            .await
            .expect("the Repository reads")
            .status
            .expect("the Repository serves status");
        let bindings = status
            .verification
            .map_or(0, |verification| verification.binding_ids.len());
        (
            status.repository.expect("a Repository serves selection"),
            bindings,
        )
    }

    /// Minimal `wyrd.toml` declares an inferred Repository root with only
    /// its listed Verifiers, selects it once complete, reuses unchanged
    /// content, auto-versions changed content, refuses changed content at an
    /// exact pin, rolls back to an old exact version on purpose, keeps the
    /// selection over a failed declaration, and selects an empty policy
    /// without disconnecting.
    ///
    /// # Panics
    /// Panics when a declaration or selection differs from that.
    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "requires the repository-managed Postgres journey lifecycle"]
    async fn toml_policy_is_selected_only_once_declared() {
        let (mock, deployment, connection, _key) = connected().await;
        let cards = deployment.cards().await;
        let answer_check = declared(
            &declare(&cards, "cards/verify_in_real_time/answer-check.yaml").await,
            "answer-check",
        );
        let checkout = checkout(&mock, "acme/api");

        let first = declare_toml(&cards, &checkout, "minimal")
            .await
            .expect("minimal wyrd.toml declares");
        assert_eq!(first.root.kind, CardKind::Repository);
        assert_eq!(first.root.name.as_str(), "api");
        let names: Vec<_> = first
            .outcomes
            .iter()
            .map(|outcome| outcome.card_ref.name.as_str())
            .collect();
        assert_eq!(names, ["code-review", "api"], "unrelated YAML stays out");
        let (status, bindings) = repository(&cards, &first.root).await;
        assert!(status.selected);
        assert_eq!(status.selected_version.as_ref(), Some(&first.root.version));
        assert_eq!(status.full_name.as_deref(), Some("acme/api"));
        assert_eq!(bindings, 1);

        let again = declare_toml(&cards, &checkout, "minimal")
            .await
            .expect("unchanged wyrd.toml declares");
        assert_eq!(again.root, first.root, "unchanged content reuses");

        let changed = declare_toml(&cards, &checkout, "changed")
            .await
            .expect("changed wyrd.toml declares");
        assert_ne!(changed.root.version, first.root.version);
        let (status, bindings) = repository(&cards, &changed.root).await;
        assert!(status.selected);
        assert_eq!(bindings, 2);
        let (replaced, _) = repository(&cards, &first.root).await;
        assert!(
            !replaced.selected,
            "the replaced version is Active, not selected"
        );
        assert_eq!(
            replaced.selected_version,
            Some(changed.root.version.clone())
        );

        let pinned = declare_toml(&cards, &checkout, "pinned")
            .await
            .expect_err("changed content at an existing exact version conflicts");
        assert_eq!(pinned.code(), "WYRD_REGISTRY_409_SPEC_DRIFT");
        let missing = declare_toml(&cards, &checkout, "missing")
            .await
            .expect_err("a missing registered Verifier is refused");
        assert_eq!(missing.code(), "WYRD_REGISTRY_422_UNRESOLVED_DEPENDENCY");
        assert!(repository(&cards, &changed.root).await.0.selected);
        cards
            .get(CardSelector::exact(answer_check.clone()))
            .await
            .expect("the registered dependency survives");

        let rollback = declare_toml(&cards, &checkout, "rollback")
            .await
            .expect("the old exact version declares again");
        assert_eq!(rollback.root, first.root);
        assert!(repository(&cards, &first.root).await.0.selected);
        assert!(!repository(&cards, &changed.root).await.0.selected);

        let empty = declare_toml(&cards, &checkout, "empty")
            .await
            .expect("an empty policy declares");
        assert_eq!(empty.outcomes.len(), 1);
        let (status, bindings) = repository(&cards, &empty.root).await;
        assert!(status.selected);
        assert_eq!(bindings, 0);
        assert_eq!(
            connection.get().await.expect("connection reads").state,
            GithubConnectionState::Active
        );
        deployment.shutdown().await;
    }

    /// A YAML Repository declares and selects through the same lifecycle,
    /// identified by its repository id, and its `pull_request` activation is
    /// refused on any other subject.
    ///
    /// # Panics
    /// Panics when the YAML Repository is not selected.
    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "requires the repository-managed Postgres journey lifecycle"]
    async fn yaml_repository_declares_through_the_same_lifecycle() {
        let (_mock, deployment, _connection, _key) = connected().await;
        let cards = deployment.cards().await;
        let receipt = declare(&cards, "cards/repository_declaration/repository.yaml").await;
        assert_eq!(receipt.root.kind, CardKind::Repository);
        assert_eq!(receipt.root.name.as_str(), "web");
        let (status, bindings) = repository(&cards, &receipt.root).await;
        assert!(status.selected);
        assert_eq!(status.full_name.as_deref(), Some("acme/web"));
        assert_eq!(bindings, 1);
        let off = cards
            .declare_from_path(&fixture("cards/repository_declaration/off-repository.yaml"))
            .await
            .expect_err("pull_request activates only a Repository binding");
        assert_eq!(off.code(), "WYRD_SPEC_400_TRIGGER_ACTIVATION_MISMATCH");
        deployment.shutdown().await;
    }

    /// The remote is only a hint: an unselected repository is refused, an
    /// explicit name cannot fork an established family, and a provider
    /// rename keeps the family name.
    ///
    /// # Panics
    /// Panics when an identity is accepted or refused differently.
    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "requires the repository-managed Postgres journey lifecycle"]
    async fn repository_identity_is_verified_by_the_connection() {
        let (mock, deployment, connection, key) = connected().await;
        let cards = deployment.cards().await;

        let unselected = declare_toml(&cards, &checkout(&mock, "acme/vault"), "minimal")
            .await
            .expect_err("an unselected repository is refused");
        assert_eq!(
            unselected.code(),
            "WYRD_GITHUB_403_REPOSITORY_INACCESSIBLE",
            "{unselected}"
        );

        let api = checkout(&mock, "acme/api");
        let first = declare_toml(&cards, &api, "minimal")
            .await
            .expect("minimal wyrd.toml declares");
        let named = declare_toml(&cards, &api, "named")
            .await
            .expect_err("a second family name is refused");
        assert_eq!(
            named.code(),
            "WYRD_REGISTRY_409_REPOSITORY_IDENTITY_CONFLICT"
        );

        mock.add_installation(101, &[(1, "acme/api-renamed"), (2, "acme/web")]);
        connection.disconnect().await.expect("connection releases");
        associate(&deployment, &connection, &key).await;
        let renamed = declare_toml(&cards, &checkout(&mock, "acme/api-renamed"), "minimal")
            .await
            .expect("the renamed repository declares");
        assert_eq!(renamed.root.name, first.root.name, "the family survives");
        assert_eq!(
            repository(&cards, &renamed.root)
                .await
                .0
                .full_name
                .as_deref(),
            Some("acme/api-renamed")
        );
        deployment.shutdown().await;
    }

    /// `wyrd declare wyrd.toml` checks offline with the server checks
    /// labelled deferred, refuses an invalid local Verifier before resolving
    /// any credential, and declares the same selected Repository.
    ///
    /// # Panics
    /// Panics when the CLI checks, refuses or declares differently.
    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "requires the repository-managed Postgres journey lifecycle"]
    async fn cli_checks_offline_and_declares_the_policy() {
        let (mock, deployment, _connection, _key) = connected().await;
        let checkout = checkout(&mock, "acme/api");

        let report = cli::declare_check(policy(&checkout, "minimal")).expect("checks offline");
        let names: Vec<_> = report.cards.iter().map(|card| card.name.as_str()).collect();
        assert_eq!(names, ["code-review", "api"]);
        assert_eq!(
            report.deferred,
            [
                "repository_identity",
                "registered_references",
                "authorization",
                "version_assignment"
            ]
        );
        let broken = cli::declare(policy(&checkout, "broken"), None)
            .await
            .expect_err("a missing local Verifier is refused locally");
        assert_eq!(
            broken.code(),
            "WYRD_LOADER_400_INVALID_ENVELOPE",
            "{broken}"
        );

        let receipt = cli::declare(policy(&checkout, "minimal"), Some(deployment.admin()))
            .await
            .expect("wyrd declare wyrd.toml declares");
        assert_eq!(receipt.root.kind, CardKind::Repository);
        assert!(
            repository(&deployment.cards().await, &receipt.root)
                .await
                .0
                .selected
        );
        deployment.shutdown().await;
    }
}
