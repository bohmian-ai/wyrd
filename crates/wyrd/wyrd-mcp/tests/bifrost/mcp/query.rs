//! Tier-1 journeys: an agent reads Bifrost through `bifrost.query`.
//!
//! The query tool is the only Bifrost capability that executes anything, so
//! these journeys own the two halves an agent depends on: a repairable refusal
//! before any row runs, and one complete positional result after Oracle's own
//! terminal settlement. Both are driven by a real `rmcp` client against the
//! real `/mcp` endpoint, never by calling the adapter in process.

use crate::connectivity::{McpJourneyError, discover, problem, structured, transport};

mod pg_tests {
    use super::{McpJourneyError, discover, problem, structured, transport};

    use std::time::Duration;

    use rmcp::ClientServiceExt as _;
    use rmcp::model::{CallToolRequest, CallToolRequestParams, ClientRequest, ErrorCode};
    use rmcp::service::PeerRequestOptions;
    use rmcp::service::ServiceError;
    use wyrd_client::transport::credential::ResolvedCredential;
    use wyrd_testing::WyrdTestServer;
    use wyrd_testing::bifrost::seed_query_fixture;
    use wyrd_testing::server::BifrostQueryResourceSnapshot;

    /// Build one `bifrost.query` call from its closed arguments.
    ///
    /// # Panics
    ///
    /// Panics when `arguments` is not a JSON object, which every caller passes.
    fn query(arguments: serde_json::Value) -> CallToolRequestParams {
        CallToolRequestParams::new("bifrost.query").with_arguments(
            arguments
                .as_object()
                .expect("query arguments are a JSON object")
                .clone(),
        )
    }

    /// Every way an agent can write an unusable query fails before rows run,
    /// and says enough to repair itself.
    ///
    /// Local argument errors use MCP invalid params; authenticated SQL and
    /// planning refusals preserve the canonical Wyrd problem with no rows.
    ///
    /// # Errors
    ///
    /// Returns fixture, transport, or shutdown failures.
    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "requires the Postgres-backed Bifrost journey lane"]
    async fn bifrost_query_errors_are_actionable_before_row_execution()
    -> Result<(), McpJourneyError> {
        let server = WyrdTestServer::start_bound().await?;
        let fixture = seed_query_fixture(&server, "mcp-query-errors").await?;
        let table = &fixture.table;
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

        for (case, arguments) in [
            (
                "unknown key",
                serde_json::json!({"sql": "SELECT 1", "path": "analytical"}),
            ),
            ("missing sql", serde_json::json!({})),
            ("wrong type", serde_json::json!({"sql": 1})),
            ("blank sql", serde_json::json!({"sql": "   "})),
            (
                "oversized sql",
                serde_json::json!({"sql": "a".repeat(65_537)}),
            ),
            (
                "zero deadline",
                serde_json::json!({"sql": "SELECT 1", "deadline_ms": 0}),
            ),
            (
                "oversized deadline",
                serde_json::json!({"sql": "SELECT 1", "deadline_ms": 4_294_967_296_u64}),
            ),
            (
                "zero rows",
                serde_json::json!({"sql": "SELECT 1", "max_rows": 0}),
            ),
            (
                "oversized rows",
                serde_json::json!({"sql": "SELECT 1", "max_rows": 10_001}),
            ),
            (
                "zero bytes",
                serde_json::json!({"sql": "SELECT 1", "max_bytes": 0}),
            ),
            (
                "oversized bytes",
                serde_json::json!({"sql": "SELECT 1", "max_bytes": 16_777_217}),
            ),
        ] {
            let error = client.call_tool(query(arguments)).await.expect_err(case);
            assert!(
                matches!(error, ServiceError::McpError(ref error) if error.code == ErrorCode::INVALID_PARAMS),
                "{case}: {error}"
            );
        }
        for (tool, arguments) in [
            (
                "bifrost.list_tables",
                serde_json::json!({"unexpected": true}),
            ),
            (
                "bifrost.describe_table",
                serde_json::json!({"namespace": "vala.bifrost"}),
            ),
        ] {
            let error = client
                .call_tool(
                    CallToolRequestParams::new(tool).with_arguments(
                        arguments
                            .as_object()
                            .expect("fixture arguments are objects")
                            .clone(),
                    ),
                )
                .await
                .expect_err(tool);
            assert!(
                matches!(error, ServiceError::McpError(ref error) if error.code == ErrorCode::INVALID_PARAMS),
                "{tool}: {error}"
            );
        }

        let cases = [
            (
                "malformed sql",
                serde_json::json!({"sql": "not valid sql at all"}),
                "WYRD_VALA_400_QUERY_INVALID_SQL",
            ),
            (
                "multiple statements",
                serde_json::json!({"sql": format!("SELECT id FROM {table}; SELECT 1")}),
                "WYRD_VALA_400_QUERY_INVALID_SQL",
            ),
            (
                "non-SELECT sql",
                serde_json::json!({"sql": format!("DELETE FROM {table}")}),
                "WYRD_VALA_400_QUERY_INVALID_SQL",
            ),
            (
                "unknown table",
                serde_json::json!({"sql": "SELECT id FROM vala.bifrost.absent_table"}),
                "WYRD_VALA_404_BIFROST_TABLE_NOT_FOUND",
            ),
            (
                "unknown field",
                serde_json::json!({"sql": format!("SELECT no_such_column FROM {table}")}),
                "WYRD_VALA_400_QUERY_INVALID_SQL",
            ),
            (
                "unsupported shape",
                serde_json::json!({"sql": format!("SELECT evil_udf(value) FROM {table}")}),
                "WYRD_VALA_400_QUERY_INVALID_SQL",
            ),
        ];
        for (case, arguments, code) in cases {
            let refusal = problem(client.call_tool(query(arguments)).await?)?;
            assert_eq!(
                refusal["code"],
                serde_json::json!(code),
                "{case}: {refusal}"
            );
            if matches!(case, "unknown field" | "unsupported shape") {
                assert_eq!(
                    refusal["detail"],
                    "invalid or unsupported query SQL: Inspect bifrost.describe_table and submit a supported SELECT query."
                );
            }
            for field in [
                "code",
                "status",
                "title",
                "detail",
                "remediation",
                "details",
            ] {
                assert!(
                    !refusal[field].is_null(),
                    "{case} must carry a repairable `{field}`: {refusal}"
                );
            }
            assert!(
                refusal.get("rows").is_none() && refusal.get("columns").is_none(),
                "{case} must carry no result payload: {refusal}"
            );
        }

        client.cancel().await?;
        server.shutdown().await?;
        Ok(())
    }

    /// A settled Oracle query owns none of the resources it borrowed.
    const RELEASED: BifrostQueryResourceSnapshot = BifrostQueryResourceSnapshot {
        admission_slots: 0,
        memory_bytes: 0,
        peer_slots: 0,
        tail_fences: 0,
    };

    /// An agent debugging a trace reads it once, completely, and can call it off.
    ///
    /// This is the whole interactive contract in one path: the agent discovers
    /// the tool, describes the table it will read, gets exactly one settled
    /// result with its columns projected once and its rows positional, and can
    /// abandon a running read without leaving Oracle holding admission, memory,
    /// peer work, or a tail fence. The two ceilings are proven on the same
    /// server because an agent that cannot bound its own read is one bad
    /// `SELECT` away from the failure this tool exists to prevent — and a
    /// bounded refusal must never arrive as a shorter success.
    ///
    /// # Errors
    ///
    /// Returns setup, ingestion, MCP transport, query, or settlement failures.
    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "requires the Postgres-backed Bifrost journey lane"]
    async fn agent_debugs_otel_error_trace_through_mcp() -> Result<(), McpJourneyError> {
        let server = WyrdTestServer::start_bound().await?;
        let bootstrap = server
            .bootstrap_service("mcp-query-trace", &["admin"])
            .await?;
        let token = server
            .exchange_api_key(
                bootstrap
                    .api_key()
                    .ok_or("trace journey requires a service key")?,
            )
            .await?;
        let now = chrono::Utc::now();
        let start = now
            .timestamp_nanos_opt()
            .ok_or("current time fits nanoseconds")?;
        let spans: Vec<_> = [
            ("0011223344556677", "healthy-span", 1),
            ("8899aabbccddeeff", "error-span", 2),
        ]
        .into_iter()
        .map(|(span_id, name, status)| {
            serde_json::json!({
                "traceId": "00112233445566778899aabbccddeeff",
                "spanId": span_id,
                "name": name,
                "kind": 2,
                "startTimeUnixNano": start.to_string(),
                "endTimeUnixNano": (start + 1_000_000).to_string(),
                "status": {"code": status},
            })
        })
        .collect();
        let response = reqwest::Client::new()
            .post(format!("{}/v1/traces", server.base_url().ok_or("server is bound")?))
            .header("x-wyrd-access-token", format!("Bearer {token}"))
            .json(&serde_json::json!({"resourceSpans": [{
                "resource": {"attributes": [{"key": "service.name", "value": {"stringValue": "mcp-query-trace"}}]},
                "scopeSpans": [{"spans": spans}],
            }]}))
            .send().await?;
        let status = response.status();
        let body = response.text().await?;
        assert!(
            status.is_success(),
            "OTLP ingestion failed ({status}): {body}"
        );
        server.flush_bifrost().await?;
        let table = "vala.traces.spans";
        let client = ()
            .serve_with_lifecycle(
                transport(&server, ResolvedCredential::BearerToken(token.into()), None)?,
                discover(),
            )
            .await?;

        let listed = structured(
            client
                .call_tool(CallToolRequestParams::new("bifrost.list_tables"))
                .await?,
        )?;
        assert!(
            listed["tables"]
                .as_array()
                .ok_or("tables is an array")?
                .iter()
                .any(|table| table["namespace"] == "vala.traces" && table["name"] == "spans"),
            "the agent discovers actual OTEL spans: {listed}"
        );

        let name = "spans";
        assert!(
            client
                .list_all_tools()
                .await?
                .iter()
                .any(|tool| tool.name == "bifrost.query"),
            "the agent discovers the query tool before using it"
        );
        let described = structured(
            client
                .call_tool(
                    CallToolRequestParams::new("bifrost.describe_table").with_arguments(
                        serde_json::json!({"namespace": "vala.traces", "name": name})
                            .as_object()
                            .ok_or("describe arguments are an object")?
                            .clone(),
                    ),
                )
                .await?,
        )?;
        assert_eq!(described["entry"]["name"], serde_json::json!(name));

        let fields = described["fields"]
            .as_array()
            .ok_or("describe returns fields")?;
        for name in ["name", "status"] {
            assert!(
                fields.iter().any(|field| field["name"] == name),
                "trace field {name} is discoverable: {described}"
            );
        }

        // One bounded read-only trace query returns one complete result.
        let lower = (now - chrono::Duration::minutes(1)).to_rfc3339();
        let upper = (now + chrono::Duration::minutes(1)).to_rfc3339();
        let sql = format!(
            "SELECT name, status FROM {table} WHERE status = 'ERROR' AND start_time >= TIMESTAMP '{lower}' AND start_time < TIMESTAMP '{upper}'"
        );
        let result = client
            .call_tool(query(serde_json::json!({"sql": sql, "max_rows": 10})))
            .await?;
        assert_ne!(
            result.is_error,
            Some(true),
            "a bounded trace read succeeds: {:?}",
            result.structured_content
        );
        let content = structured(result)?;
        assert_eq!(
            content["columns"],
            serde_json::json!([
                {"name": "name", "data_type": "Utf8", "nullable": false},
                {"name": "status", "data_type": "Utf8", "nullable": false},
            ]),
            "columns are projected exactly once, in schema order"
        );
        assert_eq!(
            content["rows"],
            serde_json::json!([["error-span", "ERROR"]]),
            "rows are positional arrays carrying the selected trace"
        );
        assert_eq!(
            content["terminal"]["execution_path"],
            serde_json::json!("interactive"),
            "Oracle owns path selection and chose Interactive: {content}"
        );
        assert_eq!(content["terminal"]["outcome"], serde_json::json!("success"));
        assert_eq!(
            content["terminal"]["freshness"],
            serde_json::json!("complete")
        );
        assert_eq!(content["terminal"]["row_count"], serde_json::json!(1));

        // Both ceilings refuse rather than truncate.
        for (case, arguments) in [
            (
                "row ceiling",
                serde_json::json!({"sql": format!("SELECT name, status FROM {table}"), "max_rows": 1}),
            ),
            (
                "byte ceiling",
                serde_json::json!({"sql": format!("SELECT name, status FROM {table}"), "max_bytes": 1}),
            ),
        ] {
            let refusal = problem(client.call_tool(query(arguments)).await?)?;
            assert_eq!(
                refusal["code"],
                serde_json::json!("WYRD_VALA_413_QUERY_RESULT_TOO_LARGE"),
                "{case}: {refusal}"
            );
            assert!(
                refusal.get("rows").is_none() && refusal.get("columns").is_none(),
                "{case} returns no truncated result: {refusal}"
            );
        }

        // An abandoned read settles in Oracle before the tool returns.
        let observer = vala_bifrost_redux::oracle::query_lifecycle_observer_for_test();
        let settled_target = observer.completed().saturating_add(1);
        server.stall_next_query_after_schema();
        let handle = client
            .send_cancellable_request(
                ClientRequest::CallToolRequest(CallToolRequest::new(query(
                    serde_json::json!({"sql": format!("SELECT name, status FROM {table}")}),
                ))),
                PeerRequestOptions::no_options(),
            )
            .await?;
        let query_id = server.wait_query_schema_stall().await?;
        handle.cancel(None).await?;
        tokio::time::timeout(
            Duration::from_secs(5),
            observer.wait_for_at_least(settled_target),
        )
        .await
        .map_err(|_| "the abandoned query never recorded a settled lifecycle")?;
        assert_eq!(
            server
                .wait_bifrost_query_resources_released(&query_id, RELEASED)
                .await?,
            RELEASED,
            "a cancelled query returns every admission, byte, peer slot, and tail fence"
        );

        client.cancel().await?;
        server.shutdown().await?;
        Ok(())
    }
}
