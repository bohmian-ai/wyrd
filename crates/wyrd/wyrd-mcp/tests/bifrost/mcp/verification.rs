//! Tier-1 journey: an agent discovers a binding, executes and enqueues
//! Verifiers, and observes executions over `/mcp`.
//!
//! A registered Service acting as itself reads its own Card, finds the binding
//! its `verified_by` projected, reads the binding's readiness, and enqueues a
//! keyed execution whose replay keeps its identity — every step through a
//! real `rmcp` client. A reader is not offered the write tools and is refused
//! when it names one anyway.
//!
//! On a runtime-enabled server the same Service enqueues an assertion
//! Verifier over inline inputs, polls `verification.get_execution` to a
//! terminal status, and reads the persisted result through `bifrost.query`.
//!
//! Through `verification.execute` the Service also judges inline inputs and
//! receives the result in the tool result, plans an evidence upload, and is
//! refused direct execution of a drift Verifier.
//!
//! A tool call carrying the caller's `traceparent` continues the caller's
//! trace: the request span and the verification, judge, and gateway work it
//! does descend from the caller's span.

/// The Postgres-backed half of the verification journey.
mod pg_tests {
    use crate::connectivity::{
        McpJourneyError, client, client_transport, discover, problem, refusal, structured,
        transport,
    };

    use std::collections::BTreeSet;
    use std::path::Path;
    use std::time::Duration;

    use rmcp::ClientServiceExt as _;
    use rmcp::model::CallToolRequestParams;
    use rmcp::service::{RoleClient, RunningService};
    use secrecy::ExposeSecret as _;
    use serde_json::Value as JsonValue;
    use wyrd_client::cards::Cards;
    use wyrd_client::transport::credential::ResolvedCredential;
    use wyrd_runtime::Permission;
    use wyrd_spec::auth::TokenAudience;
    use wyrd_spec::envelope::CardKind;
    use wyrd_spec::reference::CardRef;
    use wyrd_spec::registry::{GetCardResponse, ListCardsResponse};
    use wyrd_testing::{Bootstrap, WyrdTestServer};

    /// The Card read tool an agent uses to find binding IDs.
    const CARDS_GET: &str = "cards.get";

    /// The binding status read tool.
    const GET_BINDING: &str = "verification.get_binding";

    /// The execution status read tool.
    const GET_EXECUTION: &str = "verification.get_execution";

    /// The evidence upload planning tool, offered only with `verifier:run`.
    const EVIDENCE: &str = "verification.evidence";

    /// The direct execution write tool, offered only with `verifier:run`.
    const EXECUTE: &str = "verification.execute";

    /// The queued execution write tool, offered only with `verifier:run`.
    const ENQUEUE: &str = "verification.enqueue";

    /// Every write tool a caller holding `verifier:run` is offered.
    const WRITE_TOOLS: [&str; 3] = [EVIDENCE, EXECUTE, ENQUEUE];

    /// The existing Bifrost read tool verdicts are read through.
    const QUERY: &str = "bifrost.query";

    /// Upper bound on the wait for the server-side runtime to settle an
    /// execution.
    const SETTLE: Duration = Duration::from_secs(90);

    /// A ready Custom Drift Verifier the Service binds on a schedule.
    const VERIFIER: &str = "apiVersion: wyrd/v1\nkind: Verifier\nmetadata:\n  name: mcp-run-drift\n  version: 1.0.0\nspec:\n  inputs:\n    scores: {type: samples, features: [score]}\n  requirements:\n    - id: score-steady\n      statement: The score stays at or under its alert threshold\n      check:\n        kind: drift\n        on: inputs.scores\n        method: Custom\n        signal: {kind: Metric, name: score}\n        condition: {kind: Statistical}\n        profile: {kind: Custom, metric_name: score, baseline_value: 1.0, alert_threshold: 0.5}\n";

    /// A Service bound to [`VERIFIER`] on a schedule over its `score` metric.
    const SERVICE: &str = "apiVersion: wyrd/v1\nkind: Service\nmetadata:\n  name: mcp-run-service\n  version: 1.0.0\nspec:\n  verified_by:\n    - verifier:\n        kind: Verifier\n        name: mcp-run-drift\n        version: 1.0.0\n      runs_on:\n        kind: schedule\n        cron: \"0 2 * * *\"\n";

    /// SHA-256 of the five bytes `hello`, the evidence file the journey plans.
    const HELLO_SHA256: &str = "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824";

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

    /// The registered fixture: the Service subject and the Verifiers it is
    /// judged by.
    struct Fixture {
        /// The Service's server-minted UID.
        service_uid: String,
        /// The Service's exact UID-bearing reference.
        service_ref: CardRef,
        /// The bound Custom Drift Verifier's server-minted UID.
        verifier_uid: String,
        /// The `no-refund-promise` assertion Verifier's server-minted UID.
        assert_uid: String,
    }

    impl Fixture {
        /// A request judging `answer` with the assertion Verifier against the
        /// Service, in the shared `execute`/`enqueue` body shape.
        fn assert_request(&self, answer: &str) -> JsonValue {
            serde_json::json!({
                "verifier_card_uid": self.assert_uid,
                "card_uid": self.service_uid,
                "inputs": { "answer": { "kind": "json", "value": answer } },
            })
        }
    }

    /// An answer the `no-refund-promise` assertion passes.
    const PASSING_ANSWER: &str = "We will review your request.";

    /// An answer the `no-refund-promise` assertion fails.
    const FAILING_ANSWER: &str = "You get a guaranteed refund.";

    /// Register the fixture Verifiers and Service.
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
        let verifier = Box::pin(cards.declare_from_path(&root.join("verifier.yaml"))).await?;
        let receipt = Box::pin(cards.declare_from_path(&root.join("service.yaml"))).await?;
        std::fs::remove_dir_all(&root)?;
        let assertion = Box::pin(cards.declare_from_path(Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../fixtures/cards/declare_and_pull/no-refund-promise.yaml"
        ))))
        .await?;
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
            assert_uid: uid(&assertion.root)?,
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
        /// Connecting the administrator's Cards handle exchanges its API key
        /// eagerly, so a refused or unreachable exchange is returned here
        /// before any fixture registers.
        ///
        /// # Errors
        /// Returns server start, bootstrap, client configuration, API-key
        /// exchange or connection, fixture registration, or MCP connection
        /// failures.
        async fn start() -> Result<Self, McpJourneyError> {
            let server = WyrdTestServer::start_bound().await?;
            let admin = server
                .bootstrap_service("mcp-card-discovery", &["admin"])
                .await?;
            let cards = Cards::connect_with(&client(&server, api_key(&admin)?)?).await?;
            let fixtures = Path::new(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../../fixtures/cards"
            ));

            Box::pin(
                cards.declare_from_path(&fixtures.join("declare_and_pull/support-model.yaml")),
            )
            .await?;
            let desk = Box::pin(
                cards.declare_from_path(&fixtures.join("declare_and_pull/support-desk.yaml")),
            )
            .await?;
            Box::pin(cards.declare_from_path(&fixtures.join("observe_a_run/observed-model.yaml")))
                .await?;
            Box::pin(
                cards.declare_from_path(&fixtures.join("observe_a_run/observed-service.yaml")),
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
            let response = self.agent.call_tool(call(CARDS_GET, arguments)?).await?;
            Ok(serde_json::from_value(structured(response)?)?)
        }

        /// Select the named Service, then follow opaque continuation over every
        /// Service, the administrator's fixture Service included, without
        /// repeating or skipping one.
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

            // The two shared graphs' Services and the administrator's own
            // fixture Service.
            let all = self.list(serde_json::json!({ "kind": "Service" })).await?;
            assert_eq!(all.items.len(), 3, "{all:?}");
            assert!(all.next_cursor.is_none());
            let mut paged = Vec::new();
            let mut arguments = serde_json::json!({ "kind": "Service", "limit": 1 });
            loop {
                let page = self.list(arguments.clone()).await?;
                assert_eq!(page.items.len(), 1);
                paged.push(page.items[0].card_uid.clone());
                let Some(cursor) = page.next_cursor else {
                    break;
                };
                arguments["cursor"] = serde_json::json!(cursor);
            }
            let listed: Vec<_> = all.items.iter().map(|item| item.card_uid.clone()).collect();
            assert_eq!(paged, listed, "continuation repeats and skips no Service");
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

        /// Refuse malformed cursors, incomplete, ambiguous, or partial selectors, and missing Cards.
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
                    CARDS_GET,
                    serde_json::json!({ "card_ref": { "kind": "Service", "name": "support-desk" } }),
                    "WYRD_SPEC_400_VALIDATION",
                ),
                (
                    CARDS_GET,
                    serde_json::json!({ "card_ref": self.service, "kind": "Service", "card_uid": self.service.uid }),
                    "WYRD_SPEC_400_VALIDATION",
                ),
                (
                    CARDS_GET,
                    serde_json::json!({ "kind": "Service" }),
                    "WYRD_SPEC_400_VALIDATION",
                ),
                (
                    CARDS_GET,
                    serde_json::json!({ "card_ref": { "kind": "Service", "name": "absent", "version": "1.0.0" } }),
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
                        CARDS_GET,
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
            self.server
                .seed_role::<serde_json::Value>("mcp_no_cards", &[])
                .await?;
            let principal = self
                .server
                .bootstrap_service("denied", &["mcp_no_cards"])
                .await?;
            let denied = Self::connect(&self.server, &principal).await?;

            for (tool, arguments) in [
                ("cards.list", serde_json::json!({})),
                (CARDS_GET, serde_json::json!({ "card_ref": self.service })),
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

    /// Connect `principal` to `server` over the production MCP transport.
    ///
    /// # Errors
    ///
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

    /// The first-party client for a family runtime account, bound to the root
    /// its registered Card names, since owned work needs that root selected.
    ///
    /// # Errors
    ///
    /// Returns an error when `principal` names no Card uid or the client cannot
    /// be constructed.
    fn rooted_client(
        server: &WyrdTestServer,
        principal: &Bootstrap,
    ) -> Result<wyrd_client::WyrdClient, McpJourneyError> {
        let root = principal
            .card_ref()
            .and_then(|card| card.uid.clone())
            .ok_or("a family runtime account names its registered root")?;
        Ok(client(server, api_key(principal)?)?.for_workload(root))
    }

    /// Connect an MCP client as a family runtime account at its root.
    ///
    /// # Errors
    ///
    /// Returns client construction or MCP handshake failures.
    async fn connect_rooted(
        server: &WyrdTestServer,
        principal: &Bootstrap,
    ) -> Result<RunningService<RoleClient, ()>, McpJourneyError> {
        let base_url = server
            .base_url()
            .ok_or("journey requires a bound test server")?;
        Ok(()
            .serve_with_lifecycle(
                client_transport(base_url, &rooted_client(server, principal)?, []),
                discover(),
            )
            .await?)
    }

    /// Render a refused tool call, whether `rmcp` returned a transport error
    /// or a tool result carrying a problem.
    ///
    /// # Errors
    ///
    /// Returns an error when a tool result carries no problem body.
    fn rendered(
        refused: Result<rmcp::model::CallToolResult, rmcp::ServiceError>,
    ) -> Result<String, McpJourneyError> {
        Ok(match refused {
            Err(error) => error.to_string(),
            Ok(result) => problem(result)?.to_string(),
        })
    }

    /// A bound Service discovers its binding and enqueues a keyed execution
    /// whose replay keeps its identity over MCP; a reader is not offered the
    /// write tools and is refused.
    ///
    /// # Errors
    ///
    /// Returns server startup, bootstrap, registration, MCP transport, or
    /// tool-call failures.
    ///
    /// # Panics
    /// Panics when the administrator's Cards handle cannot connect (API-key
    /// exchange refused or unreachable), or when an assertion fails: the
    /// writer is not offered a required tool, the binding names another
    /// subject or is not ready, a keyed replay returns another execution id,
    /// the reader loses a read tool or is offered a write tool, or the
    /// reader's write call is not the stable RBAC refusal.
    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "requires the Postgres-backed Bifrost journey lane"]
    async fn an_agent_discovers_a_binding_and_enqueues_a_keyed_execution()
    -> Result<(), McpJourneyError> {
        let server = WyrdTestServer::start_bound().await?;
        let admin = server
            .bootstrap_service("mcp-run-admin", &["admin"])
            .await?;
        let cards = Cards::connect_with(&client(&server, api_key(&admin)?)?)
            .await
            .expect("handle connects");
        let fixture = register_fixture(&cards).await?;

        let writer = server
            .credential_registered_service(&fixture.service_ref, &["editor"])
            .await?;
        let writer_client = connect_rooted(&server, &writer).await?;
        let tools = writer_client.list_all_tools().await?;
        let names: Vec<&str> = tools.iter().map(|tool| tool.name.as_ref()).collect();
        for tool in [CARDS_GET, GET_BINDING, GET_EXECUTION]
            .into_iter()
            .chain(WRITE_TOOLS)
        {
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
                    serde_json::json!({ "kind": "Service", "card_uid": fixture.service_uid }),
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
        assert_eq!(text(&binding, "/card_uid")?, fixture.service_uid);
        assert_eq!(text(&binding, "/readiness")?, "ready");

        // Act: a keyed enqueue, then a replay that returns the same execution.
        let mut enqueue = fixture.assert_request(PASSING_ANSWER);
        enqueue["idempotency_key"] = "mcp-journey-0001".into();
        let accepted = structured(
            writer_client
                .call_tool(call(ENQUEUE, enqueue.clone())?)
                .await?,
        )?;
        let execution_id = text(&accepted, "/execution_id")?.to_owned();
        let replayed = structured(
            writer_client
                .call_tool(call(ENQUEUE, enqueue.clone())?)
                .await?,
        )?;
        assert_eq!(
            text(&replayed, "/execution_id")?,
            execution_id,
            "a keyed retry returns the same execution"
        );
        writer_client.cancel().await?;

        // A reader keeps the read tools but is neither offered nor allowed the write tools.
        let reader = server
            .bootstrap_service("mcp-run-reader", &["viewer"])
            .await?;
        let reader_client = connect(&server, &reader).await?;
        let reader_tools = reader_client.list_all_tools().await?;
        let reader_names: Vec<&str> = reader_tools.iter().map(|tool| tool.name.as_ref()).collect();
        assert!(
            reader_names.contains(&GET_EXECUTION),
            "read tools stay available: {reader_names:?}"
        );
        for tool in WRITE_TOOLS {
            assert!(
                !reader_names.contains(&tool),
                "an agent without verifier:run is not offered {tool}: {reader_names:?}"
            );
        }
        let refused = rendered(reader_client.call_tool(call(ENQUEUE, enqueue)?).await)?;
        assert!(
            refused.contains("WYRD_PERMISSION_403_DENIED_RBAC"),
            "naming the write tool anyway is the stable permission refusal: {refused}"
        );
        reader_client.cancel().await?;
        server.shutdown().await?;
        Ok(())
    }

    /// Poll `execution_id` over MCP until it reaches a terminal status, and
    /// a `completed` one until its result is visible.
    ///
    /// An enqueued execution becomes visible only once the queue accepts its
    /// submission, so a not-found read is retried until the deadline; a
    /// `completed` execution may briefly lack its result, so that is retried
    /// too.
    ///
    /// # Errors
    ///
    /// Returns a tool-call failure other than delayed visibility, or an error
    /// when the execution never settles within [`SETTLE`].
    async fn settle(
        client: &RunningService<RoleClient, ()>,
        execution_id: &str,
    ) -> Result<JsonValue, McpJourneyError> {
        let deadline = tokio::time::Instant::now() + SETTLE;
        loop {
            let read = client
                .call_tool(call(
                    GET_EXECUTION,
                    serde_json::json!({ "execution_id": execution_id }),
                )?)
                .await;
            let state = match read {
                Ok(result) if result.is_error != Some(true) => Some(structured(result)?),
                other => {
                    let refused = rendered(other)?;
                    if !refused.contains("WYRD_VERIFICATION_404_EXECUTION_NOT_FOUND") {
                        return Err(refused.into());
                    }
                    None
                }
            };
            if let Some(state) = state
                && !matches!(
                    state["status"].as_str(),
                    Some("pending" | "running" | "retrying")
                )
                && (state["status"] != "completed" || state.get("result").is_some())
            {
                return Ok(state);
            }
            if tokio::time::Instant::now() >= deadline {
                return Err(format!("execution {execution_id} never settled").into());
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
        client: &RunningService<RoleClient, ()>,
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

    /// A bound Service enqueues a Verifier over MCP and reads the persisted
    /// judgment back through `verification.get_execution` and
    /// `bifrost.query`.
    ///
    /// On a runtime-enabled server the Service reads the Verifier through
    /// `cards.get` as `kind: Verifier`, while `Drift` and `Eval` are refused
    /// as Card kinds; it then enqueues a keyed assertion execution over an
    /// inline answer; a keyed retry returns the same `execution_id`. Polling
    /// `verification.get_execution` reaches `completed` with a failed result
    /// naming the exact Verifier and subject, and no Operator dispatch.
    /// `bifrost.query` returns exactly one failed summary row for that
    /// execution, stamped with the calling Service as `workload_card_uid`, a null
    /// `binding_id` and the exact subject UID. A reader is refused the enqueue; another tenant's
    /// administrator cannot read the execution back, and its `bifrost.query`
    /// reads its own provisioned, empty result table.
    ///
    /// # Errors
    ///
    /// Returns server startup, bootstrap, registration, MCP transport, or
    /// tool-call failures, or an error when the execution never settles.
    ///
    /// # Panics
    ///
    /// Panics when any status, identity, row, or refusal expectation fails.
    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "requires the Postgres-backed Bifrost journey lane"]
    async fn an_agent_enqueues_a_verifier_and_reads_its_result() -> Result<(), McpJourneyError> {
        let server = Box::pin(
            WyrdTestServer::builder()
                .with_verification_runtime_for_test()
                .start_bound(),
        )
        .await?;
        let admin = server
            .bootstrap_service("mcp-queued-admin", &["admin"])
            .await?;
        let cards = Cards::connect_with(&client(&server, api_key(&admin)?)?)
            .await
            .expect("handle connects");
        let fixture = register_fixture(&cards).await?;

        let agent = server
            .credential_registered_service(&fixture.service_ref, &["editor"])
            .await?;
        let agent_client = connect_rooted(&server, &agent).await?;
        let verifier = structured(
            agent_client
                .call_tool(call(
                    CARDS_GET,
                    serde_json::json!({ "kind": "Verifier", "card_uid": fixture.assert_uid }),
                )?)
                .await?,
        )?;
        assert_eq!(text(&verifier, "/card/kind")?, "Verifier");
        for retired in ["Drift", "Eval"] {
            refusal(
                agent_client
                    .call_tool(call(
                        CARDS_GET,
                        serde_json::json!({ "kind": retired, "card_uid": fixture.assert_uid }),
                    )?)
                    .await,
            )?;
        }

        let mut enqueue = fixture.assert_request(FAILING_ANSWER);
        enqueue["idempotency_key"] = "mcp-queued-0001".into();
        let accepted = structured(
            agent_client
                .call_tool(call(ENQUEUE, enqueue.clone())?)
                .await?,
        )?;
        let execution_id = text(&accepted, "/execution_id")?.to_owned();
        let replayed = structured(
            agent_client
                .call_tool(call(ENQUEUE, enqueue.clone())?)
                .await?,
        )?;
        assert_eq!(
            text(&replayed, "/execution_id")?,
            execution_id,
            "a keyed retry returns the same queued execution"
        );

        let state = settle(&agent_client, &execution_id).await?;
        assert_eq!(state["status"], "completed", "{state}");
        assert_eq!(text(&state, "/result/execution_id")?, execution_id);
        assert_eq!(state["result"]["outcome"]["verdict"], "failed", "{state}");
        assert_eq!(text(&state, "/result/verifier/uid")?, fixture.assert_uid);
        assert_eq!(text(&state, "/result/subject/uid")?, fixture.service_uid);
        assert_eq!(
            state["dispatches"],
            serde_json::json!([]),
            "an execution with no binding never dispatches: {state}"
        );
        // The judgment is staged on the Scribe outbox when the execution
        // completes; settle it into Bifrost before reading it back.
        server.flush_bifrost().await?;

        let summary = rows(
            &agent_client,
            format!(
                "SELECT execution_status, verdict, subject_card_uid, workload_card_uid, binding_id \
                 FROM bifrost.verification.results WHERE execution_id = '{execution_id}'"
            ),
        )
        .await?;
        assert_eq!(
            summary,
            serde_json::json!([[
                "completed",
                "failed",
                fixture.service_uid,
                fixture.service_uid,
                JsonValue::Null
            ]]),
            "one summary owned by the calling Service, with no binding"
        );
        agent_client.cancel().await?;

        let reader = server
            .bootstrap_service("mcp-queued-reader", &["viewer"])
            .await?;
        let reader_client = connect(&server, &reader).await?;
        let denied = refusal(
            reader_client
                .call_tool(call(ENQUEUE, enqueue.clone())?)
                .await,
        )?;
        assert!(
            denied.contains("WYRD_PERMISSION_403_DENIED_RBAC"),
            "a caller without verifier:run is refused: {denied}"
        );
        reader_client.cancel().await?;

        let other_tenant = server.seed_tenant("mcp-queued-other").await?;
        let foreign = server
            .bootstrap_service_in_tenant(other_tenant, "mcp-queued-foreign", &["admin"])
            .await?;
        let foreign_client = connect(&server, &foreign).await?;
        let foreign_read = refusal(
            foreign_client
                .call_tool(call(
                    GET_EXECUTION,
                    serde_json::json!({ "execution_id": execution_id }),
                )?)
                .await,
        )?;
        assert!(
            foreign_read.contains("WYRD_VERIFICATION_404_EXECUTION_NOT_FOUND"),
            "another tenant cannot read the execution: {foreign_read}"
        );
        let foreign_rows = rows(
            &foreign_client,
            "SELECT result_id FROM bifrost.verification.results".to_owned(),
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

    /// An agent reads a scheduled drift execution back by its ID: an empty
    /// window settles `inconclusive` through `verification.get_execution`
    /// with its drift report, and `bifrost.query` shows its one `schedule`
    /// input record freezing an empty population and its one output record
    /// carrying that requirement detail under the summary's result ID.
    ///
    /// # Errors
    ///
    /// Returns server startup, fixture, bootstrap, MCP transport, or
    /// tool-call failures, or an error when no execution appears or settles.
    ///
    /// # Panics
    ///
    /// Panics when the status, input record, or output record differs.
    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "requires the Postgres-backed Bifrost journey lane"]
    async fn scheduled_drift_execution_reads_its_inputs_and_output() -> Result<(), McpJourneyError>
    {
        let server = Box::pin(
            WyrdTestServer::builder()
                .with_verification_runtime_for_test()
                .start_bound(),
        )
        .await?;
        let seed = server.verification_fixture().await?;
        let verifier = seed
            .custom_drift_verifier("mcp-window", "latency", 100.0, 10.0)
            .await?;
        let (owner, principal) = seed.service("mcp-window-owner").await?;
        let binding = seed
            .bind_schedule(&owner, &owner, &verifier, "0 2 * * *", Vec::new())
            .await?;
        seed.activate(principal, &owner).await?;
        seed.make_binding_due(binding).await?;
        let deadline = tokio::time::Instant::now() + SETTLE;
        let execution_id = loop {
            if let Some(execution) = seed.executions().await?.into_iter().next() {
                break execution.to_string();
            }
            if tokio::time::Instant::now() >= deadline {
                return Err("the due schedule never created an execution".into());
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        };

        let agent = server
            .bootstrap_service("mcp-window-agent", &["admin"])
            .await?;
        let client = connect(&server, &agent).await?;
        let state = settle(&client, &execution_id).await?;
        assert_eq!(state["status"], "completed", "{state}");
        assert_eq!(
            state["result"]["outcome"]["verdict"], "inconclusive",
            "{state}"
        );
        let detail = &state["result"]["outcome"]["requirements"][0]["detail"];
        assert!(detail["drift"].is_object(), "{state}");
        server.flush_bifrost().await?;

        let inputs = rows(
            &client,
            format!(
                "SELECT origin, inputs FROM bifrost.verification.execution_inputs \
                 WHERE execution_id = '{execution_id}'"
            ),
        )
        .await?;
        assert_eq!(inputs[0][0], "schedule", "{inputs}");
        assert_eq!(
            (inputs.as_array().map(Vec::len), &inputs[0][1]["record_ids"]),
            (Some(1), &serde_json::json!([])),
            "one record freezing the empty window: {inputs}"
        );
        let outputs = rows(
            &client,
            format!(
                "SELECT o.output FROM bifrost.verification.results r \
                 JOIN bifrost.verification.execution_outputs o ON o.result_id = r.result_id \
                 WHERE r.execution_id = '{execution_id}'"
            ),
        )
        .await?;
        assert_eq!(outputs.as_array().map(Vec::len), Some(1), "{outputs}");
        assert_eq!(
            &outputs[0][0]["requirements"][0]["detail"], detail,
            "the output holds the detail the status read returns"
        );
        client.cancel().await?;
        server.shutdown().await?;
        Ok(())
    }

    /// A code-check Verifier whose check declares a numeric `score` as its
    /// custom Verdict data and returns a score its schema refuses for `BAD`.
    const SCORED: &str = "apiVersion: wyrd/v1\nkind: Verifier\nmetadata:\n  name: mcp-scored\n  version: 1.0.0\nspec:\n  inputs:\n    answer: {type: json}\n  requirements:\n    - id: scored\n      statement: The answer is scored\n      check:\n        kind: code\n        entrypoint: check\n        inputs: {answer: inputs.answer}\n        response_format:\n          kind: verdict\n          data:\n            type: object\n            properties: {score: {type: number}}\n            required: [score]\n            additionalProperties: false\n        source:\n          inline: |\n            def check(answer):\n                score = \"high\" if answer == \"BAD\" else 0.5\n                return {\"outcome\": \"failed\", \"reason\": \"scored\", \"citations\": [{\"slot\": \"answer\", \"locator\": {\"json_pointer\": \"\"}}], \"data\": {\"score\": score}}\n";

    /// Custom Verdict data round-trips through every MCP result path: the
    /// direct `verification.execute` result, a queued execution read through
    /// `verification.get_execution`, and the retained outcome read through
    /// `bifrost.query`. Data the declared schema refuses leaves the
    /// requirement inconclusive with no data and no echoed payload; a caller
    /// that may run Verifiers but not query Bifrost cannot read the retained
    /// outcome, and another tenant cannot read the execution.
    ///
    /// # Errors
    ///
    /// Returns server startup, bootstrap, registration, MCP transport, or
    /// tool-call failures, or an error when the execution never settles.
    ///
    /// # Panics
    ///
    /// Panics when data is lost, invented, leaked, or readable without
    /// authority.
    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "requires the Postgres-backed Bifrost journey lane"]
    async fn custom_verdict_data_round_trips() -> Result<(), McpJourneyError> {
        let server = Box::pin(
            WyrdTestServer::builder()
                .with_verification_runtime_for_test()
                .start_bound(),
        )
        .await?;
        let admin = server
            .bootstrap_service("mcp-scored-admin", &["admin"])
            .await?;
        let cards = Cards::connect_with(&client(&server, api_key(&admin)?)?).await?;
        let fixture = register_fixture(&cards).await?;
        let root = std::env::temp_dir().join(format!("wyrd-mcp-scored-{}", uuid::Uuid::now_v7()));
        std::fs::create_dir_all(&root)?;
        std::fs::write(root.join("scored.yaml"), SCORED)?;
        let scored = Box::pin(cards.declare_from_path(&root.join("scored.yaml"))).await?;
        std::fs::remove_dir_all(&root)?;
        let request = |answer: &str| -> Result<JsonValue, McpJourneyError> {
            Ok(serde_json::json!({
                "verifier_card_uid": scored.root.uid.as_ref().ok_or("a registered Card has a UID")?.to_string(),
                "card_uid": fixture.service_uid,
                "inputs": { "answer": { "kind": "json", "value": answer } },
            }))
        };
        let agent = server
            .credential_registered_service(&fixture.service_ref, &["editor"])
            .await?;
        let agent_client = connect_rooted(&server, &agent).await?;
        let tools = agent_client.list_all_tools().await?;
        for tool in [EXECUTE, ENQUEUE, GET_EXECUTION, QUERY] {
            assert!(
                tools.iter().any(|offered| offered.name == tool),
                "{tool} is offered"
            );
        }

        let judged = structured(
            agent_client
                .call_tool(call(EXECUTE, request("fine")?)?)
                .await?,
        )?;
        let requirement = &judged["outcome"]["requirements"][0];
        assert_eq!(requirement["outcome"], "failed", "{judged}");
        assert_eq!(
            requirement["data"],
            serde_json::json!({ "score": 0.5 }),
            "{judged}"
        );
        let refused = structured(
            agent_client
                .call_tool(call(EXECUTE, request("BAD")?)?)
                .await?,
        )?;
        let requirement = &refused["outcome"]["requirements"][0];
        assert_eq!(requirement["outcome"], "inconclusive", "{refused}");
        assert!(requirement.get("data").is_none(), "{refused}");
        assert!(
            !requirement["reason"].to_string().contains("high"),
            "the reason never echoes the data: {refused}"
        );

        let accepted = structured(
            agent_client
                .call_tool(call(ENQUEUE, request("fine")?)?)
                .await?,
        )?;
        let execution_id = text(&accepted, "/execution_id")?.to_owned();
        let state = settle(&agent_client, &execution_id).await?;
        assert_eq!(
            state["result"]["outcome"]["requirements"][0]["data"],
            serde_json::json!({ "score": 0.5 }),
            "{state}"
        );
        server.flush_bifrost().await?;
        let retained = rows(
            &agent_client,
            format!(
                "SELECT to_json(output -> 'requirements' -> 0 -> 'data') \
                 FROM bifrost.verification.execution_outputs WHERE execution_id = '{execution_id}'"
            ),
        )
        .await?;
        let [row] = retained.as_array().ok_or("rows are an array")?.as_slice() else {
            return Err(format!("one retained result: {retained}").into());
        };
        let data: JsonValue = serde_json::from_str(row[0].as_str().ok_or("JSON text")?)?;
        assert_eq!(data, serde_json::json!({ "score": 0.5 }));
        agent_client.cancel().await?;

        server
            .seed_role(
                "mcp_cards_only",
                &[Permission::card_read(), Permission::verifier_run_any()],
            )
            .await?;
        let scoped = server
            .bootstrap_service("mcp-scored-cards-only", &["mcp_cards_only"])
            .await?;
        let scoped_client = connect(&server, &scoped).await?;
        let denied = refusal(
            scoped_client
                .call_tool(call(
                    QUERY,
                    serde_json::json!({
                        "sql": format!("SELECT output FROM bifrost.verification.execution_outputs WHERE execution_id = '{execution_id}'"),
                        "max_rows": 1
                    }),
                )?)
                .await,
        )?;
        assert!(
            !denied.contains("0.5"),
            "the payload stays hidden: {denied}"
        );
        scoped_client.cancel().await?;

        let other_tenant = server.seed_tenant("mcp-scored-other").await?;
        let foreign = server
            .bootstrap_service_in_tenant(other_tenant, "mcp-scored-foreign", &["admin"])
            .await?;
        let foreign_client = connect(&server, &foreign).await?;
        let foreign_read = refusal(
            foreign_client
                .call_tool(call(
                    GET_EXECUTION,
                    serde_json::json!({ "execution_id": execution_id }),
                )?)
                .await,
        )?;
        assert!(
            foreign_read.contains("WYRD_VERIFICATION_404_EXECUTION_NOT_FOUND"),
            "{foreign_read}"
        );
        foreign_client.cancel().await?;
        server.shutdown().await?;
        Ok(())
    }

    /// Send `bytes` for every upload `planned` by `verification.evidence`
    /// and complete each through the shared storage route, as `http`'s
    /// caller on the server at `base_url`.
    ///
    /// # Errors
    ///
    /// Returns a plan-shape, token, or transport failure.
    ///
    /// # Panics
    ///
    /// Panics when the transfer or completion answers a non-success status.
    async fn upload_planned(
        http: &wyrd_client::WyrdClient,
        base_url: &str,
        planned: &JsonValue,
        bytes: &[u8],
    ) -> Result<(), McpJourneyError> {
        let token = format!("Bearer {}", http.access_token().await?.expose());
        let absolute = |url: &str| {
            if url.starts_with('/') {
                format!("{base_url}{url}")
            } else {
                url.to_owned()
            }
        };
        for upload in planned["uploads"].as_array().ok_or("uploads is a list")? {
            let status = reqwest::Client::new()
                .put(absolute(text(upload, "/plan/data/put_url")?))
                .header("x-wyrd-access-token", &token)
                .header("content-type", "application/octet-stream")
                .body(bytes.to_vec())
                .send()
                .await?
                .status();
            assert!(
                status.is_success(),
                "the planned upload is accepted: {status}"
            );
            let upload_id = text(upload, "/upload_id")?;
            let status = reqwest::Client::new()
                .post(absolute(&format!("/v1/cards/upload/{upload_id}/complete")))
                .header("x-wyrd-access-token", &token)
                .json(&serde_json::json!({ "protocol": "single_put", "data": {} }))
                .send()
                .await?
                .status();
            assert!(status.is_success(), "the upload completes: {status}");
        }
        Ok(())
    }

    /// A Verifier judging one uploaded JSON document: its extracted `answer`
    /// must equal `1`.
    const DOCUMENT_VERIFIER: &str = "apiVersion: wyrd/v1\nkind: Verifier\nmetadata:\n  name: mcp-file-document\n  version: 1.0.0\nspec:\n  inputs:\n    doc: {type: media, formats: [json]}\n  steps:\n    - id: read\n      action: {kind: extract, from: inputs.doc}\n      requirements:\n        - id: answer-is-one\n          statement: The document's answer is one.\n          check: {kind: assert, path: steps.read.output.value.answer, operator: equals, expected: 1}\n";

    /// An agent holding `verifier:run` on exactly one Verifier plans a File
    /// through `verification.evidence`, uploads and completes it through the
    /// shared storage routes, and judges it with that Verifier over MCP. The
    /// completed File is reused by the next plan, while another Verifier
    /// stays refused for the same File.
    ///
    /// # Errors
    ///
    /// Returns server startup, bootstrap, registration, MCP transport,
    /// upload, or tool-call failures.
    ///
    /// # Panics
    ///
    /// Panics when a plan, verdict, or refusal expectation fails.
    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "requires the Postgres-backed Bifrost journey lane"]
    async fn file_inputs_use_shared_uploads() -> Result<(), McpJourneyError> {
        use sha2::Digest as _;
        const DOCUMENT: &[u8] = br#"{"answer":1}"#;
        let server = Box::pin(WyrdTestServer::start_bound()).await?;
        let admin = server
            .bootstrap_service("mcp-file-admin", &["admin"])
            .await?;
        let cards = Cards::connect_with(&client(&server, api_key(&admin)?)?).await?;
        let fixture = register_fixture(&cards).await?;
        let root = std::env::temp_dir().join(format!("wyrd-mcp-file-{}", uuid::Uuid::now_v7()));
        std::fs::create_dir_all(&root)?;
        std::fs::write(root.join("verifier.yaml"), DOCUMENT_VERIFIER)?;
        let document = Box::pin(cards.declare_from_path(&root.join("verifier.yaml"))).await?;
        std::fs::remove_dir_all(&root)?;
        let document_uid = document.root.uid.ok_or("a registered Card has a UID")?;
        let document = document_uid.to_string();
        server
            .seed_role(
                "mcp_file_exact",
                &[
                    Permission::card_read(),
                    Permission::verifier_run(document_uid),
                ],
            )
            .await?;
        let agent = server
            .credential_registered_service(&fixture.service_ref, &["mcp_file_exact"])
            .await?;
        let agent_client = connect_rooted(&server, &agent).await?;
        let http = rooted_client(&server, &agent)?;
        let base_url = server.base_url().ok_or("a bound server has a URL")?;

        let sha256 = format!("{:x}", sha2::Sha256::digest(DOCUMENT));
        let files =
            serde_json::json!({ "files": [{ "sha256": sha256, "size_bytes": DOCUMENT.len() }] });
        let planned = structured(
            agent_client
                .call_tool(call(EVIDENCE, files.clone())?)
                .await?,
        )?;
        text(&planned, "/uploads/0/upload_id")?;
        upload_planned(&http, base_url, &planned, DOCUMENT).await?;
        let held = structured(agent_client.call_tool(call(EVIDENCE, files)?).await?)?;
        assert_eq!(
            held["uploads"],
            serde_json::json!([]),
            "the completed File is reused"
        );

        let request = |verifier: &str| {
            serde_json::json!({
                "verifier_card_uid": verifier,
                "card_uid": fixture.service_uid,
                "inputs": { "doc": { "kind": "file", "sha256": sha256,
                                     "size_bytes": DOCUMENT.len(), "format": "json" } },
            })
        };
        let judged = structured(
            agent_client
                .call_tool(call(EXECUTE, request(&document))?)
                .await?,
        )?;
        assert_eq!(judged["outcome"]["verdict"], "passed", "{judged}");
        let denied = refusal(
            agent_client
                .call_tool(call(EXECUTE, request(&fixture.assert_uid))?)
                .await,
        )?;
        assert!(
            denied.contains("WYRD_PERMISSION_403_DENIED_RBAC"),
            "another Verifier stays refused: {denied}"
        );
        agent_client.cancel().await?;
        server.shutdown().await?;
        Ok(())
    }

    /// An agent enqueues a Verifier over an uploaded File and reads the
    /// settled execution back over MCP with its original File declaration as
    /// the evidence.
    ///
    /// # Errors
    ///
    /// Returns server startup, bootstrap, registration, MCP transport,
    /// upload, or tool-call failures, or an error when the execution never
    /// settles.
    ///
    /// # Panics
    ///
    /// Panics when the status, verdict, or evidence expectation fails.
    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "requires the Postgres-backed Bifrost journey lane"]
    async fn stored_evidence_readback_uses_canonical_records() -> Result<(), McpJourneyError> {
        use sha2::Digest as _;
        /// The uploaded JSON document; the `DOCUMENT_VERIFIER` fixture passes it.
        const DOCUMENT: &[u8] = br#"{"answer":1}"#;
        let server = Box::pin(
            WyrdTestServer::builder()
                .with_verification_runtime_for_test()
                .start_bound(),
        )
        .await?;
        let admin = server
            .bootstrap_service("mcp-readback-admin", &["admin"])
            .await?;
        let cards = Cards::connect_with(&client(&server, api_key(&admin)?)?).await?;
        let fixture = register_fixture(&cards).await?;
        let root = std::env::temp_dir().join(format!("wyrd-mcp-readback-{}", uuid::Uuid::now_v7()));
        std::fs::create_dir_all(&root)?;
        std::fs::write(root.join("verifier.yaml"), DOCUMENT_VERIFIER)?;
        let document = Box::pin(cards.declare_from_path(&root.join("verifier.yaml"))).await?;
        std::fs::remove_dir_all(&root)?;
        let document = document
            .root
            .uid
            .ok_or("a registered Card has a UID")?
            .to_string();
        let agent = server
            .credential_registered_service(&fixture.service_ref, &["editor"])
            .await?;
        let agent_client = connect_rooted(&server, &agent).await?;
        let http = rooted_client(&server, &agent)?;
        let base_url = server.base_url().ok_or("a bound server has a URL")?;

        let sha256 = format!("{:x}", sha2::Sha256::digest(DOCUMENT));
        let files =
            serde_json::json!({ "files": [{ "sha256": sha256, "size_bytes": DOCUMENT.len() }] });
        let planned = structured(agent_client.call_tool(call(EVIDENCE, files)?).await?)?;
        upload_planned(&http, base_url, &planned, DOCUMENT).await?;
        let declaration = serde_json::json!({ "kind": "file", "sha256": sha256,
                                              "size_bytes": DOCUMENT.len(), "format": "json" });
        let accepted = structured(
            agent_client
                .call_tool(call(
                    ENQUEUE,
                    serde_json::json!({
                        "verifier_card_uid": document,
                        "card_uid": fixture.service_uid,
                        "inputs": { "doc": declaration },
                    }),
                )?)
                .await?,
        )?;
        let state = settle(&agent_client, text(&accepted, "/execution_id")?).await?;
        assert_eq!(state["status"], "completed", "{state}");
        assert_eq!(state["result"]["outcome"]["verdict"], "passed", "{state}");
        assert_eq!(
            state["result"]["outcome"]["evidence"],
            serde_json::json!({ "doc": declaration }),
            "{state}"
        );
        agent_client.cancel().await?;
        server.shutdown().await?;
        Ok(())
    }

    /// A bound Service judges inline inputs with a Verifier over MCP.
    ///
    /// The tool result carries the exact Verifier and subject and the
    /// `passed` or `failed` outcome of the one assertion requirement; nothing
    /// is enqueued. The Service plans an evidence upload for a new digest,
    /// and direct execution of its bound drift Verifier is refused as
    /// unsupported input. A reader and a caller holding only an unrelated `operators:invoke` are
    /// neither offered the write tools nor allowed `verification.execute`,
    /// and another tenant's administrator cannot resolve this tenant's Cards.
    ///
    /// # Errors
    ///
    /// Returns server startup, bootstrap, registration, MCP transport, or
    /// tool-call failures.
    ///
    /// # Panics
    ///
    /// Panics when any verdict, identity, upload plan, or refusal expectation
    /// fails.
    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "requires the Postgres-backed Bifrost journey lane"]
    async fn an_agent_executes_a_verifier_on_inline_inputs() -> Result<(), McpJourneyError> {
        let server = Box::pin(WyrdTestServer::start_bound()).await?;
        let admin = server
            .bootstrap_service("mcp-execute-admin", &["admin"])
            .await?;
        let cards = Cards::connect_with(&client(&server, api_key(&admin)?)?)
            .await
            .expect("handle connects");
        let fixture = register_fixture(&cards).await?;
        let agent = server
            .credential_registered_service(&fixture.service_ref, &["editor"])
            .await?;
        let agent_client = connect_rooted(&server, &agent).await?;
        for (answer, verdict) in [(PASSING_ANSWER, "passed"), (FAILING_ANSWER, "failed")] {
            let judged = structured(
                agent_client
                    .call_tool(call(EXECUTE, fixture.assert_request(answer))?)
                    .await?,
            )?;
            assert_eq!(judged["outcome"]["verdict"], verdict, "{judged}");
            assert_eq!(
                text(&judged, "/outcome/requirements/0/id")?,
                "no-refund-promise"
            );
            assert_eq!(text(&judged, "/verifier/uid")?, fixture.assert_uid);
            assert_eq!(text(&judged, "/subject/uid")?, fixture.service_uid);
            text(&judged, "/execution_id")?;
        }

        let planned = structured(
            agent_client
                .call_tool(call(
                    EVIDENCE,
                    serde_json::json!({ "files": [{ "sha256": HELLO_SHA256, "size_bytes": 5 }] }),
                )?)
                .await?,
        )?;
        assert_eq!(
            text(&planned, "/uploads/0/sha256")?,
            HELLO_SHA256,
            "a new digest is planned for upload: {planned}"
        );
        assert!(planned["uploads"][0]["plan"].is_object(), "{planned}");

        let drift = refusal(
            agent_client
                .call_tool(call(
                    EXECUTE,
                    serde_json::json!({
                        "verifier_card_uid": fixture.verifier_uid,
                        "card_uid": fixture.service_uid,
                        "inputs": {},
                    }),
                )?)
                .await,
        )?;
        assert!(
            drift.contains("WYRD_VERIFICATION_422_INPUT_UNSUPPORTED"),
            "a drift Verifier is never executed directly: {drift}"
        );
        agent_client.cancel().await?;

        server
            .seed_role(
                "mcp_operators_only",
                &[Permission::card_read(), Permission::operator_invoke()],
            )
            .await?;
        for (name, role) in [
            ("mcp-execute-reader", "viewer"),
            ("mcp-execute-operators-only", "mcp_operators_only"),
        ] {
            let caller = server.bootstrap_service(name, &[role]).await?;
            let caller_client = connect(&server, &caller).await?;
            let tools = caller_client.list_all_tools().await?;
            for tool in WRITE_TOOLS {
                assert!(
                    !tools.iter().any(|offered| offered.name == tool),
                    "{role} is not offered {tool}"
                );
            }
            let denied = refusal(
                caller_client
                    .call_tool(call(EXECUTE, fixture.assert_request(PASSING_ANSWER))?)
                    .await,
            )?;
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
        let foreign_client = connect(&server, &foreign).await?;
        let foreign_execute = refusal(
            foreign_client
                .call_tool(call(EXECUTE, fixture.assert_request(PASSING_ANSWER))?)
                .await,
        )?;
        assert!(
            foreign_execute.contains("WYRD_VERIFICATION_404_TARGET_NOT_FOUND"),
            "another tenant cannot execute this tenant's Verifier: {foreign_execute}"
        );
        foreign_client.cancel().await?;
        server.shutdown().await?;
        Ok(())
    }

    /// `verification.execute` admits the subject through the caller's signed
    /// Card limit before it judges anything: the subject's runtime account
    /// at its root judges it, the same account without a root is refused as
    /// needing workload context, and an unrelated Service rooted at its own
    /// Card is refused as out of scope, while that Service without a root,
    /// which may attribute to any registered Card, still judges it.
    ///
    /// # Errors
    ///
    /// Returns server startup, bootstrap, registration, MCP transport, or
    /// tool-call failures.
    ///
    /// # Panics
    ///
    /// Panics when the administrator's Cards handle cannot connect or a
    /// verdict or refusal code differs from the expectation.
    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "requires the Postgres-backed Bifrost journey lane"]
    async fn direct_execution_enforces_card_access() -> Result<(), McpJourneyError> {
        let server = Box::pin(WyrdTestServer::start_bound()).await?;
        let admin = server
            .bootstrap_service("mcp-access-admin", &["admin"])
            .await?;
        let cards = Cards::connect_with(&client(&server, api_key(&admin)?)?)
            .await
            .expect("handle connects");
        let fixture = register_fixture(&cards).await?;
        let owner = server
            .credential_registered_service(&fixture.service_ref, &["editor"])
            .await?;
        let foreign = server
            .bootstrap_service("mcp-access-foreign", &["editor"])
            .await?;

        for (agent, code) in [
            (
                connect(&server, &owner).await?,
                "WYRD_AUTH_403_WORKLOAD_CONTEXT_REQUIRED",
            ),
            (
                connect_rooted(&server, &foreign).await?,
                "WYRD_PERMISSION_403_DENIED_RBAC",
            ),
        ] {
            let refused = refusal(
                agent
                    .call_tool(call(EXECUTE, fixture.assert_request(PASSING_ANSWER))?)
                    .await,
            )?;
            assert!(refused.contains(code), "{code}: {refused}");
            agent.cancel().await?;
        }
        for agent in [
            connect_rooted(&server, &owner).await?,
            connect(&server, &foreign).await?,
        ] {
            let judged = structured(
                agent
                    .call_tool(call(EXECUTE, fixture.assert_request(PASSING_ANSWER))?)
                    .await?,
            )?;
            assert_eq!(judged["outcome"]["verdict"], "passed", "{judged}");
            agent.cancel().await?;
        }
        server.shutdown().await?;
        Ok(())
    }

    /// Trace id of the caller span the continuity journey sends.
    const CALLER_TRACE: &str = "0af7651916cd43dd8448eb211c80319c";

    /// Span id of the caller span the continuity journey sends.
    const CALLER_SPAN: &str = "b7ad6b7169203331";

    /// Trace id of the caller span the delegated continuity call sends.
    const DELEGATED_TRACE: &str = "4bf92f3577b34da6a3ce929d0e0e4736";

    /// Spans the judged execution opens beneath its request span.
    const DESCENDANTS: [&str; 4] = [
        "verification.execute",
        "agent.gateway.call",
        "gateway.call",
        "gateway.execute",
    ];

    /// The Verdict the local judge returns for every answer: passed, citing
    /// the whole answer.
    const AGREES: &str = r#"{"outcome":"passed","reason":"it agrees","citations":[{"slot":"answer","locator":{"json_pointer":""}}]}"#;

    /// Start a bound server whose gateway answers `gpt-4o` judge calls from a
    /// local provider that passes every answer, register the `answer-judged`
    /// Agent-check Verifier and a Service to judge, and return that Service's
    /// principal, which may run Verifiers and invoke the judge model, with
    /// the execution request it sends.
    ///
    /// # Errors
    ///
    /// Returns provider, server, bootstrap, client configuration, API-key
    /// exchange or connection, registration, or deployment failures.
    async fn judging_server()
    -> Result<(WyrdTestServer, wiremock::MockServer, Bootstrap, JsonValue), McpJourneyError> {
        let judge = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::method("POST"))
            .and(wiremock::matchers::path("/v1/chat/completions"))
            .respond_with(
                wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "id": "chatcmpl-judge", "object": "chat.completion", "created": 1,
                    "model": "gpt-4o",
                    "choices": [{ "index": 0, "finish_reason": "stop",
                        "message": { "role": "assistant", "content": AGREES } }],
                    "usage": { "prompt_tokens": 5, "completion_tokens": 3, "total_tokens": 8 }
                })),
            )
            .mount(&judge)
            .await;
        let server = Box::pin(
            WyrdTestServer::builder()
                .with_gateway_provider_root_for_test(judge.uri().parse()?)
                .start_bound(),
        )
        .await?;
        let admin = server
            .bootstrap_service("mcp-traced-admin", &["admin"])
            .await?;
        let cards = Cards::connect_with(&client(&server, api_key(&admin)?)?).await?;
        let fixture = register_fixture(&cards).await?;
        let judged = Box::pin(cards.declare_from_path(Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../fixtures/cards/verify_in_real_time/answer-judged.yaml"
        ))))
        .await?;
        server
            .deploy_gateway_model_for_test("gpt-4o", "mcp_judge_invoker")
            .await?;
        let agent = server
            .credential_registered_service(&fixture.service_ref, &["editor", "mcp_judge_invoker"])
            .await?;
        let request = serde_json::json!({
            "verifier_card_uid": judged.root.uid.ok_or("a registered Card has a UID")?.to_string(),
            "card_uid": fixture.service_uid,
            "inputs": { "answer": { "kind": "json", "value": "yes" } },
        });
        Ok((server, judge, agent, request))
    }

    /// Execute `request` once over a fresh MCP connection sending `headers`,
    /// asserting the judge passed it.
    ///
    /// # Errors
    ///
    /// Returns MCP transport or tool-call failures.
    ///
    /// # Panics
    ///
    /// Panics when the verdict is not `passed`.
    async fn execute_traced(
        server: &WyrdTestServer,
        caller: &wyrd_client::WyrdClient,
        request: &JsonValue,
        headers: [(&'static str, &str); 2],
    ) -> Result<(), McpJourneyError> {
        let headers = headers
            .into_iter()
            .map(|(name, value)| Ok((http::HeaderName::from_static(name), value.parse()?)))
            .collect::<Result<Vec<_>, McpJourneyError>>()?;
        let base_url = server
            .base_url()
            .ok_or("journey requires a bound test server")?;
        let agent =
            ().serve_with_lifecycle(client_transport(base_url, caller, headers), discover())
                .await?;
        let judged = structured(agent.call_tool(call(EXECUTE, request.clone())?).await?)?;
        assert_eq!(judged["outcome"]["verdict"], "passed", "{judged}");
        agent.cancel().await?;
        Ok(())
    }

    /// A tool call carrying the caller's sampled `traceparent` continues the
    /// caller's trace: its SERVER request span is a child of the caller's
    /// span, and the execution, judge, and gateway spans it opens descend from
    /// that request span. A call with a malformed `traceparent` and
    /// `tracestate` still succeeds, on a new trace.
    ///
    /// Each retained span names the verified pair (AC-010/015): the Service
    /// A's request is A/A and B's request for A on its own trace is A/B,
    /// while the judge's gateway spans beneath either keep A as subject
    /// under one system actor that is neither A nor B.
    ///
    /// # Errors
    ///
    /// Returns telemetry, server, fixture, exchange, MCP transport, tool-call,
    /// or retained-span query failures.
    ///
    /// # Panics
    ///
    /// Panics when a verdict, the request span's parentage, a descendant
    /// span's ancestry, or a retained pair differs from the expectation.
    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "requires the Postgres-backed Bifrost journey lane"]
    async fn verification_tool_continues_trace_context() -> Result<(), McpJourneyError> {
        let (_telemetry, traces) =
            wyrd_server::install_capture_runtime(wyrd_telemetry::TelemetryConfig::default())?;
        let (server, _judge, agent, request) = judging_server().await?;
        let caller = rooted_client(&server, &agent)?;
        let actor = server
            .bootstrap_service("mcp-traced-actor", &["editor", "mcp_judge_invoker"])
            .await?;
        // The subject is A's family runtime account at A's root; the actor is
        // an unrestricted account that selects A's root too, since delegation
        // needs a rooted actor and narrows the Card scope to both graphs.
        let subject_jwt = server.exchange_rooted(&agent).await?;
        let actor_jwt = server
            .exchange_api_key_for_workload(
                actor.api_key().ok_or("the actor has an API key")?,
                agent
                    .card_ref()
                    .and_then(|card| card.uid.as_ref())
                    .ok_or("A has a registered root")?,
            )
            .await?;
        let delegated = server
            .delegate(&subject_jwt, &actor_jwt, TokenAudience::Wyrd)
            .await?;
        let checkpoint = traces.checkpoint();
        let parent = format!("00-{CALLER_TRACE}-{CALLER_SPAN}-01");
        execute_traced(
            &server,
            &caller,
            &request,
            [("traceparent", &parent), ("tracestate", "wyrd=caller")],
        )
        .await?;
        execute_traced(
            &server,
            &caller,
            &request,
            [("traceparent", "00-not-a-trace-01"), ("tracestate", "@@")],
        )
        .await?;
        execute_traced(
            &server,
            &client(&server, ResolvedCredential::BearerToken(delegated.into()))?,
            &request,
            [
                (
                    "traceparent",
                    &format!("00-{DELEGATED_TRACE}-{CALLER_SPAN}-01"),
                ),
                ("tracestate", "wyrd=caller"),
            ],
        )
        .await?;
        server.flush_bifrost().await?;
        let stamped = server
            .retained_span_records(
                server.data_tenant_id(),
                "encode(span_id, 'hex'), sub_principal_id, act_principal_id",
                &format!("trace_id IN (X'{CALLER_TRACE}', X'{DELEGATED_TRACE}')"),
            )
            .await?;
        server.shutdown().await?;

        let spans = traces.finished_since(checkpoint);
        let continued: Vec<_> = spans
            .iter()
            .filter(|span| {
                span.name == "request"
                    && span.trace_id == CALLER_TRACE
                    && span.parent_span_id == CALLER_SPAN
            })
            .collect();
        assert!(
            !continued.is_empty(),
            "the tool call continues the caller: {spans:#?}"
        );
        assert!(
            continued.iter().all(|span| span.kind == "Server"),
            "{continued:#?}"
        );
        let parents: std::collections::HashMap<&str, &str> = spans
            .iter()
            .filter(|span| span.trace_id == CALLER_TRACE)
            .map(|span| (span.span_id.as_str(), span.parent_span_id.as_str()))
            .collect();
        for name in DESCENDANTS {
            let span = spans
                .iter()
                .find(|span| span.name == name && span.trace_id == CALLER_TRACE)
                .ok_or_else(|| format!("{name} joins the caller's trace: {spans:#?}"))?;
            let mut ancestor = Some(span.parent_span_id.as_str());
            while let Some(id) = ancestor
                && !continued.iter().any(|request| request.span_id == id)
            {
                ancestor = parents.get(id).copied();
            }
            assert!(
                ancestor.is_some(),
                "{name} descends from a request span: {span:#?}"
            );
        }
        assert!(
            spans
                .iter()
                .filter(|span| span.name == "verification.execute")
                .any(|span| span.trace_id != CALLER_TRACE),
            "the malformed call is judged on a new trace: {spans:#?}"
        );

        let pairs = |trace: &str, judged: bool| {
            spans
                .iter()
                .filter(|span| {
                    span.trace_id == trace
                        && if judged {
                            span.name.contains("gateway.")
                        } else {
                            span.name == "request"
                        }
                })
                .filter_map(|span| {
                    stamped
                        .iter()
                        .find(|row| row[0].as_deref() == Some(span.span_id.as_str()))
                        .map(|row| (row[1].clone(), row[2].clone()))
                })
                .collect::<std::collections::BTreeSet<_>>()
        };
        let (a, b) = (Some(agent.id().to_string()), Some(actor.id().to_string()));
        assert_eq!(
            pairs(CALLER_TRACE, false),
            [(a.clone(), a.clone())].into(),
            "A's request is A/A"
        );
        assert_eq!(
            pairs(DELEGATED_TRACE, false),
            [(a.clone(), b.clone())].into(),
            "B's request for A is A/B"
        );
        let judge = pairs(CALLER_TRACE, true);
        assert_eq!(
            judge,
            pairs(DELEGATED_TRACE, true),
            "both judges' gateway calls share one pair"
        );
        assert!(
            judge.len() == 1
                && judge
                    .iter()
                    .all(|(sub, act)| *sub == a && act.is_some() && *act != a && *act != b),
            "the judge's gateway calls keep A as subject under the system actor: {judge:?}"
        );
        Ok(())
    }

    /// The one `group_end` binding of the Service `service_uid`, discovered
    /// over MCP: its Card names its bindings and each binding's status says
    /// whether it pins `group_end` settings.
    ///
    /// # Errors
    ///
    /// Returns a tool-call failure or an error when the Card names no
    /// bindings.
    ///
    /// # Panics
    ///
    /// Panics when the Service does not bind exactly one `group_end`
    /// Verifier.
    async fn group_binding(
        client: &RunningService<RoleClient, ()>,
        service_uid: &str,
    ) -> Result<String, McpJourneyError> {
        let card = structured(
            client
                .call_tool(call(
                    CARDS_GET,
                    serde_json::json!({ "kind": "Service", "card_uid": service_uid }),
                )?)
                .await?,
        )?;
        let mut grouped = Vec::new();
        for id in card["card"]["status"]["verification"]["binding_ids"]
            .as_array()
            .ok_or("the Service names its bindings")?
        {
            let binding = structured(
                client
                    .call_tool(call(GET_BINDING, serde_json::json!({ "binding_id": id }))?)
                    .await?,
            )?;
            if !binding["group_end"].is_null() {
                grouped.push(text(&binding, "/binding_id")?.to_owned());
            }
        }
        let [binding_id] = <[String; 1]>::try_from(grouped).unwrap_or_else(|grouped| {
            panic!("the Service binds exactly one group_end Verifier: {grouped:?}")
        });
        Ok(binding_id)
    }

    /// The `outcomes` source's execution IDs in a group execution's pinned
    /// snapshot, in list order; empty when the execution pins none.
    fn ids(execution: &JsonValue) -> Vec<&str> {
        execution["group"]["sources"]["outcomes"]["execution_ids"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(JsonValue::as_str)
            .collect()
    }

    /// A group execution, read over MCP once its group comes due: poll
    /// `verification.get_binding` until it names a completed execution other
    /// than `after` whose result is visible, publishing captured rows each
    /// round so the group poller discovers them and a completed execution's
    /// staged result becomes readable.
    ///
    /// # Errors
    ///
    /// Returns a tool-call or flush failure, or an error when no execution
    /// completes within [`SETTLE`].
    async fn group_execution(
        server: &WyrdTestServer,
        client: &RunningService<RoleClient, ()>,
        binding_id: &str,
        after: Option<&str>,
    ) -> Result<JsonValue, McpJourneyError> {
        let deadline = tokio::time::Instant::now() + SETTLE;
        loop {
            server.flush_bifrost().await?;
            let binding = structured(
                client
                    .call_tool(call(
                        GET_BINDING,
                        serde_json::json!({ "binding_id": binding_id }),
                    )?)
                    .await?,
            )?;
            if let Some(id) = binding["last_execution_id"].as_str()
                && Some(id) != after
            {
                let state = settle(client, id).await?;
                if state["status"] == "completed" && !state["result"].is_null() {
                    return Ok(state);
                }
            }
            if tokio::time::Instant::now() >= deadline {
                return Err(format!("binding {binding_id} completed no group execution").into());
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
    }

    /// An agent discovers its `group_end` binding, judges a conversation's
    /// turns through `verification.execute` and a keyed
    /// `verification.enqueue` that both carry `group_id`, and inspects the
    /// group execution over MCP.
    ///
    /// The Service reads its Card and finds the binding whose status pins
    /// `group_end` settings. Each grouped tool result and execution read
    /// echoes the same `group_id`, and an ungrouped `execute` leaves its
    /// result without one. Once its `after` period expires the binding names one
    /// execution whose snapshot holds exactly the two grouped turns in call
    /// order, and `bifrost.query` reads the same `group_id` on its persisted
    /// result row. A reader is refused a grouped enqueue, and another
    /// tenant's administrator cannot read the execution.
    ///
    /// # Errors
    ///
    /// Returns server startup, bootstrap, registration, MCP transport, or
    /// tool-call failures, or an error when a turn or the group execution never
    /// settles.
    ///
    /// # Panics
    ///
    /// Panics when the administrator's Cards handle cannot connect, or when
    /// any binding, group, snapshot, row, or refusal expectation fails.
    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "requires the Postgres-backed Bifrost journey lane"]
    async fn grouped_verification_discover_act_observe() -> Result<(), McpJourneyError> {
        let server = Box::pin(
            WyrdTestServer::builder()
                .with_verification_runtime_for_test()
                .start_bound(),
        )
        .await?;
        let admin = server
            .bootstrap_service("mcp-group-admin", &["admin"])
            .await?;
        let cards = Cards::connect_with(&client(&server, api_key(&admin)?)?)
            .await
            .expect("handle connects");
        let receipt = Box::pin(cards.declare_from_path(Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../fixtures/cards/verify_in_real_time/support.yaml"
        ))))
        .await?;
        let uid = |name: &str| -> Result<String, McpJourneyError> {
            Ok(receipt
                .outcomes
                .iter()
                .find(|outcome| outcome.card_ref.name.as_str() == name)
                .and_then(|outcome| outcome.card_ref.uid.as_ref())
                .ok_or("the fixture registers the named Card")?
                .to_string())
        };
        let (service_uid, turn_check, agent_uid) =
            (uid("support")?, uid("turn-check")?, uid("assistant-agent")?);
        let agent = server
            .credential_registered_service(&receipt.root, &["editor"])
            .await?;
        let agent_client = connect_rooted(&server, &agent).await?;

        // Discover: the Service's Card names the binding whose status pins group_end.
        let binding_id = &group_binding(&agent_client, &service_uid).await?;

        // Act: one grouped execute, one keyed grouped enqueue, one ungrouped execute.
        let turn = |answer: &str, group: Option<&str>| {
            let mut body = serde_json::json!({
                "verifier_card_uid": turn_check,
                "card_uid": agent_uid,
                "inputs": { "turn": { "kind": "json", "value": { "answer": answer } } },
            });
            if let Some(group) = group {
                body["group_id"] = group.into();
            }
            body
        };
        let executed = structured(
            agent_client
                .call_tool(call(EXECUTE, turn("first", Some("conv-mcp")))?)
                .await?,
        )?;
        let first = text(&executed, "/execution_id")?.to_owned();
        let mut enqueue = turn("second", Some("conv-mcp"));
        enqueue["idempotency_key"] = "mcp-group-0002".into();
        let second = text(
            &structured(
                agent_client
                    .call_tool(call(ENQUEUE, enqueue.clone())?)
                    .await?,
            )?,
            "/execution_id",
        )?
        .to_owned();
        let state = settle(&agent_client, &second).await?;
        assert_eq!(state["status"], "completed", "{state}");
        assert_eq!(state["group_id"], "conv-mcp", "{state}");
        let ungrouped = structured(
            agent_client
                .call_tool(call(EXECUTE, turn("solo", None))?)
                .await?,
        )?;
        let solo = text(&ungrouped, "/execution_id")?.to_owned();

        // Observe: one execution over exactly the two grouped turns, in call order.
        let execution = group_execution(&server, &agent_client, binding_id, None).await?;
        assert_eq!(execution["group_id"], "conv-mcp", "{execution}");
        assert_eq!(
            execution["owner_card_uid"],
            service_uid.as_str(),
            "{execution}"
        );
        assert_eq!(
            execution["group"]["sources"]["turns"]["execution_ids"],
            serde_json::json!([first, second]),
            "{execution}"
        );
        assert_eq!(
            execution["result"]["outcome"]["evidence"]["turns"],
            serde_json::json!({ "kind": "json", "value": [{ "answer": "first" }, { "answer": "second" }] }),
            "{execution}"
        );
        assert_eq!(
            execution["result"]["outcome"]["verdict"], "passed",
            "{execution}"
        );
        let group_execution_id = text(&execution, "/execution_id")?.to_owned();
        server.flush_bifrost().await?;
        let persisted = rows(
            &agent_client,
            format!(
                "SELECT execution_id, group_id FROM bifrost.verification.results \
                 WHERE execution_id IN ('{first}', '{solo}', '{group_execution_id}') ORDER BY ended_at"
            ),
        )
        .await?;
        assert_eq!(
            persisted,
            serde_json::json!([
                [first, "conv-mcp"],
                [solo, JsonValue::Null],
                [group_execution_id, "conv-mcp"]
            ]),
            "the persisted group_id column matches each call"
        );
        agent_client.cancel().await?;

        let reader = server
            .bootstrap_service("mcp-group-reader", &["viewer"])
            .await?;
        let reader_client = connect(&server, &reader).await?;
        let denied = refusal(reader_client.call_tool(call(ENQUEUE, enqueue)?).await)?;
        assert!(
            denied.contains("WYRD_PERMISSION_403_DENIED_RBAC"),
            "an unscoped grouped enqueue is refused: {denied}"
        );
        reader_client.cancel().await?;
        let other_tenant = server.seed_tenant("mcp-group-other").await?;
        let foreign = server
            .bootstrap_service_in_tenant(other_tenant, "mcp-group-foreign", &["admin"])
            .await?;
        let foreign_client = connect(&server, &foreign).await?;
        let foreign_read = refusal(
            foreign_client
                .call_tool(call(
                    GET_EXECUTION,
                    serde_json::json!({ "execution_id": group_execution_id }),
                )?)
                .await,
        )?;
        assert!(
            foreign_read.contains("WYRD_VERIFICATION_404_EXECUTION_NOT_FOUND"),
            "another tenant cannot read the group execution: {foreign_read}"
        );
        foreign_client.cancel().await?;
        server.shutdown().await?;
        Ok(())
    }

    /// The frame judge's Verdict for a frame showing a badge: passed, citing
    /// the frame as its one page.
    const BADGED: &str = r#"{"outcome":"passed","reason":"a badge is visible","citations":[{"slot":"frame","locator":{"page":1}}]}"#;

    /// The frame judge's Verdict for the [`UNBADGED`] frame.
    const NO_BADGE: &str = r#"{"outcome":"failed","reason":"no badge is visible","citations":[{"slot":"frame","locator":{"page":1}}]}"#;

    /// Text every `frame-verifier` judge request carries.
    const FRAME_CLAIM: &str = "Cite the frame.";

    /// The label of the one frame that shows no badge.
    const UNBADGED: &str = "no badge!";

    /// [`UNBADGED`] in base64, as the judge's image data URL carries it.
    const UNBADGED_BASE64: &str = "bm8gYmFkZ2Uh";

    /// One agent's MCP session for the frames Service and the HTTP access
    /// it uploads frame bytes with.
    struct FrameAgent {
        /// The agent's MCP connection.
        client: RunningService<RoleClient, ()>,
        /// The agent's HTTP client, minting the bearer for planned uploads.
        http: wyrd_client::WyrdClient,
        /// The server's base URL.
        base_url: String,
        /// The frame Verifier the agent submits to.
        frame_verifier: String,
        /// The Card the frames are judged for.
        subject: String,
    }

    impl FrameAgent {
        /// Upload the PNG frame carrying the nine-byte `label` after its
        /// signature and a newline, planning through `verification.evidence`
        /// and uploading it with [`upload_planned`], and enqueue it to the
        /// frame Verifier under `group`, returning its execution.
        ///
        /// # Errors
        ///
        /// Returns a tool-call, plan-shape, or upload failure.
        ///
        /// # Panics
        ///
        /// Panics when the upload answers a non-success status.
        async fn submit(&self, group: &str, label: &str) -> Result<String, McpJourneyError> {
            use sha2::Digest as _;
            let bytes = [b"\x89PNG\r\n\x1a\n\n".as_slice(), label.as_bytes()].concat();
            let sha256 = format!("{:x}", sha2::Sha256::digest(&bytes));
            let planned = structured(
                self.client
                    .call_tool(call(
                        EVIDENCE,
                        serde_json::json!({ "files": [{ "sha256": sha256, "size_bytes": bytes.len() }] }),
                    )?)
                    .await?,
            )?;
            upload_planned(&self.http, &self.base_url, &planned, &bytes).await?;
            let queued = structured(
                self.client
                    .call_tool(call(
                        ENQUEUE,
                        serde_json::json!({
                            "verifier_card_uid": self.frame_verifier,
                            "card_uid": self.subject,
                            "group_id": group,
                            "idempotency_key": format!("frame-{group}-{sha256}"),
                            "inputs": { "frame": {
                                "kind": "file", "sha256": sha256,
                                "size_bytes": bytes.len(), "format": "png",
                            } },
                        }),
                    )?)
                    .await?,
            )?;
            Ok(text(&queued, "/execution_id")?.to_owned())
        }
    }

    /// An HTTP/MCP agent submits an event's sampled frames as
    /// uploaded files under the event id as `group_id`, reports the event
    /// fact, and reads the event's group execution over MCP.
    ///
    /// The event fact alone yields an execution over `[]` frame outcomes,
    /// inconclusive `empty_selection`. Two frames, one unbadged, fail the
    /// event with the failed frame kept in place, while a frame another
    /// Service submits under the same `group_id` never joins it and a
    /// reader's grouped enqueue is refused. Twenty-one badged frames are
    /// judged like any other group, passing with every visible outcome.
    ///
    /// # Errors
    ///
    /// Returns provider, server, bootstrap, registration, MCP transport,
    /// upload, or tool-call failures, or an error when an execution never
    /// settles.
    ///
    /// # Panics
    ///
    /// Panics when an upload is refused, or when a group execution's selected
    /// frame executions, their outcomes, its verdict, or a refusal differ.
    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "requires the Postgres-backed Bifrost journey lane"]
    async fn grouped_frames_use_visible_outcomes() -> Result<(), McpJourneyError> {
        let judge = wiremock::MockServer::start().await;
        for (marker, verdict, priority) in
            [(UNBADGED_BASE64, NO_BADGE, 1), (FRAME_CLAIM, BADGED, 2)]
        {
            wiremock::Mock::given(wiremock::matchers::method("POST"))
                .and(wiremock::matchers::path("/v1/chat/completions"))
                .and(wiremock::matchers::body_string_contains(marker))
                .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(
                    serde_json::json!({
                        "id": "chatcmpl-frame", "object": "chat.completion", "created": 1,
                        "model": "gpt-4o",
                        "choices": [{ "index": 0, "finish_reason": "stop",
                            "message": { "role": "assistant", "content": verdict } }],
                        "usage": { "prompt_tokens": 5, "completion_tokens": 3, "total_tokens": 8 }
                    }),
                ))
                .with_priority(priority)
                .mount(&judge)
                .await;
        }
        let server = Box::pin(
            WyrdTestServer::builder()
                .with_verification_runtime_for_test()
                .with_gateway_provider_root_for_test(judge.uri().parse()?)
                .start_bound(),
        )
        .await?;
        server
            .deploy_gateway_model_for_test("gpt-4o", "mcp_frame_invoker")
            .await?;
        let admin = server
            .bootstrap_service("mcp-frames-admin", &["admin"])
            .await?;
        let cards = Cards::connect_with(&client(&server, api_key(&admin)?)?).await?;
        let fixture = |name: &str| {
            format!(
                "{}/../../../fixtures/cards/verify_in_real_time/{name}.yaml",
                env!("CARGO_MANIFEST_DIR")
            )
        };
        let mut agents = Vec::new();
        for name in ["frames", "frames-other"] {
            let receipt = Box::pin(cards.declare_from_path(Path::new(&fixture(name)))).await?;
            let uid = |name: &str| -> Result<String, McpJourneyError> {
                Ok(receipt
                    .outcomes
                    .iter()
                    .find(|outcome| outcome.card_ref.name.as_str() == name)
                    .and_then(|outcome| outcome.card_ref.uid.as_ref())
                    .ok_or("the fixture registers the named Card")?
                    .to_string())
            };
            let principal = server
                .credential_registered_service(&receipt.root, &["editor", "mcp_frame_invoker"])
                .await?;
            agents.push((
                FrameAgent {
                    client: connect_rooted(&server, &principal).await?,
                    http: rooted_client(&server, &principal)?,
                    base_url: server
                        .base_url()
                        .ok_or("a bound server has a URL")?
                        .to_owned(),
                    frame_verifier: uid("frame-verifier")?,
                    subject: uid("assistant-agent")?,
                },
                uid(name)?,
                uid("event-fact").ok(),
            ));
        }
        let [(agent, service_uid, event_fact), (other, _, _)] =
            <[_; 2]>::try_from(agents).map_err(|_| "two agents connect")?;
        let event_fact = event_fact.ok_or("the frames Service binds event-fact")?;
        let binding_id = group_binding(&agent.client, &service_uid).await?;
        let fact = |group: &str| -> Result<CallToolRequestParams, McpJourneyError> {
            call(
                EXECUTE,
                serde_json::json!({
                    "verifier_card_uid": event_fact,
                    "card_uid": agent.subject,
                    "group_id": group,
                    "inputs": { "event": { "kind": "json",
                        "value": { "event_id": group, "detected": true } } },
                }),
            )
        };
        let outcomes = "/result/outcome/evidence/outcomes";
        let reason = "/result/outcome/requirements/0/reason";

        structured(agent.client.call_tool(fact("door-44")?).await?)?;
        let empty = group_execution(&server, &agent.client, &binding_id, None).await?;
        assert_eq!(empty["group_id"], "door-44", "{empty}");
        assert_eq!(
            empty.pointer(outcomes),
            Some(&serde_json::json!({ "kind": "json", "value": [] }))
        );
        assert_eq!(text(&empty, reason)?, "empty_selection", "{empty}");
        assert_eq!(
            empty["result"]["outcome"]["verdict"], "inconclusive",
            "{empty}"
        );

        let mut frames = vec![agent.submit("door-43", "door43-01").await?];
        frames.push(agent.submit("door-43", UNBADGED).await?);
        let foreign = other.submit("door-43", "other-001").await?;
        let reader = server
            .bootstrap_service("mcp-frames-reader", &["viewer"])
            .await?;
        let reader_client = connect(&server, &reader).await?;
        let denied = refusal(reader_client.call_tool(fact("door-43")?).await)?;
        assert!(
            denied.contains("WYRD_PERMISSION_403_DENIED_RBAC"),
            "a reader adds no frame: {denied}"
        );
        reader_client.cancel().await?;
        for execution in &frames {
            assert_eq!(
                settle(&agent.client, execution).await?["status"],
                "completed"
            );
        }
        assert_eq!(
            settle(&other.client, &foreign).await?["status"],
            "completed"
        );
        structured(agent.client.call_tool(fact("door-43")?).await?)?;
        let failing = group_execution(
            &server,
            &agent.client,
            &binding_id,
            Some(text(&empty, "/execution_id")?),
        )
        .await?;
        assert_eq!(failing["group_id"], "door-43", "{failing}");
        // Frames ingested in one batch share its ingest time, so their list
        // order is by execution ID rather than by submission.
        let listed = ids(&failing);
        assert_eq!(listed.len(), frames.len(), "{failing}");
        assert_eq!(
            listed.iter().copied().collect::<BTreeSet<_>>(),
            frames.iter().map(String::as_str).collect::<BTreeSet<_>>(),
            "only this Service's frames: {failing}"
        );
        let expected: Vec<_> = listed
            .iter()
            .map(|id| if *id == frames[1] { "failed" } else { "passed" })
            .collect();
        assert_eq!(
            failing.pointer(outcomes),
            Some(&serde_json::json!({ "kind": "json", "value": expected }))
        );
        assert_eq!(
            failing["result"]["outcome"]["verdict"], "failed",
            "{failing}"
        );

        let mut many = Vec::new();
        for index in 1..=21 {
            many.push(
                agent
                    .submit("door-45", &format!("door45-{index:02}"))
                    .await?,
            );
        }
        for execution in &many {
            settle(&agent.client, execution).await?;
        }
        structured(agent.client.call_tool(fact("door-45")?).await?)?;
        let passing = group_execution(
            &server,
            &agent.client,
            &binding_id,
            Some(text(&failing, "/execution_id")?),
        )
        .await?;
        assert_eq!(passing["group_id"], "door-45", "{passing}");
        let listed = ids(&passing);
        assert_eq!(listed.len(), many.len(), "{passing}");
        assert_eq!(
            listed.into_iter().collect::<BTreeSet<_>>(),
            many.iter().map(String::as_str).collect::<BTreeSet<_>>(),
            "{passing}"
        );
        assert_eq!(
            passing.pointer(outcomes),
            Some(&serde_json::json!({ "kind": "json", "value": vec!["passed"; 21] }))
        );
        assert_eq!(
            passing["result"]["outcome"]["verdict"], "passed",
            "{passing}"
        );
        agent.client.cancel().await?;
        other.client.cancel().await?;
        server.shutdown().await?;
        Ok(())
    }

    /// A bound Service is refused execute and enqueue over MCP with the
    /// retryable staging-pressure problem when the Scribe outbox has no
    /// reservable room, and nothing of either call is staged.
    ///
    /// A one-item outbox reserves nothing, so every reserved verification
    /// write is refused deterministically; recovery after pressure clears is
    /// proven by the SDK journeys.
    ///
    /// # Errors
    ///
    /// Returns server startup, bootstrap, registration, MCP transport, flush,
    /// or tool-call failures.
    ///
    /// # Panics
    ///
    /// Panics when a write tool is not offered, a refusal is not the stable
    /// staging-pressure code with its retry hint, or an input row was staged.
    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "requires the Postgres-backed Bifrost journey lane"]
    async fn staging_pressure_is_retryable() -> Result<(), McpJourneyError> {
        let server = Box::pin(
            WyrdTestServer::builder()
                .with_scribe_outbox_capacity_for_test(1)
                .start_bound(),
        )
        .await?;
        let admin = server
            .bootstrap_service("mcp-pressure-admin", &["admin"])
            .await?;
        let cards = Cards::connect_with(&client(&server, api_key(&admin)?)?)
            .await
            .expect("handle connects");
        let fixture = register_fixture(&cards).await?;
        let agent = server
            .credential_registered_service(&fixture.service_ref, &["editor"])
            .await?;
        let agent_client = connect_rooted(&server, &agent).await?;
        let offered = agent_client.list_all_tools().await?;
        for tool in [EXECUTE, ENQUEUE] {
            assert!(
                offered.iter().any(|offered| offered.name == tool),
                "the editor is offered {tool}"
            );
            let refused = refusal(
                agent_client
                    .call_tool(call(tool, fixture.assert_request(PASSING_ANSWER))?)
                    .await,
            )?;
            assert!(
                refused.contains("WYRD_VERIFICATION_429_STAGING_FULL")
                    && refused.contains("retry_after_seconds"),
                "{tool} is refused as retryable staging pressure: {refused}"
            );
        }

        server.flush_bifrost().await?;
        let staged = rows(
            &agent_client,
            "SELECT count(*) FROM bifrost.verification.execution_inputs".to_owned(),
        )
        .await?;
        assert_eq!(
            staged,
            serde_json::json!([[0]]),
            "a refused call stages nothing"
        );
        agent_client.cancel().await?;
        Ok(())
    }

    /// A Service's configured deployment environment is a pass-through tag on
    /// both of its verification subjects over MCP.
    ///
    /// Against `configured-service`: the Service check and the Agent
    /// component's check run directly and queued under `prod`, `staging`, and
    /// no tag, with the same verdict; each result, `verification.get_execution`
    /// read, and stored `execution_inputs` and `results` row carries exactly
    /// the supplied tag, or none.
    ///
    /// # Errors
    ///
    /// Returns server startup, bootstrap, registration, MCP transport, flush,
    /// or tool-call failures, or an error when an execution never settles.
    ///
    /// # Panics
    ///
    /// Panics when a call is refused, a verdict differs between tags, or a
    /// result or stored row names another subject or environment.
    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "requires the Postgres-backed Bifrost journey lane"]
    async fn configured_service_context_keeps_subject_and_environment()
    -> Result<(), McpJourneyError> {
        let server = Box::pin(
            WyrdTestServer::builder()
                .with_verification_runtime_for_test()
                .start_bound(),
        )
        .await?;
        let admin = server
            .bootstrap_service("mcp-configured-admin", &["admin"])
            .await?;
        let cards = Cards::connect_with(&client(&server, api_key(&admin)?)?)
            .await
            .expect("handle connects");
        let receipt = Box::pin(cards.declare_from_path(Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../fixtures/cards/observe_a_run/configured-service.yaml"
        ))))
        .await?;
        let uid = |name: &str| -> Result<String, McpJourneyError> {
            Ok(receipt
                .outcomes
                .iter()
                .find(|outcome| outcome.card_ref.name.as_str() == name)
                .and_then(|outcome| outcome.card_ref.uid.as_ref())
                .ok_or("the fixture registers the named Card")?
                .to_string())
        };
        let (service, agent) = (uid("configured-service")?, uid("observed-agent")?);
        let (service_check, agent_check) = (uid("service-check")?, uid("agent-check")?);
        let caller = server
            .credential_registered_service(&receipt.root, &["editor"])
            .await?;
        let caller_client = connect_rooted(&server, &caller).await?;
        let request = |verifier: &str, subject: &str, environment: Option<&str>| {
            let mut body = serde_json::json!({
                "verifier_card_uid": verifier,
                "card_uid": subject,
                "inputs": { "answer": { "kind": "json", "value": "yes" } },
            });
            if let Some(environment) = environment {
                body["deployment_environment"] = environment.into();
            }
            body
        };

        let mut recorded = Vec::new();
        for (verifier, subject) in [(&service_check, &service), (&agent_check, &agent)] {
            for environment in [Some("prod"), Some("staging"), None] {
                let direct = structured(
                    caller_client
                        .call_tool(call(EXECUTE, request(verifier, subject, environment))?)
                        .await?,
                )?;
                assert_eq!(text(&direct, "/subject/uid")?, subject.as_str(), "{direct}");
                assert_eq!(text(&direct, "/outcome/verdict")?, "passed", "{direct}");
                assert_eq!(
                    direct["deployment_environment"].as_str(),
                    environment,
                    "{direct}"
                );
                recorded.push((text(&direct, "/execution_id")?.to_owned(), environment));

                let accepted = structured(
                    caller_client
                        .call_tool(call(ENQUEUE, request(verifier, subject, environment))?)
                        .await?,
                )?;
                let execution_id = text(&accepted, "/execution_id")?.to_owned();
                let state = settle(&caller_client, &execution_id).await?;
                assert_eq!(state["status"], "completed", "{state}");
                assert_eq!(text(&state, "/result/subject/uid")?, subject.as_str());
                assert_eq!(
                    text(&state, "/result/outcome/verdict")?,
                    "passed",
                    "{state}"
                );
                assert_eq!(
                    state["result"]["deployment_environment"].as_str(),
                    environment,
                    "{state}"
                );
                recorded.push((execution_id, environment));
            }
        }

        server.flush_bifrost().await?;
        for (execution_id, environment) in recorded {
            for table in ["execution_inputs", "results"] {
                let stored = rows(
                    &caller_client,
                    format!(
                        "SELECT deployment_environment FROM bifrost.verification.{table} \
                         WHERE execution_id = '{execution_id}'"
                    ),
                )
                .await?;
                assert_eq!(
                    stored,
                    serde_json::json!([[environment]]),
                    "{table} row of {execution_id}"
                );
            }
        }
        caller_client.cancel().await?;
        server.shutdown().await?;
        Ok(())
    }

    /// A trace Verifier requiring its trace's span names to be `retrieve`
    /// then `rank`.
    const TRACE_VERIFIER: &str = "apiVersion: wyrd/v1\nkind: Verifier\nmetadata:\n  name: mcp-trace\n  version: 1.0.0\nspec:\n  inputs:\n    trace: {type: trace}\n  requirements:\n    - id: whole-trace\n      statement: The trace retrieves then ranks\n      check: {kind: trace, on: inputs.trace, span_selector: \"$.spans[*].name\", operator: equals, expected: [retrieve, rank]}\n";

    /// Write `names` as consecutive spans of `trace` through native ingest
    /// as `client`, then publish them.
    ///
    /// # Errors
    ///
    /// Returns describe, encode, ingest, or publication failures.
    ///
    /// # Panics
    ///
    /// Panics when the described span schema does not assemble the span
    /// rows, as `canonical_signals::trace_spans` does.
    async fn write_spans(
        server: &WyrdTestServer,
        client: &wyrd_client::WyrdClient,
        trace: &[u8; 16],
        names: &[(u64, &str)],
    ) -> Result<(), McpJourneyError> {
        let described =
            wyrd_client::bifrost::TableConfig::describe_with(client, "bifrost.traces.spans")
                .await?;
        let start = chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default();
        let spans: Vec<(u64, &str, i64)> = names
            .iter()
            .map(|&(id, name)| (id, name, start + i64::try_from(id).unwrap_or_default()))
            .collect();
        let batch = wyrd_testing::bifrost::canonical_signals::trace_spans(
            described.user_schema(),
            trace,
            &spans,
        );
        let mut ipc = Vec::new();
        let mut writer = arrow::ipc::writer::StreamWriter::try_new(&mut ipc, &batch.schema())?;
        writer.write(&batch)?;
        writer.finish()?;
        drop(writer);
        wyrd_testing::bifrost::RawIngest::connect(client)
            .await?
            .insert("bifrost.traces.spans", uuid::Uuid::now_v7(), ipc)
            .await?;
        server.flush_bifrost().await?;
        Ok(())
    }

    /// An agent's trace evidence is judged whole, and a queued execution
    /// waits for evidence that has not landed (REQ-040/045/046,
    /// AC-014/019/020).
    ///
    /// A trace written partly by the agent's own Service and partly by an
    /// untagged writer is judged whole by `verification.execute`. Direct
    /// execution over an absent trace answers at once with an inconclusive
    /// requirement. `verification.enqueue` over an absent trace is
    /// `retrying` with `evidence_unavailable`; once the spans land and the
    /// retry is due, the same execution completes and passes, with no
    /// second enqueue from the agent.
    ///
    /// # Errors
    ///
    /// Returns server, fixture, ingest, MCP transport, or tool-call
    /// failures, or an error when the execution never retries or settles.
    ///
    /// # Panics
    ///
    /// Panics when a verdict, requirement outcome, or execution identity
    /// differs.
    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "requires the Postgres-backed Bifrost journey lane"]
    async fn trace_evidence_keeps_whole_trace_and_waits_when_submitted()
    -> Result<(), McpJourneyError> {
        let server = Box::pin(
            WyrdTestServer::builder()
                .with_verification_runtime_for_test()
                .start_bound(),
        )
        .await?;
        let tenant = server.data_tenant_id();
        server
            .ensure_builtin_table_for_test(tenant, "traces", "spans")
            .await?;
        let admin = server
            .bootstrap_service("mcp-trace-admin", &["admin"])
            .await?;
        let admin_client = client(&server, api_key(&admin)?)?;
        let cards = Cards::connect_with(&admin_client)
            .await
            .expect("handle connects");
        let fixture = register_fixture(&cards).await?;
        let root = std::env::temp_dir().join(format!("wyrd-mcp-trace-{}", uuid::Uuid::now_v7()));
        std::fs::create_dir_all(&root)?;
        std::fs::write(root.join("verifier.yaml"), TRACE_VERIFIER)?;
        let verifier = Box::pin(cards.declare_from_path(&root.join("verifier.yaml"))).await?;
        std::fs::remove_dir_all(&root)?;
        let verifier = verifier.root.uid.ok_or("a registered Card has a UID")?;
        let agent = server
            .credential_registered_service(&fixture.service_ref, &["editor"])
            .await?;
        let agent_client = connect_rooted(&server, &agent).await?;
        let request = |trace: &[u8; 16]| {
            serde_json::json!({
                "verifier_card_uid": verifier, "card_uid": fixture.service_uid,
                "inputs": { "trace": { "kind": "trace", "trace_id": uuid::Uuid::from_bytes(*trace).simple().to_string() } },
            })
        };

        let whole = *uuid::Uuid::now_v7().as_bytes();
        write_spans(
            &server,
            &rooted_client(&server, &agent)?,
            &whole,
            &[(1, "retrieve")],
        )
        .await?;
        write_spans(&server, &admin_client, &whole, &[(2, "rank")]).await?;
        let judged = structured(
            agent_client
                .call_tool(call(EXECUTE, request(&whole))?)
                .await?,
        )?;
        assert_eq!(judged["outcome"]["verdict"], "passed", "{judged}");

        let absent = structured(
            agent_client
                .call_tool(call(EXECUTE, request(uuid::Uuid::now_v7().as_bytes()))?)
                .await?,
        )?;
        assert_eq!(
            text(&absent, "/outcome/requirements/0/outcome")?,
            "inconclusive"
        );
        assert!(
            text(&absent, "/outcome/requirements/0/reason")?.contains("not yet landed"),
            "{absent}"
        );

        let late = *uuid::Uuid::now_v7().as_bytes();
        let accepted = structured(
            agent_client
                .call_tool(call(ENQUEUE, request(&late))?)
                .await?,
        )?;
        let execution_id = text(&accepted, "/execution_id")?.to_owned();
        let deadline = tokio::time::Instant::now() + SETTLE;
        loop {
            let state = agent_client
                .call_tool(call(
                    GET_EXECUTION,
                    serde_json::json!({ "execution_id": execution_id }),
                )?)
                .await;
            if let Ok(state) = state
                && state.is_error != Some(true)
            {
                let state = structured(state)?;
                if state["status"] == "retrying" {
                    assert_eq!(state["error"]["code"], "evidence_unavailable", "{state}");
                    break;
                }
                assert!(
                    matches!(state["status"].as_str(), Some("pending" | "running")),
                    "an absent trace waits: {state}"
                );
            }
            if tokio::time::Instant::now() >= deadline {
                return Err(format!("execution {execution_id} never retried").into());
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        write_spans(
            &server,
            &admin_client,
            &late,
            &[(1, "retrieve"), (2, "rank")],
        )
        .await?;
        wyrd_testing::verification::VerificationFixture::provision(server.state(), tenant)
            .await?
            .make_retry_due(execution_id.parse()?)
            .await?;
        let state = settle(&agent_client, &execution_id).await?;
        assert_eq!(state["status"], "completed", "{state}");
        assert_eq!(text(&state, "/result/execution_id")?, execution_id);
        assert_eq!(state["result"]["outcome"]["verdict"], "passed", "{state}");
        agent_client.cancel().await?;
        server.shutdown().await?;
        Ok(())
    }
}
