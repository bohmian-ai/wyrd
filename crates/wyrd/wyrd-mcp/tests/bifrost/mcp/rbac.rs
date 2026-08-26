mod pg_tests {
    //! Tests verifying that Bifrost tools propagate RBAC errors from the C2 routes.
    //!
    //! Uses WyrdTestServer (embedded PgFixture — no external DB) to run a real
    //! server instance. A service bootstrapped with no roles has no
    //! `bifrost_table:read` permission and must receive WYRD_PERMISSION_403_DENIED_RBAC.

    use std::sync::Arc;

    use serde_json::json;
    use skald_tool::{AgentTool, ToolError, ToolRegistry};
    use wyrd_client::auth::AuthMiddleware;
    use wyrd_client::transport::{HttpConfig, HttpTransport, ResolvedCredential};
    use wyrd_client::{WyrdClient, config::ClientConfig};
    use wyrd_mcp::bifrost::register_bifrost_tools;
    use wyrd_testing::WyrdTestServer;
    use wyrd_testing::bifrost::seed_query_fixture;

    fn client_from_bootstrap(base_url: &str, api_key: secrecy::SecretString) -> WyrdClient {
        let mut config = ClientConfig::default();
        config.http.base_url = base_url.to_owned();
        config.api_key = Some(api_key);
        WyrdClient::with_config(config).expect("test client assembles")
    }

    /// Build an authenticated-shape client for schema-only discovery tests.
    fn dummy_client() -> WyrdClient {
        WyrdClient::with_config(ClientConfig {
            api_key: Some(secrecy::SecretString::from("test-key")),
            ..ClientConfig::default()
        })
        .expect("dummy client assembles")
    }

    /// Build a client over the fixture's already exchanged bearer token.
    fn client_from_bearer(base_url: &str, token: String) -> WyrdClient {
        let config = ClientConfig {
            http: HttpConfig {
                base_url: base_url.to_owned(),
                ..HttpConfig::default()
            },
            ..ClientConfig::default()
        };
        let auth = AuthMiddleware::new(
            &config,
            ResolvedCredential::BearerToken(secrecy::SecretString::from(token)),
        )
        .expect("bearer auth assembles");
        let http = HttpTransport::new(&config.http, Arc::clone(&auth)).expect("HTTP assembles");
        WyrdClient::from_parts(auth, http, config.grpc)
    }

    /// Discover the query tool through the public registry listing contract.
    fn discover_query_tool(registry: &ToolRegistry) -> Arc<dyn AgentTool> {
        let listed = registry.names();
        assert_eq!(
            listed,
            vec![
                "bifrost.describe_table".to_owned(),
                "bifrost.list_errors".to_owned(),
                "bifrost.list_permissions".to_owned(),
                "bifrost.list_tables".to_owned(),
                "bifrost.query".to_owned(),
            ]
        );
        let name = listed
            .iter()
            .find(|name| name.as_str() == "bifrost.query")
            .expect("tools/list exposes bifrost.query");
        registry.resolve(name).expect("listed query tool resolves")
    }

    /// The public tools/list projection exposes the closed query result schema.
    #[test]
    fn bifrost_tools_list_exposes_closed_query_schema() {
        let registry = ToolRegistry::new();
        register_bifrost_tools(&registry, Arc::new(dummy_client())).expect("tools register");
        let query = discover_query_tool(&registry);
        let schema = query.output_schema();
        assert_eq!(schema["additionalProperties"], false);
        assert_eq!(
            schema["properties"]["schema"]["additionalProperties"],
            false
        );
        assert_eq!(
            schema["properties"]["terminal"]["additionalProperties"],
            false
        );
        assert_eq!(
            schema["x-wyrd-error"]["required"],
            json!([
                "code",
                "status",
                "title",
                "detail",
                "remediation",
                "safe_details"
            ])
        );
        assert_eq!(schema["x-wyrd-error"]["additionalProperties"], false);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn bifrost_rbac_list_tables_without_permission_returns_permission_denied() {
        let srv = WyrdTestServer::start_bound()
            .await
            .expect("test server starts");
        let base_url = srv
            .base_url()
            .expect("bound server has base url")
            .to_owned();

        let bootstrap = srv
            .bootstrap_agent("bifrost-rbac-list-tables", &[])
            .await
            .expect("agent bootstraps");
        let api_key = bootstrap
            .api_key()
            .expect("agent bootstrap has api key")
            .clone();

        let client = client_from_bootstrap(&base_url, api_key);
        let registry = ToolRegistry::new();
        register_bifrost_tools(&registry, Arc::new(client)).expect("tools register");

        let tool = registry
            .resolve("bifrost.list_tables")
            .expect("list_tables registered");
        let err = tool
            .invoke(json!({}))
            .await
            .expect_err("no bifrost_table:read permission — must fail");

        let detail = err.to_string();
        assert!(
            detail.contains("WYRD_PERMISSION_403_DENIED_RBAC"),
            "expected RBAC denial code in error detail, got: {detail}"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn bifrost_rbac_describe_table_without_permission_returns_permission_denied() {
        let srv = WyrdTestServer::start_bound()
            .await
            .expect("test server starts");
        let base_url = srv
            .base_url()
            .expect("bound server has base url")
            .to_owned();

        let bootstrap = srv
            .bootstrap_agent("bifrost-rbac-describe-table", &[])
            .await
            .expect("agent bootstraps");
        let api_key = bootstrap
            .api_key()
            .expect("agent bootstrap has api key")
            .clone();

        let client = client_from_bootstrap(&base_url, api_key);
        let registry = ToolRegistry::new();
        register_bifrost_tools(&registry, Arc::new(client)).expect("tools register");

        let tool = registry
            .resolve("bifrost.describe_table")
            .expect("describe_table registered");
        let err = tool
            .invoke(json!({"namespace": "bifrost", "name": "nonexistent"}))
            .await
            .expect_err("no bifrost_table:read permission — must fail");

        let detail = err.to_string();
        assert!(
            detail.contains("WYRD_PERMISSION_403_DENIED_RBAC"),
            "expected RBAC denial code in error detail, got: {detail}"
        );
    }

    /// Proves query payload authorization reaches the real server Gate.
    #[tokio::test(flavor = "multi_thread")]
    async fn bifrost_query_without_permission_returns_gate_denial() {
        let srv = WyrdTestServer::start_bound()
            .await
            .expect("test server starts");
        let base_url = srv
            .base_url()
            .expect("bound server has base url")
            .to_owned();
        let bootstrap = srv
            .bootstrap_agent("bifrost-rbac-query", &[])
            .await
            .expect("agent bootstraps");
        let api_key = bootstrap
            .api_key()
            .expect("agent bootstrap has api key")
            .clone();
        let registry = ToolRegistry::new();
        register_bifrost_tools(
            &registry,
            Arc::new(client_from_bootstrap(&base_url, api_key)),
        )
        .expect("tools register");

        let baseline = srv
            .bifrost_read_decision_count()
            .await
            .expect("read-decision baseline");
        let tool = discover_query_tool(&registry);
        let error = tool
            .invoke(json!({"sql": "SELECT 1"}))
            .await
            .expect_err("token lacks bifrost_query:read");
        match error {
            ToolError::StructuredInvocation(error) => {
                assert_eq!(error.code, "WYRD_PERMISSION_403_DENIED_RBAC");
                assert_eq!(error.status, 403);
                assert_eq!(error.title, "Permission denied (RBAC)");
                assert_eq!(
                    error.detail,
                    format!("principal {} lacks BifrostQuery/Read", bootstrap.id())
                );
                assert_eq!(
                    error.remediation,
                    "Request the required role from a workspace admin."
                );
                assert_eq!(
                    error.safe_details,
                    Some(json!({
                        "principal": bootstrap.id().to_string(),
                        "required": {"action": "read", "resource": "bifrost_query"}
                    }))
                );
            }
            other => panic!("query denial must preserve structured metadata: {other:?}"),
        }
        assert_eq!(
            srv.bifrost_read_decision_count()
                .await
                .expect("read-decision after denial"),
            baseline
        );
    }

    /// Proves the authorized query adapter returns rows and the complete terminal.
    #[tokio::test(flavor = "multi_thread")]
    async fn bifrost_query_success_returns_structured_terminal() {
        let srv = WyrdTestServer::start_bound()
            .await
            .expect("test server starts");
        let fixture = seed_query_fixture(&srv, "mcp-success")
            .await
            .expect("query fixture seeds");
        let registry = ToolRegistry::new();
        register_bifrost_tools(
            &registry,
            Arc::new(client_from_bearer(&fixture.endpoint, fixture.token)),
        )
        .expect("tools register");
        let tool = discover_query_tool(&registry);
        let output = tool
            .invoke(json!({"sql": format!("SELECT id, value FROM {}", fixture.table)}))
            .await
            .expect("authorized query succeeds");
        assert_eq!(output["rows"], json!(fixture.expected_rows));
        let terminal = &output["terminal"];
        assert_eq!(terminal["outcome"], "success");
        assert_eq!(terminal["freshness"], "complete");
        assert_eq!(terminal["row_count"], 2);
        assert_eq!(terminal["warnings"], json!([]));
        assert_eq!(
            terminal["source_completion"],
            json!([
                {"source": "iceberg", "outcome": "complete"},
                {"source": "hot_sealed", "outcome": "complete"}
            ])
        );
        assert_eq!(terminal["error"], serde_json::Value::Null);
    }

    /// A real zero-match query returns its schema, no rows, and a successful terminal.
    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "runs in the gated Bifrost real-server journey lane"]
    async fn bifrost_query_zero_rows_returns_authoritative_schema() {
        let srv = WyrdTestServer::start_bound()
            .await
            .expect("test server starts");
        let fixture = seed_query_fixture(&srv, "mcp-zero-rows")
            .await
            .expect("query fixture seeds");
        let registry = ToolRegistry::new();
        register_bifrost_tools(
            &registry,
            Arc::new(client_from_bearer(&fixture.endpoint, fixture.token)),
        )
        .expect("tools register");

        let output = discover_query_tool(&registry)
            .invoke(json!({
                "sql": format!("SELECT id, value FROM {} WHERE id < 0", fixture.table)
            }))
            .await
            .expect("zero-match query succeeds");

        assert_eq!(
            output["schema"],
            json!({
                "fields": [
                    {"name": "id", "data_type": "Int64", "nullable": false},
                    {"name": "value", "data_type": "Utf8", "nullable": false}
                ]
            })
        );
        assert_eq!(output["rows"], json!([]));
        assert_eq!(output["terminal"]["outcome"], "success");
        assert_eq!(output["terminal"]["freshness"], "complete");
        assert_eq!(output["terminal"]["row_count"], 0);
        assert_eq!(output["terminal"]["warnings"], json!([]));
        assert_eq!(output["terminal"]["error"], serde_json::Value::Null);
    }

    /// Invalid SQL preserves the exact Gate problem metadata without prose parsing.
    #[tokio::test(flavor = "multi_thread")]
    async fn bifrost_query_invalid_sql_preserves_structured_error() {
        let srv = WyrdTestServer::start_bound()
            .await
            .expect("test server starts");
        let fixture = seed_query_fixture(&srv, "mcp-invalid-sql")
            .await
            .expect("query fixture seeds");
        let registry = ToolRegistry::new();
        register_bifrost_tools(
            &registry,
            Arc::new(client_from_bearer(&fixture.endpoint, fixture.token)),
        )
        .expect("tools register");
        let error = discover_query_tool(&registry)
            .invoke(json!({"sql": "SELEC id FROM vala.bifrost.missing"}))
            .await
            .expect_err("invalid SQL is rejected");
        match error {
            ToolError::StructuredInvocation(error) => {
                assert_eq!(error.code, "WYRD_SPEC_502_UPSTREAM_FAILURE");
                assert_eq!(error.status, 502);
                assert_eq!(error.title, "Upstream dependency failed");
                assert_eq!(
                    error.detail,
                    "server error: invalid or unsupported query SQL: parse error: SQL error: ParserError(\"Expected: an SQL statement, found: SELEC at Line: 1, Column: 1\")"
                );
                assert_eq!(
                    error.remediation,
                    "Check the upstream dependency health and retry policy."
                );
                assert_eq!(
                    error.safe_details,
                    Some(json!({
                        "original_code": "WYRD_VALA_400_QUERY_INVALID_SQL",
                        "original_details": {
                            "data": {
                                "detail": "parse error: SQL error: ParserError(\"Expected: an SQL statement, found: SELEC at Line: 1, Column: 1\")"
                            },
                            "variant": "query_invalid_sql"
                        }
                    }))
                );
            }
            other => panic!("invalid SQL must preserve structured metadata: {other:?}"),
        }
    }

    /// Collection bounds preserve the exact typed oversized-result metadata.
    #[tokio::test(flavor = "multi_thread")]
    async fn bifrost_query_oversized_result_preserves_structured_error() {
        let srv = WyrdTestServer::start_bound()
            .await
            .expect("test server starts");
        let fixture = seed_query_fixture(&srv, "mcp-oversized")
            .await
            .expect("query fixture seeds");
        let registry = ToolRegistry::new();
        register_bifrost_tools(
            &registry,
            Arc::new(client_from_bearer(&fixture.endpoint, fixture.token)),
        )
        .expect("tools register");
        let error = discover_query_tool(&registry)
            .invoke(json!({
                "sql": format!("SELECT id, value FROM {}", fixture.table),
                "max_rows": 1
            }))
            .await
            .expect_err("one-row bound rejects the two-row result");
        match error {
            ToolError::StructuredInvocation(error) => {
                assert_eq!(error.code, "WYRD_VALA_413_QUERY_RESULT_TOO_LARGE");
                assert_eq!(error.status, 413);
                assert_eq!(error.title, "Query result too large");
                assert_eq!(error.detail, "query result exceeds configured bounds");
                assert_eq!(
                    error.remediation,
                    "Reduce the query result or raise the caller's explicit collection limit within its hard ceiling."
                );
                assert_eq!(error.safe_details, None);
            }
            other => panic!("oversized result must preserve structured metadata: {other:?}"),
        }
    }
}
