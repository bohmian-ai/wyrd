//! CI publishes a deferred input after review starts: the pull request's
//! execution waits for it, `Evidence::publish` supplies it for the
//! repository commit, and the same execution resumes and passes. The receipt
//! is durable: a replay answers the same identity, and different evidence or
//! an unconnected repository is refused. A media input is a file uploaded by
//! the publish itself. A pull request may also wait for a person: another
//! principal approves the pending signoff with `Execution::decide`, and the
//! same execution passes.

/// Real-server and PostgreSQL cases, placed for the `pg-servers` test group.
mod pg_tests {
    use std::time::{Duration, Instant};

    use wyrd_ai::MediaFormat;
    use wyrd_ai::WyrdClient;
    use wyrd_ai::cards::CardSelector;
    use wyrd_ai::github::GithubConnection;
    use wyrd_ai::verify::{
        ApprovalDecision, ApprovalStatus, Evidence, EvidenceStatus, Execution, File,
    };
    use wyrd_spec::verification::{VerificationExecutionStatus, VerificationVerdict};
    use wyrd_testing::WyrdTestServer;
    use wyrd_testing::github::{GithubAppMock, GithubBrowser, PullRequest};

    use crate::support::{Deployment, fixture};

    /// `name` in the "Publish CI results" guide's example directory.
    fn example(name: &str) -> std::path::PathBuf {
        std::path::Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../examples/publish-deferred-evidence"
        ))
        .join(name)
    }

    /// The head commit of pull request 7.
    fn head() -> String {
        "a1".repeat(20)
    }

    /// A deployment whose administrator connected installation 101
    /// (`acme/api`, `acme/web`) and declared the Repository file at
    /// `repository`, with pull request 7 of `acme/web` opened at [`head`].
    struct Opened {
        /// The running deployment.
        deployment: Deployment,
        /// The administrator's client.
        client: WyrdClient,
        /// The pull request's execution, waiting for `waiting`.
        execution: Execution,
        /// The GitHub mock, kept alive with the deployment.
        _mock: GithubAppMock,
    }

    impl Opened {
        /// Connect, declare `repository`, open the pull request, and wait
        /// until its execution waits for exactly `input`.
        ///
        /// # Panics
        /// Panics when a setup step is refused or the execution does not
        /// wait for `input` within 30 seconds.
        async fn waiting_for(repository: &std::path::Path, input: &str) -> Self {
            let mock = GithubAppMock::start().expect("the GitHub mock starts");
            mock.add_installation(101, &[(1, "acme/api"), (2, "acme/web")]);
            mock.add_user("octo", &[101]);
            mock.sign_in("octo");
            mock.install_target(101);
            let deployment = Deployment::start_with(
                WyrdTestServer::builder()
                    .with_github_app_for_test(&mock)
                    .with_verification_runtime_for_test(),
            )
            .await;
            let key = deployment.key("evidence_admin", &["admin"]).await;
            let client = deployment.client(&key);
            let receipt = GithubConnection::connect_with(&client)
                .await
                .expect("handle connects")
                .begin()
                .await
                .expect("connection begins");
            let mut browser = GithubBrowser::sign_in(
                deployment.server().base_url().expect("bound"),
                deployment.server().pg_fixture().tenant_slug(),
                &key,
            )
            .await
            .expect("browser signs in");
            let answer = browser
                .open(receipt.authorization_url.as_str())
                .await
                .expect("browser navigates");
            assert_eq!(answer.status, 200, "{:?}", answer.body);

            let cards = deployment.cards().await;
            let root = cards
                .declare_from_path(repository)
                .await
                .expect("the Repository declares")
                .root;
            let binding = cards
                .get(CardSelector::exact(root))
                .await
                .expect("the Repository reads")
                .status
                .and_then(|status| status.verification)
                .expect("the Repository serves its bindings")
                .binding_ids[0];
            mock.set_pull_request(2, 7, PullRequest::open(2, &"b1".repeat(20), &head()));
            let payload = mock.pull_request_event(101, 2, 7, "opened");
            let status = mock
                .deliver(
                    deployment.server().base_url().expect("bound"),
                    "pull_request",
                    &payload,
                )
                .await
                .expect("delivery sends");
            assert_eq!(status, 202);
            let id = deployment
                .binding_status(&binding)
                .await
                .last_execution_id
                .expect("the binding has an execution");
            let execution = Execution::connect_with(id, &client)
                .await
                .expect("the execution handle connects");
            let deadline = Instant::now() + Duration::from_secs(30);
            let waiting = loop {
                let state = execution.status().await.expect("the execution reads");
                if state.status == VerificationExecutionStatus::Waiting {
                    break state;
                }
                assert!(Instant::now() < deadline, "{state:?}");
                tokio::time::sleep(Duration::from_millis(100)).await;
            };
            assert_eq!(
                waiting
                    .waiting
                    .iter()
                    .map(wyrd_spec::ids::SlotName::as_str)
                    .collect::<Vec<_>>(),
                [input]
            );
            Self {
                deployment,
                client,
                execution,
                _mock: mock,
            }
        }

        /// The evidence publisher for the administrator.
        ///
        /// # Panics
        /// Panics when the handle cannot connect.
        async fn evidence(&self) -> Evidence {
            Evidence::connect_with(&self.client)
                .await
                .expect("the publisher connects")
        }

        /// Wait for the resumed execution and return its verdict.
        ///
        /// # Panics
        /// Panics when it does not complete within 30 seconds.
        async fn verdict(&self) -> VerificationVerdict {
            self.execution
                .wait(Duration::from_secs(30))
                .await
                .expect("the resumed execution completes")
                .outcome
                .verdict
        }
    }

    /// Publishing `ci` resumes the waiting execution under its pin; the
    /// receipt replays, and conflicting evidence or an unconnected
    /// repository is refused.
    ///
    /// # Panics
    /// Panics when the execution does not wait, resume and pass, or a
    /// receipt or refusal differs.
    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "requires the repository-managed Postgres journey lifecycle"]
    async fn published_evidence_resumes_the_waiting_execution() {
        let head = head();
        let opened =
            Opened::waiting_for(&fixture("cards/deferred_evidence/repository.yaml"), "ci").await;
        let evidence = opened.evidence().await;
        let green = serde_json::json!({ "status": "green" });
        let accepted = evidence
            .publish("ci", green.clone(), Some("acme/web"), Some(&head))
            .await
            .expect("the evidence is accepted");
        assert_eq!(accepted.status, EvidenceStatus::Accepted);
        assert_eq!(accepted.matched_subscriptions, 1);
        assert_eq!(accepted.repository.repository_id.as_str(), "2");
        assert_eq!(accepted.tags.commit.as_str(), head);
        assert_eq!(opened.verdict().await, VerificationVerdict::Passed);

        let replayed = evidence
            .publish("ci", green, Some("acme/web"), Some(&head))
            .await
            .expect("the same evidence replays");
        assert_eq!(replayed.status, EvidenceStatus::Replayed);
        assert_eq!(
            (replayed.evidence_id, replayed.recorded_at),
            (accepted.evidence_id, accepted.recorded_at)
        );
        for (value, repository, code) in [
            ("red", "acme/web", "WYRD_VERIFICATION_409_EVIDENCE_CONFLICT"),
            (
                "green",
                "acme/vault",
                "WYRD_GITHUB_403_REPOSITORY_INACCESSIBLE",
            ),
        ] {
            let refused = evidence
                .publish(
                    "ci",
                    serde_json::json!({ "status": value }),
                    Some(repository),
                    Some(&head),
                )
                .await
                .expect_err("the publication is refused");
            assert_eq!(refused.code(), code);
        }
        opened.deployment.shutdown().await;
    }

    /// Publishing a file for the media `report` input uploads it, resumes the
    /// waiting execution, which reads it, and replays; a file of a format
    /// the slot does not accept is refused before anything is stored.
    ///
    /// # Panics
    /// Panics when the upload is not accepted, the execution does not read
    /// it and pass, or the replay or refusal differs.
    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "requires the repository-managed Postgres journey lifecycle"]
    async fn published_file_resumes_the_waiting_execution() {
        let head = head();
        let opened = Opened::waiting_for(&example("repository.yaml"), "report").await;
        let evidence = opened.evidence().await;
        let report = example("report.csv");
        let refused = evidence
            .publish(
                "report",
                File::bytes(b"{}".to_vec()).with_format(MediaFormat::Json),
                Some("acme/web"),
                Some(&head),
            )
            .await
            .expect_err("a JSON file is not a CSV report");
        assert_eq!(refused.code(), "WYRD_VERIFICATION_400_INPUT_INVALID");

        let accepted = evidence
            .publish(
                "report",
                File::path(report.clone()),
                Some("acme/web"),
                Some(&head),
            )
            .await
            .expect("the report is accepted");
        assert_eq!(accepted.status, EvidenceStatus::Accepted);
        assert_eq!(accepted.matched_subscriptions, 1);
        assert_eq!(opened.verdict().await, VerificationVerdict::Passed);

        let replayed = evidence
            .publish("report", File::path(report), Some("acme/web"), Some(&head))
            .await
            .expect("the same report replays");
        assert_eq!(replayed.status, EvidenceStatus::Replayed);
        assert_eq!(replayed.evidence_id, accepted.evidence_id);
        opened.deployment.shutdown().await;
    }

    /// Another principal approves the pending signoff once CI is green, and
    /// the same execution passes.
    ///
    /// The execution waits for `ci` and lists the pending `signoff`
    /// occurrence. Published CI resumes it, but it still waits for the
    /// approval alone. A viewer's and an editor's decisions are refused. An
    /// approver bound only on the shared Verifier reads the Repository-owned
    /// execution as unknown and cannot decide it; with a separate read of the
    /// Repository family it reads the pending approval, and its approval
    /// returns the durable receipt, replays on retry, and the execution passes
    /// under its original ID with the decision listed.
    ///
    /// # Panics
    /// Panics when the occurrence is not pending, a decision or refusal
    /// differs, or the execution does not pass.
    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "requires the repository-managed Postgres journey lifecycle"]
    async fn a_person_approves_the_pending_signoff() {
        let opened = Opened::waiting_for(&fixture("cards/approvals/repository.yaml"), "ci").await;
        let state = opened
            .execution
            .status()
            .await
            .expect("the execution reads");
        let [pending] = state.approvals.as_slice() else {
            panic!("one approval occurrence: {:?}", state.approvals);
        };
        assert_eq!(pending.step_id.as_str(), "signoff");
        assert_eq!(pending.message, "Ship this change?");
        assert_eq!(pending.status, ApprovalStatus::Pending);
        let approval = pending.approval_id;
        opened
            .evidence()
            .await
            .publish(
                "ci",
                serde_json::json!({ "status": "green" }),
                Some("acme/web"),
                Some(&head()),
            )
            .await
            .expect("CI is accepted");
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            let state = opened
                .execution
                .status()
                .await
                .expect("the execution reads");
            if state.status == VerificationExecutionStatus::Waiting && state.waiting.is_empty() {
                break;
            }
            assert!(Instant::now() < deadline, "{state:?}");
            tokio::time::sleep(Duration::from_millis(100)).await;
        }

        let as_principal = |name: &'static str, role: &'static str| {
            let deployment = &opened.deployment;
            let id = *opened.execution.id();
            async move {
                let key = deployment.key(name, &[role]).await;
                Execution::connect_with(id, &deployment.client(&key))
                    .await
                    .expect("the execution handle connects")
            }
        };
        let refused = as_principal("signoff_viewer", "viewer")
            .await
            .decide(approval, ApprovalDecision::Approved, None)
            .await
            .expect_err("a viewer cannot approve");
        assert_eq!(refused.code(), "WYRD_PERMISSION_403_DENIED_RBAC");

        let refused = as_principal("signoff_editor", "editor")
            .await
            .decide(approval, ApprovalDecision::Approved, None)
            .await
            .expect_err("an editor cannot approve");
        assert_eq!(refused.code(), "WYRD_PERMISSION_403_DENIED_RBAC");

        // An approver bound only on the shared Verifier cannot read the
        // Repository-owned execution, so it neither sees nor decides it.
        let deployment = &opened.deployment;
        let id = *opened.execution.id();
        let family = |kind: &str, name: &str| serde_json::json!({"card": {"family": {"kind": kind, "name": name}}});
        let on_verifier = [
            serde_json::json!({"resource": "verifier", "action": "approve", "scope": family("Verifier", "signoff")}),
            serde_json::json!({"resource": "cards", "action": "read", "scope": family("Verifier", "signoff")}),
        ];
        let connect = |key: String| async move {
            Execution::connect_with(id, &deployment.client(&key))
                .await
                .expect("the execution handle connects")
        };
        let verifier_only = connect(
            deployment
                .granted_key("signoff_on_verifier", &on_verifier)
                .await,
        )
        .await;
        let unseen = verifier_only
            .status()
            .await
            .expect_err("the execution is unseen");
        assert_eq!(unseen.code(), "WYRD_VERIFICATION_404_EXECUTION_NOT_FOUND");
        let refused = verifier_only
            .decide(approval, ApprovalDecision::Approved, None)
            .await
            .expect_err("a Verifier-only approver cannot decide");
        assert_eq!(refused.code(), "WYRD_PERMISSION_403_DENIED_RBAC");

        // With a separate read of the Repository family it reads the pending
        // approval and approves.
        let mut scoped = on_verifier.to_vec();
        scoped.extend([
            serde_json::json!({"resource": "cards", "action": "read", "scope": family("Repository", "web")}),
            serde_json::json!({"resource": "executions", "action": "read", "scope": family("Repository", "web")}),
        ]);
        let approver = connect(
            deployment
                .granted_key("signoff_scoped_approver", &scoped)
                .await,
        )
        .await;
        let seen = approver
            .status()
            .await
            .expect("the scoped approver reads the execution");
        assert_eq!(seen.approvals[0].status, ApprovalStatus::Pending);
        let reason = Some("reviewed the diff".to_owned());
        let receipt = approver
            .decide(approval, ApprovalDecision::Approved, reason.clone())
            .await
            .expect("the approver approves");
        assert_eq!(receipt.approval_id, approval);
        assert_eq!(receipt.status, ApprovalStatus::Approved);
        assert_eq!(receipt.reason, reason);
        assert!(receipt.actor_id.is_some() && receipt.decided_at.is_some());
        let replayed = approver
            .decide(approval, ApprovalDecision::Approved, reason)
            .await
            .expect("the same decision replays");
        assert_eq!(replayed, receipt);

        assert_eq!(opened.verdict().await, VerificationVerdict::Passed);
        let state = opened
            .execution
            .status()
            .await
            .expect("the execution reads");
        assert_eq!(state.approvals[0].status, ApprovalStatus::Approved);
        assert_eq!(state.approvals[0].actor_id, receipt.actor_id);
        opened.deployment.shutdown().await;
    }
}
