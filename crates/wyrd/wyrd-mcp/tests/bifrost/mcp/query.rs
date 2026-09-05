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
    use rmcp::model::{CallToolRequest, CallToolRequestParams, ClientRequest};
    use rmcp::service::PeerRequestOptions;
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
    /// The cases span all three owning boundaries — the MCP input bounds, the
    /// server's SQL floor, and Oracle's own planning — because an agent cannot
    /// tell them apart and should not have to: each returns the same canonical
    /// Wyrd problem shape with no partial result attached.
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
            (
                "row ceiling above the MCP floor",
                serde_json::json!({
                    "sql": format!("SELECT id FROM {table}"),
                    "max_rows": 10_001,
                }),
                "WYRD_SPEC_400_VALIDATION",
            ),
            (
                "byte ceiling above the MCP floor",
                serde_json::json!({
                    "sql": format!("SELECT id FROM {table}"),
                    "max_bytes": 16_777_217_u64,
                }),
                "WYRD_SPEC_400_VALIDATION",
            ),
            (
                "blank sql",
                serde_json::json!({"sql": "   "}),
                "WYRD_SPEC_400_VALIDATION",
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
    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "requires the Postgres-backed Bifrost journey lane"]
    async fn agent_debugs_otel_error_trace_through_mcp() -> Result<(), McpJourneyError> {
        let server = WyrdTestServer::start_bound().await?;
        let fixture = seed_query_fixture(&server, "mcp-query-trace").await?;
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

        let name = table
            .strip_prefix("vala.bifrost.")
            .ok_or("the fixture table is Bifrost-qualified")?;
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
                        serde_json::json!({"namespace": "vala.bifrost", "name": name})
                            .as_object()
                            .ok_or("describe arguments are an object")?
                            .clone(),
                    ),
                )
                .await?,
        )?;
        assert_eq!(described["entry"]["name"], serde_json::json!(name));

        // One bounded read-only trace query returns one complete result.
        let sql = format!("SELECT id, value FROM {table} WHERE value = 'second'");
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
                {"name": "id", "data_type": "Int64", "nullable": false},
                {"name": "value", "data_type": "Utf8", "nullable": false},
            ]),
            "columns are projected exactly once, in schema order"
        );
        assert_eq!(
            content["rows"],
            serde_json::json!([[2, "second"]]),
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
                serde_json::json!({"sql": format!("SELECT id, value FROM {table}"), "max_rows": 1}),
            ),
            (
                "byte ceiling",
                serde_json::json!({"sql": format!("SELECT id, value FROM {table}"), "max_bytes": 1}),
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
                    serde_json::json!({"sql": format!("SELECT id, value FROM {table}")}),
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
