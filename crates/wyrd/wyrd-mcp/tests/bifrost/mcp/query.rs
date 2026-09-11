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
    use wyrd_spec::request_id::RequestId;
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

    /// An agent acting for another agent is attributed as such, all the way to
    /// the durable audit record of the read it performed.
    ///
    /// This is the whole delegation contract in one path: a real two-hop
    /// delegated token reaches a real Bifrost tool, authorization stays bound
    /// to the effective principal the token names, and the read-decision row
    /// Oracle commits before any row is disclosed carries the exact
    /// initiator-first chain — the identity, kind, and card authority of each
    /// delegator — under the caller's own request id and tenant. The
    /// nondelegated caller in the same tenant proves the negative half: its row
    /// carries no chain and keeps the encoding it always had.
    ///
    /// # Errors
    ///
    /// Returns fixture, delegation, transport, audit, or shutdown failures.
    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "requires the Postgres-backed Bifrost journey lane"]
    async fn delegated_agent_query_is_attributed_in_its_durable_audit_record()
    -> Result<(), McpJourneyError> {
        let server = WyrdTestServer::start_bound().await?;
        let tenant = server.data_tenant_id();
        let fixture = seed_query_fixture(&server, "mcp-delegation").await?;
        let sql = format!("SELECT id, value FROM {} ORDER BY id", fixture.table);

        // A acts through B, which acts as C. Only C's own roles authorize the
        // read; A and B are attribution.
        let initiator = server
            .bootstrap_service("mcp-delegation-initiator", &["runtime_admin"])
            .await?;
        let middle = server
            .bootstrap_service("mcp-delegation-middle", &["runtime_admin"])
            .await?;
        let subject = server
            .bootstrap_service("mcp-delegation-subject", &["admin"])
            .await?;
        let initiator_jwt = server
            .exchange_api_key(initiator.api_key().ok_or("a service carries a key")?)
            .await?;
        let one_hop = server.delegate(
            &initiator_jwt,
            middle.card_ref().ok_or("a service carries a card ref")?,
        );
        let one_hop = one_hop.await?;
        let two_hop = server
            .delegate(
                &one_hop,
                subject.card_ref().ok_or("a service carries a card ref")?,
            )
            .await?;

        let request_id = RequestId::now_v7();
        let client = ()
            .serve_with_lifecycle(
                transport(
                    &server,
                    ResolvedCredential::BearerToken(two_hop.into()),
                    Some(&request_id),
                )?,
                discover(),
            )
            .await?;
        let result = client
            .call_tool(query(serde_json::json!({"sql": sql, "max_rows": 10})))
            .await?;
        assert_ne!(
            result.is_error,
            Some(true),
            "the delegated agent reads: {:?}",
            result.structured_content
        );
        client.cancel().await?;

        let detail = read_decision_detail(&server, tenant, request_id.as_str()).await?;
        let chain = detail["delegation_chain"]
            .as_array()
            .ok_or("the read decision records a delegation chain")?;
        assert_eq!(
            chain
                .iter()
                .map(|step| step["principal_id"].as_str().unwrap_or_default())
                .collect::<Vec<_>>(),
            vec![
                initiator.id().to_string().as_str(),
                middle.id().to_string().as_str(),
            ],
            "the chain is initiator-first and complete: {detail}"
        );
        for (step, service) in chain.iter().zip([&initiator, &middle]) {
            assert_eq!(step["principal_kind"], serde_json::json!("service"));
            let card_ref = service.card_ref().ok_or("a service carries a card ref")?;
            assert_eq!(
                step["card_ref"]["name"],
                serde_json::json!(card_ref.name.as_str()),
                "each step names the card authority it acted under: {step}"
            );
            assert!(
                step["card_ref_scope"]
                    .as_array()
                    .is_some_and(|scope| !scope.is_empty()),
                "each step carries its verified card scope: {step}"
            );
        }
        assert!(
            !detail.to_string().contains(&fixture.token),
            "no credential material reaches the audit record"
        );

        // Same tenant, same tool, no delegation: an unchanged record shape.
        let plain_request_id = RequestId::now_v7();
        let plain = ()
            .serve_with_lifecycle(
                transport(
                    &server,
                    ResolvedCredential::BearerToken(fixture.token.clone().into()),
                    Some(&plain_request_id),
                )?,
                discover(),
            )
            .await?;
        let plain_result = plain
            .call_tool(query(serde_json::json!({"sql": sql, "max_rows": 10})))
            .await?;
        assert_ne!(plain_result.is_error, Some(true));
        plain.cancel().await?;
        let plain_detail = read_decision_detail(&server, tenant, plain_request_id.as_str()).await?;
        assert!(
            plain_detail.get("delegation_chain").is_none(),
            "a nondelegated caller keeps the original record shape: {plain_detail}"
        );

        // A delegated token that lacks the permission is refused by the same
        // audited RBAC path, and the refusal still says who was acting for whom.
        let underprivileged = server
            .bootstrap_service("mcp-delegation-reader", &["reader"])
            .await?;
        let denied_token = server
            .delegate(
                &initiator_jwt,
                underprivileged
                    .card_ref()
                    .ok_or("a service carries a card ref")?,
            )
            .await?;
        let denied_request_id = RequestId::now_v7();
        let reads_before = server
            .bifrost_read_decision_count_for_tenant(tenant)
            .await?;
        let denied_client = ()
            .serve_with_lifecycle(
                transport(
                    &server,
                    ResolvedCredential::BearerToken(denied_token.into()),
                    Some(&denied_request_id),
                )?,
                discover(),
            )
            .await?;
        let refusal = problem(
            denied_client
                .call_tool(query(serde_json::json!({"sql": sql, "max_rows": 10})))
                .await?,
        )?;
        assert_eq!(
            refusal["code"],
            serde_json::json!("WYRD_PERMISSION_403_DENIED_RBAC"),
            "the under-privileged delegate is refused: {refusal}"
        );
        assert!(
            refusal.get("rows").is_none(),
            "a refused delegate sees no rows: {refusal}"
        );
        denied_client.cancel().await?;
        assert_eq!(
            server
                .bifrost_read_decision_count_for_tenant(tenant)
                .await?,
            reads_before,
            "a refused delegate produces no read-decision acceptance"
        );

        let mut conn = server.tenant_conn_for(tenant).await?;
        let denial: Vec<(uuid::Uuid, String, Option<String>)> = sqlx::query_as(
            "SELECT principal_id, decision, detail FROM vala.audit_staging \
             WHERE operation = 'vala.query.sync' AND request_id = $1",
        )
        .bind(denied_request_id.as_str())
        .fetch_all(&mut **conn.transaction())
        .await?;
        conn.commit().await?;
        assert_eq!(denial.len(), 1, "the denial is audited exactly once");
        assert_eq!(denial[0].0, underprivileged.id().as_uuid());
        assert_eq!(denial[0].1, "deny");
        let denial_detail: serde_json::Value = serde_json::from_str(
            denial[0]
                .2
                .as_deref()
                .ok_or("the denial retains delegation attribution")?,
        )?;
        assert_eq!(
            denial_detail["kind"],
            serde_json::json!("delegation_attribution")
        );
        assert_eq!(
            denial_detail["delegation_chain"]
                .as_array()
                .ok_or("the denial names its delegators")?
                .iter()
                .map(|step| step["principal_id"].as_str().unwrap_or_default())
                .collect::<Vec<_>>(),
            vec![initiator.id().to_string().as_str()],
            "the denial keeps the verified chain: {denial_detail}"
        );

        server.shutdown().await?;
        Ok(())
    }

    /// Read the exact committed read-decision detail for one request id.
    async fn read_decision_detail(
        server: &WyrdTestServer,
        tenant: wyrd_spec::DataTenantId,
        request_id: &str,
    ) -> Result<serde_json::Value, McpJourneyError> {
        let mut conn = server.tenant_conn_for(tenant).await?;
        let detail: String = sqlx::query_scalar(
            "SELECT detail FROM vala.audit_staging \
             WHERE operation = 'bifrost.query.read_decision' AND request_id = $1 \
             ORDER BY seq DESC LIMIT 1",
        )
        .bind(request_id)
        .fetch_one(&mut **conn.transaction())
        .await?;
        conn.commit().await?;
        Ok(serde_json::from_str(&detail)?)
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

        let fields = described["user_fields"]
            .as_array()
            .ok_or("describe returns user fields")?;
        for name in ["name", "status_code"] {
            assert!(
                fields.iter().any(|field| field["name"] == name),
                "trace field {name} is discoverable: {described}"
            );
        }

        // One bounded read-only trace query returns one complete result. The
        // canonical span ledger stores the OTLP status as its numeric
        // `status_code` and the start instant as `start_time_unix_nano`, so the
        // window is expressed in nanoseconds rather than as a timestamp.
        let minute_nanos = 60_000_000_000_i64;
        let lower = start - minute_nanos;
        let upper = start + minute_nanos;
        let sql = format!(
            "SELECT name, status_code FROM {table} WHERE status_code = 2 AND start_time_unix_nano >= {lower} AND start_time_unix_nano < {upper}"
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
                {"name": "status_code", "data_type": "Int32", "nullable": true},
            ]),
            "columns are projected exactly once, in schema order"
        );
        assert_eq!(
            content["rows"],
            serde_json::json!([["error-span", 2]]),
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
                serde_json::json!({"sql": format!("SELECT name, status_code FROM {table}"), "max_rows": 1}),
            ),
            (
                "byte ceiling",
                serde_json::json!({"sql": format!("SELECT name, status_code FROM {table}"), "max_bytes": 1}),
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
                    serde_json::json!({"sql": format!("SELECT name, status_code FROM {table}")}),
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

    /// An agent answers a complete GenAI incident question over the three
    /// canonical signal ledgers.
    ///
    /// This is the agent-surface half of the canonical journey: nothing is
    /// seeded through a private seam, the dataset arrives through the public
    /// Arrow batch door, and every answer is read back through `bifrost.query`
    /// SQL — the trace hierarchy, the promoted GenAI token aggregate, the log
    /// correlated to the span that failed, and one metric aggregate.
    ///
    /// # Errors
    ///
    /// Returns fixture, transport, tool-call, or shutdown failures.
    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "requires the Postgres-backed Bifrost journey lane"]
    async fn agent_reads_canonical_trace_genai_logs_and_metrics_through_sql()
    -> Result<(), McpJourneyError> {
        use wyrd_testing::bifrost::canonical_signals as fixture;

        let server = WyrdTestServer::start_bound().await?;
        let seeded = fixture::seed_canonical_signals(&server, "mcp-canonical-signals").await?;
        let scope = seeded.scope.as_str();
        let client = ()
            .serve_with_lifecycle(
                transport(
                    &server,
                    ResolvedCredential::BearerToken(seeded.token.clone().into()),
                    None,
                )?,
                discover(),
            )
            .await?;

        let hierarchy = structured(
            client
                .call_tool(query(serde_json::json!({
                    "sql": format!(
                        "SELECT name, gen_ai_operation_name, status_code, \
                         CAST(CASE WHEN parent_span_id IS NULL THEN 1 ELSE 0 END AS BIGINT) \
                         AS is_root FROM vala.traces.spans WHERE scope_name = '{scope}' \
                         ORDER BY start_time_unix_nano"
                    ),
                    "max_rows": 10,
                })))
                .await?,
        )?;
        assert_eq!(
            hierarchy["rows"],
            serde_json::json!([
                ["chat claude-opus-5", "chat", 1, 1],
                ["execute_tool search", "execute_tool", 2, 0],
            ]),
            "the agent reads the parent GenAI span and the tool span it made"
        );

        let tokens = structured(
            client
                .call_tool(query(serde_json::json!({
                    "sql": format!(
                        "SELECT CAST(SUM(gen_ai_usage_input_tokens) AS BIGINT) AS input_tokens, \
                         CAST(SUM(gen_ai_usage_output_tokens) AS BIGINT) AS output_tokens, \
                         CAST(COUNT(*) AS BIGINT) AS spans FROM vala.traces.spans \
                         WHERE scope_name = '{scope}' AND gen_ai_request_model = '{model}'",
                        model = fixture::MODEL,
                    ),
                    "max_rows": 10,
                })))
                .await?,
        )?;
        assert_eq!(
            tokens["rows"],
            serde_json::json!([[fixture::INPUT_TOKENS + 64, fixture::OUTPUT_TOKENS + 16, 2]]),
            "promoted GenAI token columns aggregate for one model"
        );

        let correlated = structured(
            client
                .call_tool(query(serde_json::json!({
                    "sql": format!(
                        "SELECT l.severity_text, l.event_name, s.name AS span_name \
                         FROM vala.logs.records l JOIN vala.traces.spans s \
                         ON l.trace_id = s.trace_id AND l.span_id = s.span_id \
                         WHERE l.scope_name = '{scope}'"
                    ),
                    "max_rows": 10,
                })))
                .await?,
        )?;
        assert_eq!(
            correlated["rows"],
            serde_json::json!([["ERROR", "tool.retry.exhausted", "execute_tool search"]]),
            "the error log correlates to the span that failed"
        );

        let metrics = structured(
            client
                .call_tool(query(serde_json::json!({
                    "sql": format!(
                        "SELECT metric_type, \
                         CAST(SUM(COALESCE(int_value, 0)) AS BIGINT) AS ints, \
                         CAST(SUM(COALESCE(histogram_count, 0)) AS BIGINT) AS observations \
                         FROM vala.metrics.points WHERE scope_name = '{scope}' \
                         GROUP BY metric_type ORDER BY metric_type"
                    ),
                    "max_rows": 10,
                })))
                .await?,
        )?;
        assert_eq!(
            metrics["rows"],
            serde_json::json!([
                ["gauge", 0, 0],
                ["histogram", 0, fixture::HISTOGRAM_COUNT],
                ["sum", fixture::COUNTER_VALUE, 0],
            ]),
            "each metric kind aggregates over its own value column"
        );
        assert_eq!(metrics["terminal"]["outcome"], serde_json::json!("success"));

        // GenAI messages are ordinary trace-span columns governed by table RBAC.
        let payload_sql = format!(
            "SELECT attributes FROM vala.traces.spans \
             WHERE scope_name = '{scope}' AND parent_span_id IS NULL"
        );
        let reader = agent_with_permissions(
            &server,
            "mcp_canonical_reader",
            &[wyrd_runtime::Permission::bifrost_query_read()],
        )
        .await?;
        let messages = structured(
            reader
                .call_tool(query(serde_json::json!({"sql": payload_sql})))
                .await?,
        )?;
        // The tool renders a binary column as hex, so the expected structured
        // messages are compared in the same encoding the agent receives.
        let payload = messages["rows"][0][0]
            .as_str()
            .ok_or("the attribute payload is a JSON string")?;
        let hex_of = |text: &str| {
            text.bytes()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>()
        };
        assert!(
            payload.contains(&hex_of(fixture::INPUT_MESSAGES))
                && payload.contains(&hex_of(fixture::OUTPUT_MESSAGES)),
            "an authorized agent reads the structured GenAI messages: {payload}"
        );
        reader.cancel().await?;

        client.cancel().await?;
        server.shutdown().await?;
        Ok(())
    }

    /// Connect one MCP client whose principal holds exactly `permissions`.
    ///
    /// # Errors
    ///
    /// Returns role-seeding, bootstrap, token-exchange, or transport failures.
    async fn agent_with_permissions(
        server: &WyrdTestServer,
        role: &str,
        permissions: &[wyrd_runtime::Permission],
    ) -> Result<rmcp::service::RunningService<rmcp::service::RoleClient, ()>, McpJourneyError> {
        server.seed_role(role, permissions).await?;
        let bootstrap = server.bootstrap_service(role, &[role]).await?;
        let token = server
            .exchange_api_key(bootstrap.api_key().ok_or("a service carries a key")?)
            .await?;
        Ok(()
            .serve_with_lifecycle(
                transport(server, ResolvedCredential::BearerToken(token.into()), None)?,
                discover(),
            )
            .await?)
    }
}
