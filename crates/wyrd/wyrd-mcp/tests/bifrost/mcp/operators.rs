//! Tier-1 journey: an agent administers Operator connections over `/mcp`.
//!
//! A tenant administrator creates Slack, PagerDuty, and HTTP connections,
//! lists, reads, and rotates them, and disables and re-enables one through a
//! real `rmcp` client; no answer or refusal carries a secret. A writer keeps
//! the read tools but is neither offered nor allowed the write tools, and
//! another tenant's administrator can neither read nor mutate them.

/// The Postgres-backed half of the Operator connection journey.
mod pg_tests {
    use crate::connectivity::{McpJourneyError, discover, problem, structured, transport};

    use rmcp::ClientServiceExt as _;
    use rmcp::model::{CallToolRequestParams, CallToolResult};
    use secrecy::ExposeSecret as _;
    use serde_json::{Value as JsonValue, json};
    use wyrd_client::transport::credential::ResolvedCredential;
    use wyrd_testing::{Bootstrap, WyrdTestServer};

    /// Secrets the journey sends; none may appear in any answer or refusal.
    const SECRETS: [&str; 6] = [
        "xoxb-mcp-journey-secret",
        "xoxb-mcp-journey-rotated",
        "pd-mcp-journey-secret",
        "pd-mcp-journey-rotated",
        "http-mcp-journey-secret",
        "http-mcp-journey-rotated",
    ];

    /// Unwrap a machine bootstrap into an API-key credential.
    ///
    /// # Errors
    ///
    /// Returns an error when the bootstrap is a user principal.
    fn api_key(bootstrap: &Bootstrap) -> Result<ResolvedCredential, McpJourneyError> {
        Ok(ResolvedCredential::ApiKey(
            bootstrap
                .api_key()
                .ok_or("a service bootstraps with a machine key")?
                .expose_secret()
                .to_owned()
                .into(),
        ))
    }

    /// Build one tool call carrying `arguments`.
    ///
    /// # Errors
    ///
    /// Returns an error when `arguments` is not a JSON object.
    fn call(
        name: &'static str,
        arguments: JsonValue,
    ) -> Result<CallToolRequestParams, McpJourneyError> {
        Ok(CallToolRequestParams::new(name).with_arguments(
            arguments
                .as_object()
                .ok_or("arguments are an object")?
                .clone(),
        ))
    }

    /// Render a call result and assert it carries no journey secret.
    ///
    /// # Panics
    ///
    /// Panics when a secret appears.
    fn redacted(result: &CallToolResult) -> String {
        let text = format!("{result:?}");
        for secret in SECRETS {
            assert!(!text.contains(secret), "secret leaked over MCP: {text}");
        }
        text
    }

    /// An administrator manages redacted Slack, PagerDuty, and HTTP
    /// connections over MCP; a writer is not offered the write tools and is
    /// refused when it names one; a foreign tenant's administrator gets the
    /// stable not-found refusal on read, rotate, and disable.
    ///
    /// # Errors
    ///
    /// Returns server startup, bootstrap, MCP transport, or tool-call failures.
    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "requires the Postgres-backed Bifrost journey lane"]
    async fn an_agent_administers_redacted_connections() -> Result<(), McpJourneyError> {
        let server = WyrdTestServer::start_bound().await?;
        let admin = server.bootstrap_service("mcp-oc-admin", &["admin"]).await?;
        let admin_client =
            ().serve_with_lifecycle(transport(&server, api_key(&admin)?, None)?, discover())
                .await?;

        let create = json!({ "provider": "slack", "name": "mcp-slack",
                             "workspace_id": "T0001", "bot_token": SECRETS[0] });
        let created = admin_client
            .call_tool(call("operator_connections.create", create)?)
            .await?;
        redacted(&created);
        let created = structured(created)?;
        assert_eq!(created["status"], "active");
        let id = created["connection_id"]
            .as_str()
            .ok_or("connection_id is a string")?
            .to_owned();

        let mut ids = vec![id.clone()];
        for (create, rotate) in [
            (
                json!({ "provider": "pager_duty", "name": "mcp-pagerduty",
                        "integration_key": SECRETS[2] }),
                json!({ "provider": "pager_duty", "integration_key": SECRETS[3] }),
            ),
            (
                json!({ "provider": "http", "name": "mcp-http",
                        "origin": "https://hooks.example.com",
                        "auth": { "scheme": "bearer", "token": SECRETS[4] } }),
                json!({ "provider": "http", "auth": { "scheme": "bearer", "token": SECRETS[5] } }),
            ),
        ] {
            let created = admin_client
                .call_tool(call("operator_connections.create", create)?)
                .await?;
            redacted(&created);
            let created = structured(created)?;
            assert_eq!(created["status"], "active");
            let other = created["connection_id"]
                .as_str()
                .ok_or("connection_id is a string")?
                .to_owned();
            let rotated = admin_client
                .call_tool(call(
                    "operator_connections.update",
                    json!({ "connection_id": other, "request": rotate }),
                )?)
                .await?;
            redacted(&rotated);
            assert_eq!(structured(rotated)?["connection_id"], other);
            ids.push(other);
        }

        let listed = admin_client
            .call_tool(call("operator_connections.list", json!({}))?)
            .await?;
        redacted(&listed);
        let listed = structured(listed)?;
        let mut listed_ids: Vec<&str> = listed["connections"]
            .as_array()
            .ok_or("connections is an array")?
            .iter()
            .filter_map(|connection| connection["connection_id"].as_str())
            .collect();
        listed_ids.sort_unstable();
        let mut expected: Vec<&str> = ids.iter().map(String::as_str).collect();
        expected.sort_unstable();
        assert_eq!(listed_ids, expected);

        let rotated = admin_client
            .call_tool(call(
                "operator_connections.update",
                json!({ "connection_id": id,
                        "request": { "provider": "slack", "bot_token": SECRETS[1] } }),
            )?)
            .await?;
        redacted(&rotated);
        assert_eq!(structured(rotated)?["connection_id"], id);

        let malformed = admin_client
            .call_tool(call(
                "operator_connections.update",
                json!({ "connection_id": id,
                        "request": { "provider": "slack", "bot_token": 7, "note": SECRETS[1] } }),
            )?)
            .await;
        let rendered = match malformed {
            Err(error) => error.to_string(),
            Ok(result) => redacted(&result),
        };
        assert!(
            rendered.contains("WYRD_OPERATOR_400_INVALID_CONNECTION")
                && !rendered.contains(SECRETS[1]),
            "malformed arguments are refused without echo: {rendered}"
        );

        let disabled = structured(
            admin_client
                .call_tool(call(
                    "operator_connections.disable",
                    json!({ "connection_id": id }),
                )?)
                .await?,
        )?;
        assert_eq!(disabled["status"], "disabled");
        let read = structured(
            admin_client
                .call_tool(call(
                    "operator_connections.get",
                    json!({ "connection_id": id }),
                )?)
                .await?,
        )?;
        assert_eq!(read["status"], "disabled");
        let enabled = admin_client
            .call_tool(call(
                "operator_connections.update",
                json!({ "connection_id": id,
                        "request": { "provider": "slack", "status": "active" } }),
            )?)
            .await?;
        redacted(&enabled);
        assert_eq!(structured(enabled)?["status"], "active");
        admin_client.cancel().await?;

        let other_tenant = server.seed_tenant("mcp-oc-other").await?;
        let foreign = server
            .bootstrap_service_in_tenant(other_tenant, "mcp-oc-foreign", &["admin"])
            .await?;
        let foreign_client =
            ().serve_with_lifecycle(transport(&server, api_key(&foreign)?, None)?, discover())
                .await?;
        let foreign_list = structured(
            foreign_client
                .call_tool(call("operator_connections.list", json!({}))?)
                .await?,
        )?;
        assert_eq!(foreign_list["connections"], json!([]));
        for (tool, arguments) in [
            ("operator_connections.get", json!({ "connection_id": id })),
            (
                "operator_connections.update",
                json!({ "connection_id": id,
                        "request": { "provider": "slack", "bot_token": SECRETS[1] } }),
            ),
            (
                "operator_connections.disable",
                json!({ "connection_id": id }),
            ),
        ] {
            let refused = foreign_client.call_tool(call(tool, arguments)?).await;
            let rendered = match refused {
                Err(error) => error.to_string(),
                Ok(result) => {
                    redacted(&result);
                    problem(result)?.to_string()
                }
            };
            assert!(
                rendered.contains("WYRD_OPERATOR_404_CONNECTION_NOT_FOUND")
                    && !rendered.contains(SECRETS[1]),
                "{tool} on another tenant's connection is not found: {rendered}"
            );
        }
        foreign_client.cancel().await?;

        let writer = server
            .bootstrap_service("mcp-oc-writer", &["editor"])
            .await?;
        let writer_client =
            ().serve_with_lifecycle(transport(&server, api_key(&writer)?, None)?, discover())
                .await?;
        let tools = writer_client.list_all_tools().await?;
        let names: Vec<&str> = tools.iter().map(|tool| tool.name.as_ref()).collect();
        assert!(names.contains(&"operator_connections.list"), "{names:?}");
        assert!(!names.contains(&"operator_connections.create"), "{names:?}");
        let refused = writer_client
            .call_tool(call(
                "operator_connections.create",
                json!({ "provider": "pager_duty", "name": "mcp-pd",
                        "integration_key": SECRETS[0] }),
            )?)
            .await;
        let rendered = match refused {
            Err(error) => error.to_string(),
            Ok(result) => {
                redacted(&result);
                problem(result)?.to_string()
            }
        };
        assert!(
            rendered.contains("WYRD_PERMISSION_403_DENIED_RBAC"),
            "naming a write tool anyway is the stable permission refusal: {rendered}"
        );
        writer_client.cancel().await?;
        Ok(())
    }
}
