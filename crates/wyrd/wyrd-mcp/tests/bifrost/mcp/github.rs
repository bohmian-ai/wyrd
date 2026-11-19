//! Tier-1 journey: an agent manages the tenant's GitHub connection over
//! `/mcp`.
//!
//! A tenant administrator begins a connection with `github_connection.connect`,
//! a browser signed in as the same principal completes it on the GitHub App
//! mock, and the agent reads and releases it; no answer carries a token, code
//! or key. A writer sees the read tool but not the write tools, and is
//! refused all three when it names them. An agent reads a declared
//! Repository's served selection and lists the Repository family, requests a
//! fresh review of its pull request,
//! publishes the deferred evidence a waiting pull request execution needs,
//! and has another agent approve its pending signoff.

/// The Postgres-backed half of the GitHub connection journey.
mod pg_tests {
    use crate::connectivity::{McpJourneyError, client, discover, problem, structured, transport};

    use rmcp::ClientServiceExt as _;
    use rmcp::model::{CallToolRequestParams, CallToolResult};
    use rmcp::service::{RoleClient, RunningService};
    use secrecy::ExposeSecret as _;
    use serde_json::json;
    use wyrd_client::cards::Cards;
    use wyrd_client::transport::credential::ResolvedCredential;
    use wyrd_testing::WyrdTestServer;
    use wyrd_testing::github::{GithubAppMock, GithubBrowser, PullRequest};

    /// The tool `name` with `arguments`.
    ///
    /// # Errors
    ///
    /// Returns an error when `arguments` is not an object.
    fn call_with(
        name: &'static str,
        arguments: &serde_json::Value,
    ) -> Result<CallToolRequestParams, McpJourneyError> {
        Ok(CallToolRequestParams::new(name)
            .with_arguments(arguments.as_object().cloned().ok_or("an object")?))
    }

    /// The tool `name` with no arguments.
    fn call(name: &'static str) -> CallToolRequestParams {
        CallToolRequestParams::new(name).with_arguments(serde_json::Map::new())
    }

    /// Render `result` and assert none of `mock`'s secrets appear in it.
    ///
    /// # Panics
    ///
    /// Panics when a secret appears.
    fn redacted(mock: &GithubAppMock, result: &CallToolResult) {
        let text = format!("{result:?}");
        for secret in mock.secrets() {
            assert!(!text.contains(&secret), "secret leaked over MCP: {text}");
        }
    }

    /// Render a call that must have been refused, whether the server
    /// answered with a protocol error or a failed tool result.
    ///
    /// # Errors
    ///
    /// Returns an error when the call succeeded.
    fn refused(
        result: Result<CallToolResult, rmcp::ServiceError>,
    ) -> Result<String, McpJourneyError> {
        Ok(match result {
            Err(error) => error.to_string(),
            Ok(result) => problem(result)?.to_string(),
        })
    }

    /// An administrator connects, reads and disconnects GitHub over MCP; a
    /// writer is offered only the read and is refused every tool.
    ///
    /// # Errors
    ///
    /// Returns server startup, bootstrap, MCP transport, or tool-call failures.
    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "requires the Postgres-backed Bifrost journey lane"]
    async fn an_agent_manages_the_github_connection() -> Result<(), McpJourneyError> {
        let mock = GithubAppMock::start()?;
        mock.add_installation(101, &[(1, "acme/api")]);
        mock.add_user("octo", &[101]);
        mock.sign_in("octo");
        mock.install_target(101);
        let server = WyrdTestServer::builder()
            .with_github_app_for_test(&mock)
            .start_bound()
            .await?;
        let admin = server.bootstrap_service("mcp-gh-admin", &["admin"]).await?;
        let key = admin
            .api_key()
            .ok_or("a service bootstraps with a machine key")?
            .expose_secret()
            .to_owned();
        let admin_client = ()
            .serve_with_lifecycle(
                transport(
                    &server,
                    ResolvedCredential::ApiKey(key.clone().into()),
                    None,
                )?,
                discover(),
            )
            .await?;
        let tools = admin_client.list_all_tools().await?;
        let names: Vec<&str> = tools.iter().map(|tool| tool.name.as_ref()).collect();
        for tool in [
            "github_connection.get",
            "github_connection.connect",
            "github_connection.disconnect",
        ] {
            assert!(names.contains(&tool), "{tool}: {names:?}");
        }

        let before = admin_client
            .call_tool(call("github_connection.get"))
            .await?;
        assert_eq!(structured(before)?["state"], "not_connected");
        let receipt = admin_client
            .call_tool(call("github_connection.connect"))
            .await?;
        redacted(&mock, &receipt);
        let receipt = structured(receipt)?;
        let url = receipt["authorization_url"]
            .as_str()
            .ok_or("connect returns the install URL")?;
        let mut browser = GithubBrowser::sign_in(
            server.base_url().ok_or("bound")?,
            server.pg_fixture().tenant_slug(),
            &key,
        )
        .await?;
        let answer = browser.open(url).await?;
        assert_eq!(answer.status, 200, "{:?}", answer.body);

        let read = admin_client
            .call_tool(call("github_connection.get"))
            .await?;
        redacted(&mock, &read);
        let read = structured(read)?;
        assert_eq!(read["state"], "active");
        assert_eq!(read["installation_id"], 101);
        assert_eq!(
            read["repositories"],
            json!([{ "repository_id": "1", "full_name": "acme/api" }])
        );
        let released = structured(
            admin_client
                .call_tool(call("github_connection.disconnect"))
                .await?,
        )?;
        assert_eq!(released["state"], "not_connected");
        let again = refused(
            admin_client
                .call_tool(call("github_connection.disconnect"))
                .await,
        )?;
        assert!(again.contains("WYRD_GITHUB_404_NOT_CONNECTED"), "{again}");
        admin_client.cancel().await?;

        let writer = server
            .bootstrap_service("mcp-gh-writer", &["editor"])
            .await?;
        let writer_client = ()
            .serve_with_lifecycle(
                transport(
                    &server,
                    ResolvedCredential::ApiKey(
                        writer
                            .api_key()
                            .ok_or("a service bootstraps with a machine key")?
                            .expose_secret()
                            .to_owned()
                            .into(),
                    ),
                    None,
                )?,
                discover(),
            )
            .await?;
        let tools = writer_client.list_all_tools().await?;
        let names: Vec<&str> = tools.iter().map(|tool| tool.name.as_ref()).collect();
        assert!(names.contains(&"github_connection.get"), "{names:?}");
        assert!(!names.contains(&"github_connection.connect"), "{names:?}");
        assert!(
            !names.contains(&"github_connection.disconnect"),
            "{names:?}"
        );
        for tool in [
            "github_connection.get",
            "github_connection.connect",
            "github_connection.disconnect",
        ] {
            let refusal = refused(writer_client.call_tool(call(tool)).await)?;
            assert!(
                refusal.contains("WYRD_PERMISSION_403_DENIED_RBAC"),
                "{tool}: {refusal}"
            );
        }
        writer_client.cancel().await?;
        server.shutdown().await?;
        Ok(())
    }

    /// A Repository declared through the SDK reads back over MCP with its
    /// served selection and binding, and `cards.list` filters the family by
    /// the `Repository` kind. A pull request on it admits an execution the
    /// agent finds through the binding, pinned to its base and head, and a
    /// pushed head cancels it as superseded with its pins kept. The agent
    /// then requests a fresh review at the unchanged head, whose retry
    /// returns the same generation, while a viewer is neither offered nor
    /// granted the request.
    ///
    /// # Errors
    ///
    /// Returns server startup, bootstrap, connection, declaration, MCP
    /// transport, or tool-call failures.
    ///
    /// # Panics
    ///
    /// Panics when an admitted execution's pins, binding, or supersession
    /// differ from what GitHub showed.
    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "requires the Postgres-backed Bifrost journey lane"]
    async fn an_agent_reads_the_selected_repository_policy() -> Result<(), McpJourneyError> {
        let mock = GithubAppMock::start()?;
        mock.add_installation(101, &[(2, "acme/web")]);
        mock.add_user("octo", &[101]);
        mock.sign_in("octo");
        mock.install_target(101);
        let server = WyrdTestServer::builder()
            .with_github_app_for_test(&mock)
            .start_bound()
            .await?;
        let admin = server
            .bootstrap_service("mcp-repo-admin", &["admin"])
            .await?;
        let key = admin
            .api_key()
            .ok_or("a service bootstraps with a machine key")?
            .expose_secret()
            .to_owned();
        let credential = || ResolvedCredential::ApiKey(key.clone().into());
        let agent =
            ().serve_with_lifecycle(transport(&server, credential(), None)?, discover())
                .await?;
        let receipt = structured(agent.call_tool(call("github_connection.connect")).await?)?;
        let mut browser = GithubBrowser::sign_in(
            server.base_url().ok_or("bound")?,
            server.pg_fixture().tenant_slug(),
            &key,
        )
        .await?;
        let url = receipt["authorization_url"]
            .as_str()
            .ok_or("connect returns the install URL")?;
        assert_eq!(browser.open(url).await?.status, 200);
        let cards = Cards::connect_with(&client(&server, credential())?).await?;
        let declared = Box::pin(cards.declare_from_path(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../fixtures/cards/repository_declaration/repository.yaml"
        )))
        .await?;

        let read = structured(
            agent
                .call_tool(
                    CallToolRequestParams::new("cards.get").with_arguments(
                        json!({ "card_ref": declared.root })
                            .as_object()
                            .cloned()
                            .ok_or("an object")?,
                    ),
                )
                .await?,
        )?;
        let card = &read["card"];
        assert_eq!(card["kind"], "Repository");
        assert_eq!(card["spec"]["repository_id"], "2");
        assert_eq!(
            card["status"]["repository"],
            json!({
                "selected": true,
                "selected_version": declared.root.version,
                "full_name": "acme/web",
            })
        );
        assert_eq!(
            card["status"]["verification"]["binding_ids"]
                .as_array()
                .map(Vec::len),
            Some(1)
        );
        let listed = structured(
            agent
                .call_tool(
                    CallToolRequestParams::new("cards.list").with_arguments(
                        json!({ "kind": "Repository" })
                            .as_object()
                            .cloned()
                            .ok_or("an object")?,
                    ),
                )
                .await?,
        )?;
        let names: Vec<_> = listed["items"]
            .as_array()
            .ok_or("items")?
            .iter()
            .map(|item| item["name"].clone())
            .collect();
        assert_eq!(names, [json!("web")]);

        let binding = card["status"]["verification"]["binding_ids"][0].clone();
        let base = server.base_url().ok_or("bound")?;
        let newest = async || -> Result<serde_json::Value, McpJourneyError> {
            let read = structured(
                agent
                    .call_tool(call_with(
                        "verification.get_binding",
                        &json!({ "binding_id": binding }),
                    )?)
                    .await?,
            )?;
            Ok(read["last_execution_id"].clone())
        };
        let execution =
            async |id: &serde_json::Value| -> Result<serde_json::Value, McpJourneyError> {
                structured(
                    agent
                        .call_tool(call_with(
                            "verification.get_execution",
                            &json!({ "execution_id": id }),
                        )?)
                        .await?,
                )
            };
        mock.set_pull_request(2, 7, PullRequest::open(2, "b1", "a1"));
        let opened = mock.pull_request_event(101, 2, 7, "opened");
        assert_eq!(mock.deliver(base, "pull_request", &opened).await?, 202);
        let first = newest().await?;
        let admitted = execution(&first).await?;
        assert_eq!(admitted["status"], "pending");
        assert_eq!(admitted["pull_request"]["number"], 7);
        assert_eq!(admitted["pull_request"]["repository_id"], "2");
        assert_eq!(
            admitted["pull_request"]["base"],
            json!({ "ref": "main", "sha": "b1" })
        );
        assert_eq!(admitted["pull_request"]["head"]["sha"], "a1");
        assert_eq!(admitted["pull_request"]["head"]["ref"], "feature");
        assert_eq!(admitted["pull_request"]["head_repository_id"], "2");
        assert!(
            admitted["pull_request"]["delivery_id"]
                .as_str()
                .is_some_and(|id| !id.is_empty())
        );
        mock.edit_pull_request(2, 7, |pr| pr.head_sha = "b2".to_owned());
        let pushed = mock.pull_request_event(101, 2, 7, "synchronize");
        assert_eq!(mock.deliver(base, "pull_request", &pushed).await?, 202);
        assert_ne!(newest().await?, first);
        let superseded = execution(&first).await?;
        assert_eq!(superseded["status"], "cancelled");
        assert_eq!(superseded["error"]["code"], "pull_request_superseded");
        assert_eq!(superseded["pull_request"]["head"]["sha"], "a1");

        let tools = agent.list_all_tools().await?;
        assert!(
            tools
                .iter()
                .any(|tool| tool.name == "github_connection.review_pull_request"),
            "a caller who may run Verifiers is offered the review request"
        );
        let review = json!({
            "host": mock.host(),
            "repository_id": "2",
            "number": 7,
            "idempotency_key": "mcp-review-7-a",
        });
        let requested = structured(
            agent
                .call_tool(call_with("github_connection.review_pull_request", &review)?)
                .await?,
        )?;
        let members = requested["executions"].clone();
        assert_eq!(members.as_array().map(Vec::len), Some(1), "{requested}");
        let fresh = execution(&members[0]).await?;
        assert_eq!(fresh["status"], "pending");
        assert_eq!(fresh["pull_request"]["head"]["sha"], "b2");
        assert_eq!(fresh["pull_request"]["generation"], requested["generation"]);
        assert_eq!(fresh["pull_request"]["members"], members);
        assert_eq!(fresh["pull_request"]["request_key"], "mcp-review-7-a");
        assert_eq!(
            structured(
                agent
                    .call_tool(call_with("github_connection.review_pull_request", &review)?)
                    .await?,
            )?,
            requested,
            "a retry of the key returns the same generation"
        );
        assert_eq!(newest().await?, members[0]);

        let viewer = server
            .bootstrap_service("mcp-repo-viewer", &["viewer"])
            .await?;
        let viewer_key = viewer
            .api_key()
            .ok_or("a service bootstraps with a machine key")?
            .expose_secret()
            .to_owned();
        let watcher = ()
            .serve_with_lifecycle(
                transport(&server, ResolvedCredential::ApiKey(viewer_key.into()), None)?,
                discover(),
            )
            .await?;
        let tools = watcher.list_all_tools().await?;
        assert!(
            !tools
                .iter()
                .any(|tool| tool.name == "github_connection.review_pull_request"),
            "a viewer is not offered the review request"
        );
        let mut other = review.clone();
        other["idempotency_key"] = json!("mcp-review-7-viewer");
        let refusal = refused(
            watcher
                .call_tool(call_with("github_connection.review_pull_request", &other)?)
                .await,
        )?;
        assert!(
            refusal.contains("WYRD_PERMISSION_403_DENIED_RBAC"),
            "{refusal}"
        );
        assert_eq!(
            newest().await?,
            members[0],
            "a refused request admits nothing"
        );
        watcher.cancel().await?;
        agent.cancel().await?;
        server.shutdown().await?;
        Ok(())
    }

    /// The head commit every waiting pull request opens at.
    const HEAD: &str = "a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1";

    /// A connected deployment whose administrator agent declared a
    /// Repository and opened pull request 9 of `acme/web` at [`HEAD`].
    struct Opened {
        /// The GitHub App mock, kept alive with the server.
        mock: GithubAppMock,
        /// The running server.
        server: WyrdTestServer,
        /// The administrator's agent.
        agent: RunningService<RoleClient, ()>,
        /// The pull request's execution ID.
        execution: serde_json::Value,
    }

    impl Opened {
        /// Connect GitHub as an administrator agent, declare the Repository
        /// fixture at `repository`, and open the pull request.
        ///
        /// # Errors
        ///
        /// Returns server startup, bootstrap, connection, declaration, MCP
        /// transport, or tool-call failures.
        ///
        /// # Panics
        ///
        /// Panics when the browser install does not answer 200 or the
        /// `opened` webhook delivery is not accepted with 202.
        async fn start(repository: &str) -> Result<Self, McpJourneyError> {
            let mock = GithubAppMock::start()?;
            mock.add_installation(101, &[(2, "acme/web")]);
            mock.add_user("octo", &[101]);
            mock.sign_in("octo");
            mock.install_target(101);
            let server = WyrdTestServer::builder()
                .with_github_app_for_test(&mock)
                .with_verification_runtime_for_test()
                .start_bound()
                .await?;
            let key = Self::key(&server, "mcp-github-admin", "admin").await?;
            let credential = || ResolvedCredential::ApiKey(key.clone().into());
            let agent =
                ().serve_with_lifecycle(transport(&server, credential(), None)?, discover())
                    .await?;
            let receipt = structured(agent.call_tool(call("github_connection.connect")).await?)?;
            let mut browser = GithubBrowser::sign_in(
                server.base_url().ok_or("bound")?,
                server.pg_fixture().tenant_slug(),
                &key,
            )
            .await?;
            let url = receipt["authorization_url"]
                .as_str()
                .ok_or("connect returns the install URL")?;
            assert_eq!(browser.open(url).await?.status, 200);
            let cards = Cards::connect_with(&client(&server, credential())?).await?;
            let declared = Box::pin(cards.declare_from_path(format!(
                "{}/../../../fixtures/cards/{repository}",
                env!("CARGO_MANIFEST_DIR")
            )))
            .await?;
            let card = structured(
                agent
                    .call_tool(call_with(
                        "cards.get",
                        &json!({ "card_ref": declared.root }),
                    )?)
                    .await?,
            )?;
            let binding = card["card"]["status"]["verification"]["binding_ids"][0].clone();

            mock.set_pull_request(2, 9, PullRequest::open(2, "b1", HEAD));
            let opened = mock.pull_request_event(101, 2, 9, "opened");
            let base = server.base_url().ok_or("bound")?;
            assert_eq!(mock.deliver(base, "pull_request", &opened).await?, 202);
            let read = structured(
                agent
                    .call_tool(call_with(
                        "verification.get_binding",
                        &json!({ "binding_id": binding }),
                    )?)
                    .await?,
            )?;
            let execution = read["last_execution_id"].clone();
            Ok(Self {
                mock,
                server,
                agent,
                execution,
            })
        }

        /// The API key of a new service principal `name` holding `role`.
        ///
        /// # Errors
        ///
        /// Returns bootstrap failures.
        async fn key(
            server: &WyrdTestServer,
            name: &str,
            role: &str,
        ) -> Result<String, McpJourneyError> {
            Ok(server
                .bootstrap_service(name, &[role])
                .await?
                .api_key()
                .ok_or("a service bootstraps with a machine key")?
                .expose_secret()
                .to_owned())
        }

        /// A new agent of a service principal `name` holding `role`.
        ///
        /// # Errors
        ///
        /// Returns bootstrap or MCP transport failures.
        async fn agent(
            &self,
            name: &str,
            role: &str,
        ) -> Result<RunningService<RoleClient, ()>, McpJourneyError> {
            let key = Self::key(&self.server, name, role).await?;
            Ok(()
                .serve_with_lifecycle(
                    transport(&self.server, ResolvedCredential::ApiKey(key.into()), None)?,
                    discover(),
                )
                .await?)
        }

        /// Read the execution until `done` holds, within one minute.
        ///
        /// # Errors
        ///
        /// Returns MCP failures or an error naming the last state when it
        /// never holds.
        async fn until(
            &self,
            done: impl Fn(&serde_json::Value) -> bool,
        ) -> Result<serde_json::Value, McpJourneyError> {
            let deadline = tokio::time::Instant::now() + std::time::Duration::from_mins(1);
            loop {
                let state = structured(
                    self.agent
                        .call_tool(call_with(
                            "verification.get_execution",
                            &json!({ "execution_id": self.execution }),
                        )?)
                        .await?,
                )?;
                if done(&state) {
                    return Ok(state);
                }
                if tokio::time::Instant::now() >= deadline {
                    return Err(format!("execution never settled as expected: {state}").into());
                }
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            }
        }

        /// The `verification.publish_evidence` call supplying `ci` as `value`.
        ///
        /// # Errors
        ///
        /// Returns an error when the arguments are not an object.
        fn publish(value: serde_json::Value) -> Result<CallToolRequestParams, McpJourneyError> {
            call_with(
                "verification.publish_evidence",
                &json!({
                    "repository": "acme/web",
                    "tags": { "commit": HEAD },
                    "input": "ci",
                    "value": { "kind": "json", "value": value },
                }),
            )
        }

        /// Close the agent and stop the server.
        ///
        /// # Errors
        ///
        /// Returns MCP or shutdown failures.
        async fn stop(self) -> Result<(), McpJourneyError> {
            self.agent.cancel().await?;
            self.server.shutdown().await?;
            Ok(())
        }
    }

    /// The names of the tools `agent` is offered.
    ///
    /// # Errors
    ///
    /// Returns MCP failures.
    async fn tools(agent: &RunningService<RoleClient, ()>) -> Result<Vec<String>, McpJourneyError> {
        Ok(agent
            .list_all_tools()
            .await?
            .into_iter()
            .map(|tool| tool.name.into_owned())
            .collect())
    }

    /// An agent finds a pull request execution waiting for its deferred
    /// `ci` input, publishes the evidence for the repository slug and head
    /// commit, and the same execution completes; publishing again replays
    /// the receipt, and different content is refused as a conflict.
    ///
    /// # Errors
    ///
    /// Returns server startup, bootstrap, connection, declaration, MCP
    /// transport, or tool-call failures.
    ///
    /// # Panics
    ///
    /// Panics when the execution does not wait, the receipt differs, or the
    /// resumed execution does not pass.
    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "requires the Postgres-backed Bifrost journey lane"]
    async fn an_agent_publishes_deferred_evidence_for_a_waiting_execution()
    -> Result<(), McpJourneyError> {
        let opened = Opened::start("deferred_evidence/repository.yaml").await?;
        let agent = &opened.agent;
        assert!(
            tools(agent)
                .await?
                .iter()
                .any(|name| name == "verification.publish_evidence")
        );
        let waiting = opened.until(|state| state["status"] == "waiting").await?;
        assert_eq!(waiting["waiting"], json!(["ci"]));

        let accepted = structured(
            agent
                .call_tool(Opened::publish(json!({"status": "green"}))?)
                .await?,
        )?;
        assert_eq!(accepted["status"], "accepted");
        assert_eq!(accepted["matched_subscriptions"], 1);
        assert_eq!(
            accepted["repository"],
            json!({ "host": opened.mock.host(), "repository_id": "2" })
        );
        let completed = opened
            .until(|state| state["status"] == "completed" && !state["result"].is_null())
            .await?;
        assert_eq!(completed["result"]["outcome"]["verdict"], "passed");
        let replayed = structured(
            agent
                .call_tool(Opened::publish(json!({"status": "green"}))?)
                .await?,
        )?;
        assert_eq!(replayed["status"], "replayed");
        assert_eq!(replayed["evidence_id"], accepted["evidence_id"]);
        let conflict = refused(
            agent
                .call_tool(Opened::publish(json!({"status": "red"}))?)
                .await,
        )?;
        assert!(
            conflict.contains("WYRD_VERIFICATION_409_EVIDENCE_CONFLICT"),
            "{conflict}"
        );
        opened.stop().await
    }

    /// Another agent approves a pull request's pending signoff over MCP and
    /// the same execution passes.
    ///
    /// The execution lists one pending `signoff` occurrence while it waits
    /// for `ci`; once CI is published it waits for the approval alone. A
    /// viewer agent is not offered `verification.decide_approval` and is
    /// refused when it names it; an editor agent approves with a reason,
    /// receives the durable receipt, replays it, and the execution passes
    /// listing who approved.
    ///
    /// # Errors
    ///
    /// Returns server startup, bootstrap, connection, declaration, MCP
    /// transport, or tool-call failures.
    ///
    /// # Panics
    ///
    /// Panics when the occurrence is not pending, a decision or refusal
    /// differs, or the execution does not pass.
    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "requires the Postgres-backed Bifrost journey lane"]
    async fn an_agent_approves_a_pending_signoff() -> Result<(), McpJourneyError> {
        let opened = Opened::start("approvals/repository.yaml").await?;
        let waiting = opened.until(|state| state["status"] == "waiting").await?;
        assert_eq!(waiting["waiting"], json!(["ci"]));
        let pending = &waiting["approvals"][0];
        assert_eq!(pending["step_id"], "signoff");
        assert_eq!(pending["message"], "Ship this change?");
        assert_eq!(pending["status"], "pending");
        let approval = pending["approval_id"].clone();
        structured(
            opened
                .agent
                .call_tool(Opened::publish(json!({"status": "green"}))?)
                .await?,
        )?;
        opened
            .until(|state| state["status"] == "waiting" && state["waiting"].is_null())
            .await?;

        let decide = |reason: Option<&str>| {
            call_with(
                "verification.decide_approval",
                &json!({
                    "execution_id": opened.execution,
                    "approval_id": approval,
                    "decision": "approved",
                    "reason": reason,
                }),
            )
        };
        let viewer = opened.agent("mcp-signoff-viewer", "viewer").await?;
        assert!(
            !tools(&viewer)
                .await?
                .iter()
                .any(|name| name == "verification.decide_approval"),
            "a viewer is not offered the decision"
        );
        let denied = refused(viewer.call_tool(decide(Some("looks fine"))?).await)?;
        assert!(
            denied.contains("WYRD_PERMISSION_403_DENIED_RBAC"),
            "{denied}"
        );
        viewer.cancel().await?;

        let editor = opened.agent("mcp-signoff-editor", "editor").await?;
        assert!(
            tools(&editor)
                .await?
                .iter()
                .any(|name| name == "verification.decide_approval")
        );
        let receipt = structured(editor.call_tool(decide(Some("reviewed the diff"))?).await?)?;
        assert_eq!(receipt["approval_id"], approval);
        assert_eq!(receipt["status"], "approved");
        assert_eq!(receipt["reason"], "reviewed the diff");
        assert!(receipt["actor_id"].is_string() && receipt["decided_at"].is_string());
        let replayed = structured(editor.call_tool(decide(Some("reviewed the diff"))?).await?)?;
        assert_eq!(replayed, receipt);
        let conflict = refused(editor.call_tool(decide(None)?).await)?;
        assert!(
            conflict.contains("WYRD_VERIFICATION_409_APPROVAL_NOT_PENDING"),
            "{conflict}"
        );
        editor.cancel().await?;

        let completed = opened
            .until(|state| state["status"] == "completed" && !state["result"].is_null())
            .await?;
        assert_eq!(completed["result"]["outcome"]["verdict"], "passed");
        assert_eq!(completed["approvals"][0]["status"], "approved");
        assert_eq!(completed["approvals"][0]["actor_id"], receipt["actor_id"]);
        opened.stop().await
    }
}
