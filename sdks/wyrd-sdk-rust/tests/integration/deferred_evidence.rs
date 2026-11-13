//! CI publishes a deferred input after review starts: the pull request's
//! execution waits for `ci`, `Evidence::publish` supplies it for the
//! repository commit, and the same execution resumes and passes. The receipt
//! is durable: a replay answers the same identity, and different evidence or
//! an unconnected repository is refused.

/// Real-server and PostgreSQL cases, placed for the `pg-servers` test group.
mod pg_tests {
    use std::time::{Duration, Instant};

    use wyrd_ai::cards::CardSelector;
    use wyrd_ai::github::GithubConnection;
    use wyrd_ai::verify::{Evidence, EvidenceStatus, Execution};
    use wyrd_spec::verification::{VerificationExecutionStatus, VerificationVerdict};
    use wyrd_testing::WyrdTestServer;
    use wyrd_testing::github::{GithubAppMock, GithubBrowser, PullRequest};

    use crate::support::{Deployment, declare};

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
        let head = "a1".repeat(20);
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
        let root = declare(&cards, "cards/deferred_evidence/repository.yaml")
            .await
            .root;
        let binding = cards
            .get(CardSelector::exact(root))
            .await
            .expect("the Repository reads")
            .status
            .and_then(|status| status.verification)
            .expect("the Repository serves its bindings")
            .binding_ids[0];
        mock.set_pull_request(2, 7, PullRequest::open(2, &"b1".repeat(20), &head));
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
            ["ci"]
        );

        let evidence = Evidence::connect_with(&client)
            .await
            .expect("the publisher connects");
        let green = serde_json::json!({ "status": "green" });
        let accepted = evidence
            .publish("ci", green.clone(), Some("acme/web"), Some(&head))
            .await
            .expect("the evidence is accepted");
        assert_eq!(accepted.status, EvidenceStatus::Accepted);
        assert_eq!(accepted.matched_subscriptions, 1);
        assert_eq!(accepted.repository.repository_id.as_str(), "2");
        assert_eq!(accepted.tags.commit.as_str(), head);
        let result = execution
            .wait(Duration::from_secs(30))
            .await
            .expect("the resumed execution completes");
        assert_eq!(result.outcome.verdict, VerificationVerdict::Passed);

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
        deployment.shutdown().await;
    }
}
