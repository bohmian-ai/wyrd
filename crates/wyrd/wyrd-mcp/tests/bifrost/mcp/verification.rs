//! Tier-1 journey: an agent discovers a binding, starts a run, and observes it over `/mcp`.
//!
//! A registered Service acting as itself reads its own Card, finds the binding
//! its `verified_by` projected, reads the binding's readiness, starts a keyed
//! manual Drift run, and reads the run back — every step through a real `rmcp`
//! client. A reader is not offered the write tool and is refused when it names
//! it anyway.
//!
//! On a runtime-enabled server the same Service starts a direct Verifier run
//! over its own emitted Drift observations, polls it to a terminal status, and
//! reads the persisted result and feature rows through `bifrost.query`.
//!
//! Through `verification.execute` the Service also judges supplied samples
//! inline and receives the verdict in the tool result.

/// The Postgres-backed half of the verification journey.
mod pg_tests {
    use crate::connectivity::{
        McpJourneyError, client, discover, problem, refusal, structured, transport,
    };

    use std::path::Path;
    use std::time::Duration;

    use rmcp::ClientServiceExt as _;
    use rmcp::model::CallToolRequestParams;
    use rmcp::service::{RoleClient, RunningService};
    use secrecy::ExposeSecret as _;
    use serde_json::Value as JsonValue;
    use wyrd_client::QueueConfig;
    use wyrd_client::bifrost::client_from_options;
    use wyrd_client::cards::{CardGraphHydrator, CardSelector, Cards, HydrationMode};
    use wyrd_client::state::WyrdState;
    use wyrd_client::transport::credential::ResolvedCredential;
    use wyrd_runtime::Permission;
    use wyrd_spec::envelope::CardKind;
    use wyrd_spec::reference::CardRef;
    use wyrd_spec::registry::{GetCardResponse, ListCardsResponse};
    use wyrd_testing::{Bootstrap, WyrdTestServer};

    /// The Card read tool an agent uses to find binding IDs.
    const CARDS_GET: &str = "cards.get";

    /// The binding status read tool.
    const GET_BINDING: &str = "verification.get_binding";

    /// The manual run write tool, offered only with `verifier:run`.
    const START_RUN: &str = "verification.start_run";

    /// The run status read tool.
    const GET_RUN: &str = "verification.get_run";

    /// The direct execution write tool, offered only with `verifier:run`.
    const EXECUTE: &str = "verification.execute";

    /// The existing Bifrost read tool verdicts are read through.
    const QUERY: &str = "bifrost.query";

    /// Upper bound on the wait for the server-side runtime to settle a run.
    const SETTLE: Duration = Duration::from_secs(90);

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

    /// The registered fixture: the Service subject and the Verifier it binds.
    struct Fixture {
        /// The Service's server-minted UID.
        service_uid: String,
        /// The Service's exact UID-bearing reference.
        service_ref: CardRef,
        /// The Custom Drift Verifier's server-minted UID.
        verifier_uid: String,
    }

    /// Register the fixture Verifier and Service.
    ///
    /// # Errors
    ///
    /// Returns an error when a fixture cannot be written or registered, or a
    /// receipt carries no UID.
    async fn register_fixture(cards: &Cards) -> Result<Fixture, McpJourneyError> {
        let root = std::env::temp_dir().join(format!("wyrd-mcp-run-{}", uuid::Uuid::now_v7()));
        std::fs::create_dir_all(&root)?;
        std::fs::write(root.join("verifier.yaml"), VERIFIER)?;
        std::fs::write(root.join("service.yaml"), SERVICE)?;
        let verifier = Box::pin(cards.register_from_path(&root.join("verifier.yaml"))).await?;
        let receipt = Box::pin(cards.register_from_path(&root.join("service.yaml"))).await?;
        std::fs::remove_dir_all(&root)?;
        let uid = |root: &CardRef| -> Result<String, McpJourneyError> {
            Ok(root
                .uid
                .as_ref()
                .ok_or("a registered Card has a UID")?
                .to_string())
        };
        Ok(Fixture {
            service_uid: uid(&receipt.root)?,
            verifier_uid: uid(&verifier.root)?,
            service_ref: receipt.root,
        })
    }

    /// A real MCP connection and registered shared Card graphs for investigation.
    struct CardInvestigation {
        /// Owns the listener, tenant fixtures, and shutdown lifecycle.
        server: WyrdTestServer,
        /// Administrator connection used for Card discovery and navigation.
        agent: RunningService<RoleClient, ()>,
        /// Exact root of the shared support desk graph.
        service: CardRef,
    }

    impl CardInvestigation {
        /// Register the shared support desk and observed Service graphs.
        ///
        /// # Errors
        /// Returns server, fixture registration, or connection failures.
        async fn start() -> Result<Self, McpJourneyError> {
            let server = WyrdTestServer::start_bound().await?;
            let admin = server
                .bootstrap_agent("mcp-card-discovery", &["admin"])
                .await?;
            let cards = Cards::with_client(client(&server, api_key(&admin)?)?);
            let fixtures = Path::new(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../../fixtures/cards"
            ));

            Box::pin(
                cards.register_from_path(&fixtures.join("register_and_hydrate/support-model.yaml")),
            )
            .await?;
            let desk = Box::pin(
                cards.register_from_path(&fixtures.join("register_and_hydrate/support-desk.yaml")),
            )
            .await?;
            Box::pin(cards.register_from_path(&fixtures.join("observe_a_run/observed-model.yaml")))
                .await?;
            Box::pin(
                cards.register_from_path(&fixtures.join("observe_a_run/observed-service.yaml")),
            )
            .await?;

            let agent = Self::connect(&server, &admin).await?;
            Ok(Self {
                server,
                agent,
                service: desk.root,
            })
        }

        /// Connect a principal through the production MCP transport.
        ///
        /// # Errors
        /// Returns credential or transport failures.
        async fn connect(
            server: &WyrdTestServer,
            principal: &Bootstrap,
        ) -> Result<RunningService<RoleClient, ()>, McpJourneyError> {
            Ok(
                ().serve_with_lifecycle(transport(server, api_key(principal)?, None)?, discover())
                    .await?,
            )
        }

        /// Read a typed Card page through MCP's advertised filters.
        ///
        /// # Errors
        /// Returns tool, transport, or response decoding failures.
        async fn list(&self, arguments: JsonValue) -> Result<ListCardsResponse, McpJourneyError> {
            let response = self.agent.call_tool(call("cards.list", arguments)?).await?;
            Ok(serde_json::from_value(structured(response)?)?)
        }

        /// Read one exact reference through MCP.
        ///
        /// # Errors
        /// Returns tool, transport, or response decoding failures.
        async fn get(&self, reference: &CardRef) -> Result<GetCardResponse, McpJourneyError> {
            let arguments = serde_json::json!({ "card_ref": reference });
            let response = self
                .agent
                .call_tool(call("cards.get_by_ref", arguments)?)
                .await?;
            Ok(serde_json::from_value(structured(response)?)?)
        }

        /// Select the named Service and follow opaque continuation without repeating a Card.
        ///
        /// # Errors
        /// Returns MCP or response decoding failures.
        async fn discover(&self) -> Result<(), McpJourneyError> {
            let named = self
                .list(serde_json::json!({ "kind": "Service", "name": "support-desk" }))
                .await?;
            assert_eq!(named.items.len(), 1);
            assert_eq!(Some(&named.items[0].card_uid), self.service.uid.as_ref());
            assert_eq!(named.items[0].version, self.service.version);

            let first = self
                .list(serde_json::json!({ "kind": "Service", "limit": 1 }))
                .await?;
            assert_eq!(first.items.len(), 1);
            let cursor = first
                .next_cursor
                .ok_or("two Services require continuation")?;
            let second = self
                .list(serde_json::json!({ "kind": "Service", "limit": 1, "cursor": cursor }))
                .await?;
            assert_eq!(second.items.len(), 1);
            assert_ne!(first.items[0].card_uid, second.items[0].card_uid);
            assert!(second.next_cursor.is_none());
            Ok(())
        }

        /// Follow the Service's Model and Agent edges, then their inbound references.
        ///
        /// # Errors
        /// Returns MCP or response decoding failures.
        async fn navigate(&self) -> Result<(), McpJourneyError> {
            let parent = self.get(&self.service).await?;
            let arguments = serde_json::json!({ "kind": "Service", "card_uid": self.service.uid });
            let response = self.agent.call_tool(call(CARDS_GET, arguments)?).await?;
            let by_uid: GetCardResponse = serde_json::from_value(structured(response)?)?;
            assert_eq!(parent, by_uid, "UID reads retain the full Card contract");

            let components = &parent.card.relationships.outbound_refs;
            for kind in [CardKind::Model, CardKind::Agent] {
                let edge = components
                    .iter()
                    .find(|edge| edge.card_ref.kind == kind)
                    .ok_or("Service has both component kinds")?;
                let component = self.get(&edge.card_ref).await?;
                assert_eq!(component.card.kind, kind);
                assert!(
                    component
                        .card
                        .relationships
                        .inbound_refs
                        .iter()
                        .any(|edge| edge.card_ref == self.service),
                    "component identifies its exact parent"
                );
            }
            Ok(())
        }

        /// Refuse malformed cursors, incomplete references, and missing Cards.
        ///
        /// # Errors
        /// Returns transport failures or an unexpectedly successful call.
        async fn reject_invalid_reads(&self) -> Result<(), McpJourneyError> {
            for (tool, arguments, code) in [
                (
                    "cards.list",
                    serde_json::json!({ "cursor": "invalid!" }),
                    "WYRD_REGISTRY_400_INVALID_CARD_SPEC",
                ),
                (
                    "cards.get_by_ref",
                    serde_json::json!({ "card_ref": { "kind": "Service", "name": "support-desk" } }),
                    "WYRD_SPEC_400_VALIDATION",
                ),
                (
                    "cards.get_by_ref",
                    serde_json::json!({ "card_ref": { "kind": "Service", "name": "absent", "version": "1.0.0", "space": "default" } }),
                    "WYRD_REGISTRY_404_CARD_NOT_FOUND",
                ),
            ] {
                let failure = refusal(self.agent.call_tool(call(tool, arguments)?).await)?;
                assert!(failure.contains(code), "{failure}");
            }
            Ok(())
        }

        /// A foreign tenant cannot list or resolve the registered Service.
        ///
        /// # Errors
        /// Returns tenant, principal, transport, or response failures.
        async fn isolate_tenants(&self) -> Result<(), McpJourneyError> {
            let tenant = self.server.seed_tenant("mcp-card-foreign").await?;
            let principal = self
                .server
                .bootstrap_service_in_tenant(tenant, "foreign", &["admin"])
                .await?;
            let foreign = Self::connect(&self.server, &principal).await?;

            let failure = refusal(
                foreign
                    .call_tool(call(
                        "cards.get_by_ref",
                        serde_json::json!({ "card_ref": self.service }),
                    )?)
                    .await,
            )?;
            assert!(
                failure.contains("WYRD_REGISTRY_404_CARD_NOT_FOUND"),
                "{failure}"
            );
            let response = foreign
                .call_tool(call(
                    "cards.list",
                    serde_json::json!({ "name": "support-desk" }),
                )?)
                .await?;
            let listed: ListCardsResponse = serde_json::from_value(structured(response)?)?;
            assert!(listed.items.is_empty());

            foreign.cancel().await?;
            Ok(())
        }

        /// Both new reads enforce cards:read even when called by name.
        ///
        /// # Errors
        /// Returns principal, transport, or response failures.
        async fn enforce_permissions(&self) -> Result<(), McpJourneyError> {
            self.server.seed_role("mcp_no_cards", &[]).await?;
            let principal = self
                .server
                .bootstrap_service("denied", &["mcp_no_cards"])
                .await?;
            let denied = Self::connect(&self.server, &principal).await?;

            for (tool, arguments) in [
                ("cards.list", serde_json::json!({})),
                (
                    "cards.get_by_ref",
                    serde_json::json!({ "card_ref": self.service }),
                ),
            ] {
                let failure = refusal(denied.call_tool(call(tool, arguments)?).await)?;
                assert!(
                    failure.contains("WYRD_PERMISSION_403_DENIED_RBAC"),
                    "{failure}"
                );
            }
            denied.cancel().await?;
            Ok(())
        }

        /// Close the connection before draining the server.
        ///
        /// # Errors
        /// Returns transport or server shutdown failures.
        async fn shutdown(self) -> Result<(), McpJourneyError> {
            self.agent.cancel().await?;
            self.server.shutdown().await?;
            Ok(())
        }
    }

    /// An agent discovers and navigates shared Card graphs through the real MCP endpoint.
    ///
    /// # Errors
    /// Returns fixture, MCP, or shutdown failures.
    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "requires the Postgres-backed Bifrost journey lane"]
    async fn agent_discovers_service_and_component_cards() -> Result<(), McpJourneyError> {
        let investigation = CardInvestigation::start().await?;
        investigation.discover().await?;
        investigation.navigate().await?;
        investigation.reject_invalid_reads().await?;
        investigation.isolate_tenants().await?;
        investigation.enforce_permissions().await?;
        investigation.shutdown().await
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
        let Fixture {
            service_uid,
            service_ref,
            ..
        } = register_fixture(&cards).await?;

        let writer = server
            .credential_registered_service(&service_ref, &["editor"])
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
            .bootstrap_service("mcp-run-reader", &["viewer"])
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
            "an agent without verifier:run is not offered the write tool: {reader_names:?}"
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

    /// Emit Drift observations of `service_ref` whose `score` averages 2.0
    /// against the fixture's 1.0 baseline, through one `WyrdState` lifetime.
    ///
    /// The Service's graph is hydrated into a temporary bundle with `admin`'s
    /// registry access, observations are written as the Service's own
    /// `workload`-role credential, and the lifetime is drained and the server
    /// Scribe flushed before returning, so every row is durable and readable.
    ///
    /// # Errors
    ///
    /// Returns hydration, credential, client, startup, emit, drain, or flush
    /// failures.
    async fn emit_drifted_scores(
        server: &WyrdTestServer,
        admin: &Cards,
        service_ref: &CardRef,
    ) -> Result<(), McpJourneyError> {
        let bundle = std::env::temp_dir().join(format!("wyrd-mcp-direct-{}", uuid::Uuid::now_v7()));
        Box::pin(CardGraphHydrator::new(admin.registry_context()).hydrate(
            &CardSelector::exact(service_ref.clone()),
            &bundle,
            HydrationMode::Complete,
        ))
        .await?;
        let emitter = server
            .credential_registered_service(service_ref, &[])
            .await?;
        let key = emitter
            .api_key()
            .ok_or("a registered Service carries a key")?
            .expose_secret()
            .to_owned();
        let client = client_from_options(
            server.base_url(),
            Some(key.as_str()),
            server.grpc_url().as_deref(),
        )?;
        let state = WyrdState::from_path_with_client(&bundle, client)?;
        state
            .start_bifrost_with_config(None, QueueConfig::default())
            .await?;
        let run = state.run();
        for score in [1.5_f64, 2.5, 1.5, 2.5] {
            run.observe()
                .drift(&serde_json::json!({ "score": score }), None)?;
        }
        state.shutdown().await?;
        server.flush_bifrost().await?;
        std::fs::remove_dir_all(&bundle)?;
        Ok(())
    }

    /// Poll `run_id` over MCP until it leaves pending, running, and retrying.
    ///
    /// # Errors
    ///
    /// Returns a tool-call failure, or an error when the run never settles
    /// within [`SETTLE`].
    async fn settle(
        client: &rmcp::service::RunningService<rmcp::RoleClient, ()>,
        run_id: &str,
    ) -> Result<JsonValue, McpJourneyError> {
        let deadline = tokio::time::Instant::now() + SETTLE;
        loop {
            let run = structured(
                client
                    .call_tool(call(GET_RUN, serde_json::json!({ "run_id": run_id }))?)
                    .await?,
            )?;
            if !matches!(
                run["status"].as_str(),
                Some("pending" | "running" | "retrying")
            ) {
                return Ok(run);
            }
            if tokio::time::Instant::now() >= deadline {
                return Err(format!("run never settled: {run}").into());
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }

    /// Run one `bifrost.query` over `sql` and return its rows.
    ///
    /// # Errors
    ///
    /// Returns a tool-call failure or a structured query refusal.
    async fn rows(
        client: &rmcp::service::RunningService<rmcp::RoleClient, ()>,
        sql: String,
    ) -> Result<JsonValue, McpJourneyError> {
        let answer = structured(
            client
                .call_tool(call(
                    QUERY,
                    serde_json::json!({ "sql": sql, "max_rows": 10 }),
                )?)
                .await?,
        )?;
        Ok(answer["rows"].clone())
    }

    /// A bound Service runs a Verifier directly over MCP and reads the
    /// persisted judgment back through `bifrost.query`.
    ///
    /// On a runtime-enabled server the Service emits drifted observations and
    /// reads the Verifier through `cards.get` as `kind: Verifier` with a
    /// `drift` implementation, while `Drift` and `Eval` are refused as Card
    /// kinds; it then starts a keyed direct Verifier/subject run; a keyed retry returns
    /// the same `run_id`. Polling `verification.get_run` reaches `completed`
    /// with a Bifrost `result_id`, the Service's own principal as
    /// `requested_by_principal_id`, and no Operator dispatch. `bifrost.query`
    /// returns exactly one failed summary row and one Custom feature row for
    /// that `result_id`, both with null `owner_card_uid` and `binding_id` and
    /// the exact subject UID. A reader is refused the start; another tenant's
    /// administrator can neither start the run nor read it back, and its
    /// `bifrost.query` reads its own provisioned, empty result table.
    ///
    /// # Errors
    ///
    /// Returns server startup, bootstrap, registration, emit, MCP transport,
    /// or tool-call failures, or an error when the run never settles.
    ///
    /// # Panics
    ///
    /// Panics when any status, identity, row, or refusal expectation fails.
    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "requires the Postgres-backed Bifrost journey lane"]
    async fn an_agent_runs_a_verifier_directly_and_reads_its_result() -> Result<(), McpJourneyError>
    {
        let server = Box::pin(
            WyrdTestServer::builder()
                .with_verification_runtime_for_test()
                .start_bound(),
        )
        .await?;
        let admin = server
            .bootstrap_service("mcp-direct-admin", &["admin"])
            .await?;
        let cards = Cards::with_client(client(&server, api_key(&admin)?)?);
        let fixture = register_fixture(&cards).await?;
        emit_drifted_scores(&server, &cards, &fixture.service_ref).await?;

        // A direct run reads the tenant's Bifrost observations, which only the
        // explicitly granted `workload` role allows.
        let agent = server
            .credential_registered_service(&fixture.service_ref, &["editor"])
            .await?;
        let agent_client =
            ().serve_with_lifecycle(transport(&server, api_key(&agent)?, None)?, discover())
                .await?;
        let verifier = structured(
            agent_client
                .call_tool(call(
                    CARDS_GET,
                    serde_json::json!({ "kind": "Verifier", "card_uid": fixture.verifier_uid }),
                )?)
                .await?,
        )?;
        assert_eq!(text(&verifier, "/card/kind")?, "Verifier");
        assert_eq!(text(&verifier, "/card/spec/implementation/kind")?, "drift");
        for retired in ["Drift", "Eval"] {
            refusal(
                agent_client
                    .call_tool(call(
                        CARDS_GET,
                        serde_json::json!({ "kind": retired, "card_uid": fixture.verifier_uid }),
                    )?)
                    .await,
            )?;
        }

        let now = chrono::Utc::now();
        let start = serde_json::json!({
            "target": {
                "kind": "verifier",
                "verifier_uid": fixture.verifier_uid,
                "subject_card_uid": fixture.service_uid,
            },
            "input": {
                "kind": "drift_window",
                "start": now - chrono::Duration::hours(1),
                "end": now + chrono::Duration::hours(1),
            },
            "idempotency_key": "mcp-direct-0001",
        });
        let started = structured(
            agent_client
                .call_tool(call(START_RUN, start.clone())?)
                .await?,
        )?;
        let run_id = text(&started, "/run_id")?.to_owned();
        let replayed = structured(
            agent_client
                .call_tool(call(START_RUN, start.clone())?)
                .await?,
        )?;
        assert_eq!(
            text(&replayed, "/run_id")?,
            run_id,
            "a keyed retry returns the same direct run"
        );

        let run = settle(&agent_client, &run_id).await?;
        assert_eq!(run["status"], "completed", "{run}");
        assert_eq!(
            text(&run, "/requested_by_principal_id")?,
            agent.id().to_string(),
            "the run records the authenticated requester: {run}"
        );
        assert_eq!(
            run["dispatches"],
            serde_json::json!([]),
            "a direct run never dispatches: {run}"
        );
        let result_id = text(&run, "/result_id")?.to_owned();

        let summary = rows(
            &agent_client,
            format!(
                "SELECT execution_status, verdict, subject_card_uid, owner_card_uid, binding_id \
                 FROM vala.verification.results WHERE result_id = '{result_id}'"
            ),
        )
        .await?;
        assert_eq!(
            summary,
            serde_json::json!([[
                "completed",
                "failed",
                fixture.service_uid,
                JsonValue::Null,
                JsonValue::Null
            ]]),
            "one direct summary with null owner and binding"
        );
        let features = rows(
            &agent_client,
            format!(
                "SELECT method, feature, verdict, subject_card_uid, owner_card_uid, binding_id \
                 FROM vala.drift.result_features WHERE result_id = '{result_id}'"
            ),
        )
        .await?;
        assert_eq!(
            features,
            serde_json::json!([[
                "Custom",
                "score",
                "drift",
                fixture.service_uid,
                JsonValue::Null,
                JsonValue::Null
            ]]),
            "one direct feature row with null owner and binding"
        );
        agent_client.cancel().await?;

        let reader = server
            .bootstrap_service("mcp-direct-reader", &["viewer"])
            .await?;
        let reader_client =
            ().serve_with_lifecycle(transport(&server, api_key(&reader)?, None)?, discover())
                .await?;
        let denied = refusal(
            reader_client
                .call_tool(call(START_RUN, start.clone())?)
                .await,
        )?;
        assert!(
            denied.contains("WYRD_PERMISSION_403_DENIED_RBAC"),
            "a caller without verifier:run is refused: {denied}"
        );
        reader_client.cancel().await?;

        let other_tenant = server.seed_tenant("mcp-direct-other").await?;
        let foreign = server
            .bootstrap_service_in_tenant(other_tenant, "mcp-direct-foreign", &["admin"])
            .await?;
        let foreign_client =
            ().serve_with_lifecycle(transport(&server, api_key(&foreign)?, None)?, discover())
                .await?;
        let foreign_start = refusal(foreign_client.call_tool(call(START_RUN, start)?).await)?;
        assert!(
            foreign_start.contains("WYRD_VERIFICATION_400_INVALID_TARGET"),
            "another tenant cannot run this tenant's Verifier: {foreign_start}"
        );
        let foreign_read = refusal(
            foreign_client
                .call_tool(call(GET_RUN, serde_json::json!({ "run_id": run_id }))?)
                .await,
        )?;
        assert!(
            foreign_read.contains("WYRD_VERIFICATION_404_RUN_NOT_FOUND"),
            "another tenant cannot read the run: {foreign_read}"
        );
        let foreign_rows = rows(
            &foreign_client,
            "SELECT result_id FROM vala.verification.results".to_owned(),
        )
        .await?;
        assert_eq!(
            foreign_rows,
            serde_json::json!([]),
            "no result crosses tenants"
        );
        foreign_client.cancel().await?;
        server.shutdown().await?;
        Ok(())
    }

    /// A bound Service judges supplied samples with its Verifier over MCP.
    ///
    /// The tool result carries the exact Verifier and subject, the `passed`
    /// or `failed` verdict, and the Drift detail; nothing is enqueued. A
    /// reader and a caller holding only `evals:run` are neither offered the
    /// tool nor allowed it, and another tenant's administrator cannot resolve
    /// this tenant's Cards.
    ///
    /// # Errors
    ///
    /// Returns server startup, bootstrap, registration, MCP transport, or
    /// tool-call failures.
    ///
    /// # Panics
    ///
    /// Panics when any verdict, identity, or refusal expectation fails.
    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "requires the Postgres-backed Bifrost journey lane"]
    async fn an_agent_executes_a_verifier_on_supplied_samples() -> Result<(), McpJourneyError> {
        let server = Box::pin(WyrdTestServer::start_bound()).await?;
        let admin = server
            .bootstrap_service("mcp-execute-admin", &["admin"])
            .await?;
        let cards = Cards::with_client(client(&server, api_key(&admin)?)?);
        let fixture = register_fixture(&cards).await?;
        let agent = server
            .credential_registered_service(&fixture.service_ref, &["editor"])
            .await?;
        let agent_client =
            ().serve_with_lifecycle(transport(&server, api_key(&agent)?, None)?, discover())
                .await?;
        let request = |score: f64| {
            serde_json::json!({
                "verifier_uid": fixture.verifier_uid,
                "subject_card_uid": fixture.service_uid,
                "input": { "kind": "drift_samples", "columns": { "score": [score] } },
            })
        };
        for (score, verdict) in [(1.2, "passed"), (3.0, "failed")] {
            let judged = structured(
                agent_client
                    .call_tool(call(EXECUTE, request(score))?)
                    .await?,
            )?;
            assert_eq!(judged["verdict"], verdict, "{judged}");
            assert_eq!(judged["kind"], "drift_custom", "{judged}");
            assert_eq!(text(&judged, "/verifier/uid")?, fixture.verifier_uid);
            assert_eq!(text(&judged, "/subject/uid")?, fixture.service_uid);
            assert!(judged["detail"]["drift"].is_object(), "{judged}");
        }
        agent_client.cancel().await?;

        server
            .seed_role(
                "mcp_evals_only",
                &[Permission::card_read(), Permission::eval_run()],
            )
            .await?;
        for (name, role) in [
            ("mcp-execute-reader", "viewer"),
            ("mcp-execute-evals-only", "mcp_evals_only"),
        ] {
            let caller = server.bootstrap_service(name, &[role]).await?;
            let caller_client =
                ().serve_with_lifecycle(transport(&server, api_key(&caller)?, None)?, discover())
                    .await?;
            let tools = caller_client.list_all_tools().await?;
            assert!(
                !tools.iter().any(|tool| tool.name == EXECUTE),
                "{role} is not offered the execute tool"
            );
            let denied = refusal(caller_client.call_tool(call(EXECUTE, request(1.0))?).await)?;
            assert!(
                denied.contains("WYRD_PERMISSION_403_DENIED_RBAC"),
                "a caller without verifier:run is refused: {role}: {denied}"
            );
            caller_client.cancel().await?;
        }

        let other_tenant = server.seed_tenant("mcp-execute-other").await?;
        let foreign = server
            .bootstrap_service_in_tenant(other_tenant, "mcp-execute-foreign", &["admin"])
            .await?;
        let foreign_client =
            ().serve_with_lifecycle(transport(&server, api_key(&foreign)?, None)?, discover())
                .await?;
        let foreign_execute =
            refusal(foreign_client.call_tool(call(EXECUTE, request(1.0))?).await)?;
        assert!(
            foreign_execute.contains("WYRD_VERIFICATION_404_TARGET_NOT_FOUND"),
            "another tenant cannot execute this tenant's Verifier: {foreign_execute}"
        );
        foreign_client.cancel().await?;
        server.shutdown().await?;
        Ok(())
    }
}
