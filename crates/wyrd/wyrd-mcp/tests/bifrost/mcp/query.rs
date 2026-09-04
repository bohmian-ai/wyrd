//! Tier-1 journeys: an agent reads Bifrost through `bifrost.query`.
//!
//! The query tool is the only Bifrost capability that executes anything, so
//! these journeys own the two halves an agent depends on: a repairable refusal
//! before any row runs, and one complete positional result after Oracle's own
//! terminal settlement. Both are driven by a real `rmcp` client against the
//! real `/mcp` endpoint, never by calling the adapter in process.

use crate::connectivity::{McpJourneyError, discover, transport};

mod pg_tests {
    use super::{McpJourneyError, discover, transport};

    use rmcp::ClientServiceExt as _;
    use rmcp::model::{CallToolRequestParams, CallToolResult};
    use wyrd_client::transport::credential::ResolvedCredential;
    use wyrd_testing::WyrdTestServer;
    use wyrd_testing::bifrost::seed_query_fixture;

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

    /// Read the canonical Wyrd problem from a call that must have failed.
    ///
    /// # Errors
    ///
    /// Returns a description when the call succeeded or carried no problem.
    fn problem(result: CallToolResult) -> Result<serde_json::Value, McpJourneyError> {
        let content = result
            .structured_content
            .ok_or("a failed Bifrost tool returns its structured problem")?;
        if result.is_error != Some(true) {
            return Err(format!("the query unexpectedly succeeded: {content}").into());
        }
        Ok(content)
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
                serde_json::json!({"sql": format!("SELECT id FROM \"{table}\"; SELECT 1")}),
                "WYRD_VALA_400_QUERY_INVALID_SQL",
            ),
            (
                "non-SELECT sql",
                serde_json::json!({"sql": format!("DELETE FROM \"{table}\"")}),
                "WYRD_VALA_400_QUERY_INVALID_SQL",
            ),
            (
                "unknown table",
                serde_json::json!({"sql": "SELECT id FROM \"vala.bifrost.absent_table\""}),
                "WYRD_VALA_404_BIFROST_TABLE_NOT_FOUND",
            ),
            // An unresolvable column or function passes the SQL floor and dies
            // in Oracle's own planner, which scrubs the DataFusion message
            // rather than leak schema shape. The agent still gets the complete
            // canonical problem and no partial rows, but the code is Oracle's
            // opaque execution failure, not a repairable 400.
            (
                "unknown field",
                serde_json::json!({"sql": format!("SELECT no_such_column FROM \"{table}\"")}),
                "WYRD_VALA_500_QUERY_EXECUTION_FAILED",
            ),
            (
                "unsupported shape",
                serde_json::json!({"sql": format!("SELECT evil_udf(value) FROM \"{table}\"")}),
                "WYRD_VALA_500_QUERY_EXECUTION_FAILED",
            ),
            (
                "row ceiling above the MCP floor",
                serde_json::json!({
                    "sql": format!("SELECT id FROM \"{table}\""),
                    "max_rows": 10_001,
                }),
                "WYRD_SPEC_400_VALIDATION",
            ),
            (
                "byte ceiling above the MCP floor",
                serde_json::json!({
                    "sql": format!("SELECT id FROM \"{table}\""),
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
}
