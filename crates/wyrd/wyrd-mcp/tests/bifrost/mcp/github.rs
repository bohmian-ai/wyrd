//! Tier-1 journey: an agent manages the tenant's GitHub connection over
//! `/mcp`.
//!
//! A tenant administrator begins a connection with `github_connection.connect`,
//! a browser signed in as the same principal completes it on the GitHub App
//! mock, and the agent reads and releases it; no answer carries a token, code
//! or key. A writer sees the read tool but not the write tools, and is
//! refused all three when it names them. An agent reads a declared
//! Repository's served selection and lists the Repository family.

/// The Postgres-backed half of the GitHub connection journey.
mod pg_tests {
    use crate::connectivity::{McpJourneyError, client, discover, problem, structured, transport};

    use rmcp::ClientServiceExt as _;
    use rmcp::model::{CallToolRequestParams, CallToolResult};
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
    /// pushed head cancels it as superseded with its pins kept.
    ///
    /// # Errors
    ///
    /// Returns server startup, bootstrap, connection, declaration, MCP
    /// transport, or tool-call failures.
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
        agent.cancel().await?;
        server.shutdown().await?;
        Ok(())
    }
}
