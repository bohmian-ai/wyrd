//! Tier-1 journey: an agent discovers Bifrost through the real `/mcp` endpoint.
//!
//! Discovery is the half of the contract that decides what an agent can even
//! attempt: the advertised catalog, the compact table list its own tenant
//! authorizes, and the physical description it needs to write bounded SQL
//! without a second lookup. The journey drives all three through a real `rmcp`
//! client so what it observes is what an agent observes.

use crate::connectivity::{McpJourneyError, discover, problem, refusal, structured, transport};

/// The discovery journey's Postgres-backed cases.
mod pg_tests {
    use super::{McpJourneyError, discover, problem, refusal, structured, transport};

    use rmcp::ClientServiceExt as _;
    use rmcp::model::{CallToolRequestParams, Tool};
    use rmcp::service::{RoleClient, RunningService};
    use serde_json::{Value as JsonValue, json};
    use vala_bifrost_redux::catalog::{CreateTableRequest, TableRef};
    use vala_bifrost_redux::namespaces::BifrostNamespace;
    use wyrd_client::transport::credential::ResolvedCredential;
    use wyrd_testing::WyrdTestServer;
    use wyrd_testing::bifrost::canonical_signals;
    use wyrd_testing::bifrost::seed_query_fixture;

    /// The exact catalog an ordinary Wyrd server advertises over `/mcp` to a
    /// caller that also holds tenant principal administration, `verifier:run`, and
    /// `operators:write`: every read tool, then that caller's write tools.
    const ADVERTISED_TOOLS: [&str; 35] = [
        "bifrost.list_tables",
        "bifrost.describe_table",
        "bifrost.query",
        "principals.list_credentials",
        "cards.get",
        "cards.list",
        "cards.get_by_ref",
        "verification.get_binding",
        "verification.get_run",
        "operator_connections.list",
        "operator_connections.get",
        "gateway.list_provider_credentials",
        "gateway.list_provider_deployments",
        "gateway.get_fallback_policy",
        "gateway.get_governance_policy",
        "gateway.get_capture_policy",
        "gateway.get_provider_credential",
        "gateway.get_provider_deployment",
        "gateway.put_provider_credential",
        "gateway.put_provider_deployment",
        "gateway.put_fallback_policy",
        "gateway.put_governance_policy",
        "gateway.put_capture_policy",
        "gateway.revoke_provider_credential",
        "gateway.delete_provider_credential",
        "gateway.delete_provider_deployment",
        "gateway.delete_fallback_policy",
        "gateway.delete_governance_policy",
        "wyrd.guide",
        "principals.revoke_credential",
        "verification.start_run",
        "verification.execute",
        "operator_connections.create",
        "operator_connections.update",
        "operator_connections.disable",
    ];

    /// A bound MCP reader and the custom table used to execute guide examples.
    struct GuideInvestigation {
        /// Owns the real catalog and query service.
        server: WyrdTestServer,
        /// Authenticated MCP connection under test.
        agent: RunningService<RoleClient, ()>,
        /// Qualified custom table substituted into guide examples.
        table: String,
    }

    impl GuideInvestigation {
        /// Seed the existing query fixture and connect through MCP.
        ///
        /// # Errors
        /// Returns server, seed, or transport failures.
        async fn start() -> Result<Self, McpJourneyError> {
            let server = WyrdTestServer::start_bound().await?;
            let fixture = seed_query_fixture(&server, "mcp-guide").await?;
            let agent = ()
                .serve_with_lifecycle(
                    transport(
                        &server,
                        ResolvedCredential::BearerToken(fixture.token.into()),
                        None,
                    )?,
                    discover(),
                )
                .await?;
            Ok(Self {
                server,
                agent,
                table: fixture.table,
            })
        }

        /// Read one tool's structured response using its advertised argument object.
        ///
        /// # Errors
        /// Returns invalid fixture arguments, transport failures, or a tool refusal.
        async fn read(
            &self,
            name: &str,
            arguments: JsonValue,
        ) -> Result<JsonValue, McpJourneyError> {
            let arguments = arguments
                .as_object()
                .ok_or("tool arguments are an object")?
                .clone();
            let request = CallToolRequestParams::new(name.to_owned()).with_arguments(arguments);
            structured(self.agent.call_tool(request).await?)
        }

        /// Check discovery instructions and closed, read-only descriptors.
        ///
        /// # Errors
        /// Returns missing discovery fields or a catalog read failure.
        async fn catalog(&self) -> Result<Vec<Tool>, McpJourneyError> {
            let info = self.agent.peer_info().ok_or("server discovery info")?;
            let instructions = info.instructions.as_deref().ok_or("server instructions")?;
            assert!(instructions.contains("tools/list"));
            assert!(instructions.contains("wyrd.guide"));
            let tools = self.agent.list_all_tools().await?;
            for tool in &tools {
                assert_eq!(
                    tool.input_schema.get("type"),
                    Some(&json!("object")),
                    "MCP requires an object input schema: {}",
                    tool.name
                );
            }
            for name in ["cards.list", "cards.get_by_ref", "wyrd.guide"] {
                let tool = tools
                    .iter()
                    .find(|tool| tool.name == name)
                    .ok_or("investigation tool is advertised")?;
                assert_eq!(
                    tool.annotations.as_ref().and_then(|a| a.read_only_hint),
                    Some(true)
                );
                assert_eq!(
                    tool.input_schema.get("additionalProperties"),
                    Some(&json!(false))
                );
                assert!(tool.output_schema.is_some());
            }
            Ok(tools)
        }

        /// Validate every topic and execute each SQL example against current schemas.
        ///
        /// # Errors
        /// Returns a malformed guide, missing tool, or failed query example.
        async fn topics(&self, tools: &[Tool]) -> Result<(), McpJourneyError> {
            for topic in ["overview", "cards", "verification", "bifrost_sql"] {
                let guide = self.read("wyrd.guide", json!({ "topic": topic })).await?;
                assert_eq!(guide["topic"], topic);
                let steps = guide["steps"].as_array().ok_or("guide has steps")?;
                assert!(!steps.is_empty());
                let examples = guide["examples"].as_array().ok_or("guide has examples")?;
                for example in examples {
                    let name = example["tool"].as_str().ok_or("example names its tool")?;
                    assert!(
                        tools.iter().any(|tool| tool.name == name),
                        "example names a shipped tool: {example}"
                    );
                    assert!(example["purpose"].as_str().is_some());
                    assert!(example["arguments"].is_object());
                    if name == "bifrost.query" {
                        self.execute_sql_example(example).await?;
                    }
                }
            }
            Ok(())
        }

        /// Substitute only documented identifiers; Oracle validates the SQL and fields.
        ///
        /// # Errors
        /// Returns missing SQL or a query refusal.
        async fn execute_sql_example(&self, example: &JsonValue) -> Result<(), McpJourneyError> {
            let mut arguments = example["arguments"].clone();
            let trace_id =
                wyrd_spec::vala::ids::TraceId::from_bytes(canonical_signals::TRACE_ID)?.to_hex();
            let sql = arguments["sql"]
                .as_str()
                .ok_or("query example has sql")?
                .replace("<custom_table>", &self.table)
                .replace("<subject_card_uid>", "01890f28-7c4a-7cc3-98e7-4f4a3c2d1b00")
                .replace("<result_id>", "guide-absent-result")
                .replace("<trace_id>", &trace_id);
            arguments["sql"] = json!(sql);
            let result = self.read("bifrost.query", arguments).await?;
            assert_eq!(
                result["terminal"]["outcome"], "success",
                "{example}: {result}"
            );
            Ok(())
        }

        /// Refuse unknown topics and caller-supplied tenant data.
        ///
        /// # Errors
        /// Returns transport failures or an unexpectedly successful call.
        async fn reject_invalid_topics(&self) -> Result<(), McpJourneyError> {
            for arguments in [
                json!({ "topic": "unknown" }),
                json!({ "topic": "overview", "tenant": "other" }),
            ] {
                let request = CallToolRequestParams::new("wyrd.guide").with_arguments(
                    arguments
                        .as_object()
                        .ok_or("invalid arguments object")?
                        .clone(),
                );
                let failure = refusal(self.agent.call_tool(request).await)?;
                assert!(failure.contains("WYRD_SPEC_400_VALIDATION"), "{failure}");
            }
            Ok(())
        }

        /// Close the MCP connection before the server drains.
        ///
        /// # Errors
        /// Returns connection or server shutdown failures.
        async fn shutdown(self) -> Result<(), McpJourneyError> {
            self.agent.cancel().await?;
            self.server.shutdown().await?;
            Ok(())
        }
    }

    /// An authenticated agent discovers the guide and executes its bounded SQL examples.
    ///
    /// # Errors
    /// Returns fixture, MCP, query, or shutdown failures.
    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "requires the Postgres-backed Bifrost journey lane"]
    async fn agent_discovers_mcp_guide_and_sql_examples() -> Result<(), McpJourneyError> {
        let investigation = GuideInvestigation::start().await?;
        let tools = investigation.catalog().await?;
        investigation.topics(&tools).await?;
        investigation.reject_invalid_topics().await?;
        investigation.shutdown().await
    }

    /// An agent sees exactly the advertised Bifrost, principal, and gateway
    /// tools, only its own tenant's tables, and the complete physical layout
    /// of the one it selects.
    ///
    /// The two tenants are what make the list assertion a tenancy claim rather
    /// than a formatting one: both tables exist in the same catalog, and only
    /// the caller's own is nameable — by listing, by describing, and by the
    /// error a foreign name produces, which is the same not-found the absent
    /// name produces so nothing leaks across the boundary.
    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "requires the Postgres-backed Bifrost journey lane"]
    async fn agent_discovers_only_authorized_tables_and_layout() -> Result<(), McpJourneyError> {
        let server = WyrdTestServer::start_bound().await?;
        let fixture = seed_query_fixture(&server, "mcp-discovery").await?;
        let authorized = fixture
            .table
            .strip_prefix("vala.bifrost.")
            .ok_or("the fixture table is Bifrost-qualified")?
            .to_owned();

        let foreign_tenant = server.seed_tenant("mcp-discovery-foreign").await?;
        let foreign_table = format!("foreign_{}", uuid::Uuid::now_v7().simple());
        server
            .state()
            .bifrost_catalog()
            .ok_or("the test server composes a Bifrost catalog")?
            .create_table(CreateTableRequest {
                table: TableRef::new(BifrostNamespace::Bifrost, &foreign_table),
                user_fields: vec![arrow::datatypes::Field::new(
                    "id",
                    arrow::datatypes::DataType::Int64,
                    false,
                )],
                tenant: foreign_tenant,
                physical_layout: None,
            })
            .await?;

        let client = ()
            .serve_with_lifecycle(
                transport(
                    &server,
                    ResolvedCredential::BearerToken(fixture.token.clone().into()),
                    None,
                )?,
                discover(),
            )
            .await?;

        let tools = client.list_all_tools().await?;
        let names: Vec<String> = tools.iter().map(|tool| tool.name.to_string()).collect();
        assert_eq!(
            names, ADVERTISED_TOOLS,
            "an ordinary server advertises the Bifrost, principal, Card, verification, \
             and gateway tools, and no test probe"
        );
        let list_connections = tools
            .iter()
            .find(|tool| tool.name == "operator_connections.list")
            .ok_or("the catalog advertises the Operator connection list tool")?;
        assert_eq!(
            list_connections.description.as_deref(),
            Some(
                "List this tenant's Operator provider connections as redacted metadata: ID, \
                 provider, name, status, and nonsecret config. Secrets are never returned. \
                 Requires operators:read."
            ),
            "the list descriptor promises only fields the redacted view returns"
        );

        let listed = structured(
            client
                .call_tool(CallToolRequestParams::new("bifrost.list_tables"))
                .await?,
        )?;
        let tables = listed["tables"]
            .as_array()
            .ok_or("list_tables returns a tables array")?;
        for table in tables {
            let keys: Vec<&String> = table
                .as_object()
                .ok_or("each listed table is an object")?
                .keys()
                .collect();
            assert_eq!(
                keys,
                vec!["namespace", "name", "status"],
                "the list projection stays compact"
            );
        }
        let listed_names: Vec<&str> = tables
            .iter()
            .filter_map(|table| table["name"].as_str())
            .collect();
        assert!(
            tables.iter().any(|table| {
                table["name"] == authorized
                    && table["namespace"] == serde_json::json!("vala.bifrost")
            }),
            "the caller's own table is listed in vala.bifrost: {tables:?}"
        );
        assert!(
            !listed_names.contains(&foreign_table.as_str()),
            "another tenant's table is never listed: {listed_names:?}"
        );

        let described = structured(
            client
                .call_tool(
                    CallToolRequestParams::new("bifrost.describe_table").with_arguments(
                        serde_json::json!({
                            "namespace": "vala.bifrost",
                            "name": authorized,
                        })
                        .as_object()
                        .ok_or("describe arguments are an object")?
                        .clone(),
                    ),
                )
                .await?,
        )?;
        assert_eq!(described["entry"]["name"], serde_json::json!(authorized));
        let fields = described["user_fields"]
            .as_array()
            .ok_or("describe returns the stored user field list")?;
        let field_names: Vec<&str> = fields
            .iter()
            .filter_map(|field| field["name"].as_str())
            .collect();
        assert!(
            field_names.contains(&"id") && field_names.contains(&"value"),
            "every user field is described: {field_names:?}"
        );
        let correlation = described["correlation_fields"]
            .as_array()
            .ok_or("describe returns the server-resolved correlation fields")?;
        let correlation_names: Vec<&str> = correlation
            .iter()
            .filter_map(|field| field["name"].as_str())
            .collect();
        assert!(
            correlation_names.contains(&"card_uid") && correlation_names.contains(&"run_id"),
            "the server-resolved correlation columns are their own class: {correlation_names:?}"
        );
        assert!(
            correlation.iter().all(|field| field["metadata"]
                .as_object()
                .is_some_and(|meta| !meta.is_empty())),
            "field metadata reaches the agent: {correlation:?}"
        );
        let layout = &described["physical_layout"];
        assert!(
            layout["partition_granularity"].is_string(),
            "the event-time partition granularity is described: {layout}"
        );
        assert!(
            layout["sort_keys"].as_array().is_some_and(
                |keys| !keys.is_empty() && keys.iter().all(|key| key["column"].is_string())
            ),
            "the ordered sort keys are described: {layout}"
        );
        assert!(
            layout["bloom_columns"].is_array(),
            "the Bloom columns are described: {layout}"
        );

        for absent in [
            format!("absent_{}", uuid::Uuid::now_v7().simple()),
            foreign_table.clone(),
        ] {
            let refusal = problem(
                client
                    .call_tool(
                        CallToolRequestParams::new("bifrost.describe_table").with_arguments(
                            serde_json::json!({"namespace": "vala.bifrost", "name": absent})
                                .as_object()
                                .ok_or("describe arguments are an object")?
                                .clone(),
                        ),
                    )
                    .await?,
            )?;
            assert_eq!(
                refusal["code"],
                serde_json::json!("WYRD_VALA_404_BIFROST_TABLE_NOT_FOUND"),
                "an absent or foreign table is the same canonical refusal: {refusal}"
            );
            assert_eq!(refusal["status"], serde_json::json!(404));
        }

        client.cancel().await?;

        let denied = server
            .bootstrap_service("mcp-discovery-denied", &[])
            .await?;
        let denied_client = ()
            .serve_with_lifecycle(
                transport(
                    &server,
                    ResolvedCredential::ApiKey(
                        denied
                            .api_key()
                            .ok_or("service bootstrap carries a key")?
                            .clone(),
                    ),
                    None,
                )?,
                discover(),
            )
            .await?;
        let describe_arguments = serde_json::json!({
            "namespace": "vala.bifrost",
            "name": authorized,
        })
        .as_object()
        .ok_or("describe arguments are an object")?
        .clone();
        let refusals = [
            CallToolRequestParams::new("bifrost.list_tables"),
            CallToolRequestParams::new("bifrost.describe_table").with_arguments(describe_arguments),
        ];
        for parameters in refusals {
            let tool = parameters.name.clone();
            let refusal = problem(denied_client.call_tool(parameters).await?)?;
            assert_eq!(
                refusal["code"],
                serde_json::json!("WYRD_PERMISSION_403_DENIED_RBAC"),
                "{tool} denies an under-scoped principal"
            );
        }
        denied_client.cancel().await?;

        server.shutdown().await?;
        Ok(())
    }
}
