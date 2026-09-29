//! Tier-1 journey: an agent discovers a binding, starts a run, and observes it over `/mcp`.
//!
//! A registered Service acting as itself reads its own Card, finds the binding
//! its `verified_by` projected, reads the binding's readiness, starts a keyed
//! manual Drift run, and reads the run back — every step through a real `rmcp`
//! client. A reader is not offered the write tool and is refused when it names
//! it anyway.

/// The Postgres-backed half of the verification journey.
mod pg_tests {
    use crate::connectivity::{McpJourneyError, client, discover, problem, structured, transport};

    use rmcp::ClientServiceExt as _;
    use rmcp::model::CallToolRequestParams;
    use secrecy::ExposeSecret as _;
    use serde_json::Value as JsonValue;
    use wyrd_client::cards::Cards;
    use wyrd_client::transport::credential::ResolvedCredential;
    use wyrd_spec::reference::CardRef;
    use wyrd_testing::{Bootstrap, WyrdTestServer};

    /// The Card read tool an agent uses to find binding IDs.
    const CARDS_GET: &str = "cards.get";

    /// The binding status read tool.
    const GET_BINDING: &str = "verification.get_binding";

    /// The manual run write tool, offered only with `evals:run`.
    const START_RUN: &str = "verification.start_run";

    /// The run status read tool.
    const GET_RUN: &str = "verification.get_run";

    /// A ready Custom Drift Verifier the Service binds.
    const VERIFIER: &str = "apiVersion: wyrd/v1\nkind: Verifier\nmetadata:\n  name: mcp-run-drift\n  version: 1.0.0\n  space: default\nspec:\n  implementation:\n    kind: drift\n    spec:\n      method: Custom\n      signal:\n        kind: Metric\n        name: score\n      condition:\n        kind: Statistical\n      profile:\n        kind: Custom\n        metric_name: score\n        baseline_value: 1.0\n        alert_threshold: 0.5\n";

    /// A Service bound to [`VERIFIER`] on a schedule.
    const SERVICE: &str = "apiVersion: wyrd/v1\nkind: Service\nmetadata:\n  name: mcp-run-service\n  version: 1.0.0\n  space: default\nspec:\n  verified_by:\n    - verifier:\n        kind: Verifier\n        name: mcp-run-drift\n        version: 1.0.0\n        space: default\n      runs_on:\n        kind: schedule\n        cron: \"0 2 * * *\"\n";

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

    /// Read one string field from a structured result.
    ///
    /// # Errors
    ///
    /// Returns an error when the field is absent or not a string.
    fn text<'a>(value: &'a JsonValue, pointer: &str) -> Result<&'a str, McpJourneyError> {
        value
            .pointer(pointer)
            .and_then(JsonValue::as_str)
            .ok_or_else(|| format!("{pointer} is a string in {value}").into())
    }

    /// Register the fixture Verifier and Service and return the Service's UID.
    ///
    /// # Errors
    ///
    /// Returns an error when a fixture cannot be written or registered.
    async fn register_fixture(cards: &Cards) -> Result<(String, CardRef), McpJourneyError> {
        let root = std::env::temp_dir().join(format!("wyrd-mcp-run-{}", uuid::Uuid::now_v7()));
        std::fs::create_dir_all(&root)?;
        std::fs::write(root.join("verifier.yaml"), VERIFIER)?;
        std::fs::write(root.join("service.yaml"), SERVICE)?;
        Box::pin(cards.register_from_path(&root.join("verifier.yaml"))).await?;
        let receipt = Box::pin(cards.register_from_path(&root.join("service.yaml"))).await?;
        std::fs::remove_dir_all(&root)?;
        let uid = receipt
            .root
            .uid
            .as_ref()
            .ok_or("a registered Service has a UID")?
            .to_string();
        Ok((uid, receipt.root))
    }

    /// A bound Service discovers its binding, starts a keyed run, and reads it
    /// back over MCP; a reader is not offered the write tool and is refused.
    ///
    /// # Errors
    ///
    /// Returns server startup, bootstrap, registration, MCP transport, or
    /// tool-call failures.
    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "requires the Postgres-backed Bifrost journey lane"]
    async fn an_agent_discovers_starts_and_observes_a_manual_run() -> Result<(), McpJourneyError> {
        let server = WyrdTestServer::start_bound().await?;
        let admin = server
            .bootstrap_service("mcp-run-admin", &["admin"])
            .await?;
        let cards = Cards::with_client(client(&server, api_key(&admin)?)?);
        let (service_uid, service_ref) = register_fixture(&cards).await?;

        let writer = server
            .credential_registered_service(&service_ref, &["writer"])
            .await?;
        let writer_client =
            ().serve_with_lifecycle(transport(&server, api_key(&writer)?, None)?, discover())
                .await?;
        let tools = writer_client.list_all_tools().await?;
        let names: Vec<&str> = tools.iter().map(|tool| tool.name.as_ref()).collect();
        for tool in [CARDS_GET, GET_BINDING, START_RUN, GET_RUN] {
            assert!(
                names.contains(&tool),
                "a writer is offered {tool}: {names:?}"
            );
        }

        // Discover: the Card the agent owns names its binding.
        let card = structured(
            writer_client
                .call_tool(call(
                    CARDS_GET,
                    serde_json::json!({ "kind": "Service", "card_uid": service_uid }),
                )?)
                .await?,
        )?;
        let binding_id = text(&card, "/card/status/verification/binding_ids/0")?.to_owned();
        let binding = structured(
            writer_client
                .call_tool(call(
                    GET_BINDING,
                    serde_json::json!({ "binding_id": binding_id }),
                )?)
                .await?,
        )?;
        assert_eq!(text(&binding, "/subject_card_uid")?, service_uid);
        assert_eq!(text(&binding, "/readiness")?, "ready");

        // Act: a keyed start, then a replay that returns the same run.
        let start = serde_json::json!({
            "target": { "kind": "binding", "binding_id": binding_id },
            "input": {
                "kind": "drift_window",
                "start": "2026-09-17T00:00:00Z",
                "end": "2026-09-17T01:00:00Z",
            },
            "idempotency_key": "mcp-journey-0001",
        });
        let started = structured(
            writer_client
                .call_tool(call(START_RUN, start.clone())?)
                .await?,
        )?;
        let run_id = text(&started, "/run_id")?.to_owned();
        let replayed = structured(
            writer_client
                .call_tool(call(START_RUN, start.clone())?)
                .await?,
        )?;
        assert_eq!(
            text(&replayed, "/run_id")?,
            run_id,
            "a keyed retry returns the same run"
        );

        // Observe: the run reads back under the same identity.
        let run = structured(
            writer_client
                .call_tool(call(GET_RUN, serde_json::json!({ "run_id": run_id }))?)
                .await?,
        )?;
        assert_eq!(text(&run, "/run_id")?, run_id);
        writer_client.cancel().await?;

        // A reader keeps the read tools but is neither offered nor allowed the write tool.
        let reader = server
            .bootstrap_service("mcp-run-reader", &["reader"])
            .await?;
        let reader_client =
            ().serve_with_lifecycle(transport(&server, api_key(&reader)?, None)?, discover())
                .await?;
        let reader_tools = reader_client.list_all_tools().await?;
        let reader_names: Vec<&str> = reader_tools.iter().map(|tool| tool.name.as_ref()).collect();
        assert!(
            reader_names.contains(&GET_RUN),
            "read tools stay available: {reader_names:?}"
        );
        assert!(
            !reader_names.contains(&START_RUN),
            "an agent without evals:run is not offered the write tool: {reader_names:?}"
        );
        let refused = reader_client.call_tool(call(START_RUN, start)?).await;
        let rendered = match refused {
            Err(error) => error.to_string(),
            Ok(result) => problem(result)?.to_string(),
        };
        assert!(
            rendered.contains("WYRD_PERMISSION_403_DENIED_RBAC"),
            "naming the write tool anyway is the stable permission refusal: {rendered}"
        );
        reader_client.cancel().await?;
        Ok(())
    }
}
