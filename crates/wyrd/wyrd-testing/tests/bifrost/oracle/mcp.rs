//! Tier-1 journey: an agent runs a distributed analytical query over MCP.
//!
//! Everything else in this binary drives Oracle through the typed public
//! surfaces. This module drives it the way an agent actually does — an `rmcp`
//! client against one pod's real `/mcp` endpoint on a live four-pod peer
//! topology — because that is the only path where discovery, the closed input
//! bounds, the single settled result, and Oracle's own path selection have to
//! agree at once. The MCP adapter contributes no plan hint, so an Analytical
//! result here is Oracle's decision reaching an agent unchanged.

use rmcp::model::{CallToolRequest, CallToolRequestParams, CallToolResult, ClientRequest};
use rmcp::service::PeerRequestOptions;
use rmcp::transport::StreamableHttpClientTransport;
use rmcp::transport::streamable_http_client::StreamableHttpClientTransportConfig;
use wyrd_server::config::BifrostTarget;
use wyrd_testing::WyrdTestServer;

use crate::peer_cluster::PeerCluster;
use crate::support::{JourneyError, await_baseline};

/// Pod index that plans, admits, and coordinates every query.
const COORDINATOR: usize = 0;

/// Pod indices that can only be reached as remote execution followers.
const PEER_FOLLOWERS: [usize; 2] = [1, 2];

/// Pod index that owns ingest and publication for the fixture tables.
const PEER_SCRIBE: usize = 3;

/// Rows written into each fixture table.
const FIXTURE_ROWS: i64 = 12;

/// Distinct `filter_key` groups the fixture rows fall into.
const FIXTURE_GROUPS: i64 = 3;

/// Exchange one provisioned API key for the bearer `/mcp` accepts.
///
/// The exchange runs over the pod's real public listener rather than in
/// process, so the credential an agent presents to MCP is one the deployment
/// actually issued.
///
/// # Errors
///
/// Returns the transport error, or a description when the pod serves no
/// public HTTP listener, the auth route refuses the key, or it answers with a
/// body that is not a token response.
async fn bearer(
    node: &WyrdTestServer,
    api_key: &secrecy::SecretString,
) -> Result<String, JourneyError> {
    use secrecy::ExposeSecret as _;

    let base_url = node
        .base_url()
        .ok_or("the pod serves no public HTTP listener")?;
    let response = reqwest::Client::new()
        .post(format!("{base_url}/auth/token"))
        .json(&wyrd_spec::auth::TokenRequest::WyrdApiKey {
            api_key: wyrd_spec::auth::SecretBearer::new(api_key.expose_secret().to_owned()),
        })
        .send()
        .await?;
    let status = response.status();
    let body = response.text().await?;
    if !status.is_success() {
        return Err(format!("token exchange failed with {status}: {body}").into());
    }
    let token: wyrd_spec::auth::TokenResponse = serde_json::from_str(&body)?;
    Ok(token.access_token.expose().to_owned())
}

/// Build one MCP client transport against a pod's `/mcp` endpoint.
///
/// Wyrd's public edge reads the caller's JWT from `x-wyrd-access-token`, not
/// from `Authorization`, so the journey attaches it the way every other Wyrd
/// client does.
///
/// # Errors
///
/// Returns an error when the pod serves no public HTTP listener or the bearer
/// cannot be encoded as a header value.
fn transport(
    node: &WyrdTestServer,
    bearer: &str,
) -> Result<StreamableHttpClientTransport<reqwest::Client>, JourneyError> {
    let base_url = node
        .base_url()
        .ok_or("the pod serves no public HTTP listener")?;
    let mut config = StreamableHttpClientTransportConfig::with_uri(format!("{base_url}/mcp"));
    config.allow_stateless = true;
    config.custom_headers.insert(
        http::HeaderName::from_static("x-wyrd-access-token"),
        http::HeaderValue::from_str(&format!("Bearer {bearer}"))?,
    );
    Ok(StreamableHttpClientTransport::with_client(
        reqwest::Client::new(),
        config,
    ))
}

/// The session-free lifecycle Wyrd's `/mcp` endpoint speaks.
fn discover() -> rmcp::ClientLifecycleMode {
    rmcp::ClientLifecycleMode::Discover {
        preferred_versions: vec![rmcp::model::ProtocolVersion::V_2026_07_28],
    }
}

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

/// Read one tool call's structured content, failing on a tool-level error.
///
/// # Errors
///
/// Returns the structured Wyrd problem when the call reported `is_error`, and
/// a description when it carried no structured content at all.
fn structured(result: CallToolResult) -> Result<serde_json::Value, JourneyError> {
    let content = result
        .structured_content
        .ok_or("a Wyrd MCP tool returns structured content")?;
    if result.is_error == Some(true) {
        return Err(format!("tool call failed: {content}").into());
    }
    Ok(content)
}

/// Read the canonical Wyrd problem from a call that must have failed.
///
/// # Errors
///
/// Returns a description when the call succeeded or carried no problem.
fn problem(result: CallToolResult) -> Result<serde_json::Value, JourneyError> {
    let content = result
        .structured_content
        .ok_or("a failed Wyrd MCP tool returns its structured problem")?;
    if result.is_error != Some(true) {
        return Err(format!("tool call unexpectedly succeeded: {content}").into());
    }
    Ok(content)
}

mod pg_tests {
    use rmcp::ClientServiceExt as _;

    use super::{
        BifrostTarget, COORDINATOR, CallToolRequest, CallToolRequestParams, ClientRequest,
        FIXTURE_GROUPS, FIXTURE_ROWS, JourneyError, PEER_FOLLOWERS, PEER_SCRIBE, PeerCluster,
        PeerRequestOptions, await_baseline, bearer, discover, problem, query, structured,
        transport,
    };

    /// An agent joins three tables it discovered and gets one Analytical result.
    ///
    /// # Panics
    ///
    /// Panics when the MCP analytical journey cannot be driven to its claims.
    #[tokio::test(flavor = "multi_thread", worker_threads = 8)]
    #[ignore = "requires the serialized Postgres-backed Oracle journey lane"]
    async fn agent_runs_three_table_analytical_query_through_mcp() {
        prove_analytical_mcp_journey()
            .await
            .expect("MCP analytical journey");
    }

    /// Drives discovery, the distributed join, both ceilings, repairs, and
    /// cancellation against one live four-pod peer topology.
    ///
    /// # Errors
    ///
    /// Returns the first claim that broke.
    ///
    /// # Panics
    /// Panics if a ceiling result precedes retained cleanup, ownership is not
    /// charged during the hold, or a refusal includes partial rows.
    async fn prove_analytical_mcp_journey() -> Result<(), JourneyError> {
        let mut cluster = PeerCluster::start(&[
            BifrostTarget::Oracle,
            BifrostTarget::Oracle,
            BifrostTarget::Oracle,
            BifrostTarget::Scribe,
        ])
        .await?;
        let api_key = cluster
            .provision_public_api_key("mcp-analytical-agent")
            .await?;

        let tables: Vec<String> = (0..3)
            .map(|index| format!("mcp_join_{index}_{}", uuid::Uuid::now_v7().simple()))
            .collect();
        for table in &tables {
            cluster.register_table(PEER_SCRIBE, table).await?;
            cluster
                .ingest_rows(PEER_SCRIBE, table, 0, FIXTURE_ROWS, FIXTURE_GROUPS)
                .await?;
        }
        for index in [
            COORDINATOR,
            PEER_FOLLOWERS[0],
            PEER_FOLLOWERS[1],
            PEER_SCRIBE,
        ] {
            cluster.refresh_snapshot(index).await?;
        }

        let baseline: Vec<_> = [COORDINATOR, PEER_FOLLOWERS[0], PEER_FOLLOWERS[1]]
            .into_iter()
            .map(|index| Ok((index, cluster.ownership_snapshot(index)?)))
            .collect::<Result<_, JourneyError>>()?;
        let leases_before: Vec<u64> = PEER_FOLLOWERS
            .iter()
            .map(|index| Ok(cluster.graph_leases(*index)?.0))
            .collect::<Result<_, JourneyError>>()?;
        let polls_before = cluster.peer_body_polls();

        let token = bearer(cluster.server(COORDINATOR)?, &api_key).await?;
        let client =
            ().serve_with_lifecycle(transport(cluster.server(COORDINATOR)?, &token)?, discover())
                .await?;

        // The agent learns the three tables and their columns from the catalog
        // alone: nothing below names a column this discovery did not return.
        let listed = structured(
            client
                .call_tool(CallToolRequestParams::new("bifrost.list_tables"))
                .await?,
        )?;
        let listed_names: Vec<&str> = listed["tables"]
            .as_array()
            .ok_or("list_tables returns a tables array")?
            .iter()
            .filter_map(|table| table["name"].as_str())
            .collect();
        for table in &tables {
            if !listed_names.contains(&table.as_str()) {
                return Err(format!("{table} was not discoverable: {listed_names:?}").into());
            }
        }
        for table in &tables {
            let described = structured(
                client
                    .call_tool(
                        CallToolRequestParams::new("bifrost.describe_table").with_arguments(
                            serde_json::json!({"namespace": "vala.bifrost", "name": table})
                                .as_object()
                                .ok_or("describe arguments are an object")?
                                .clone(),
                        ),
                    )
                    .await?,
            )?;
            let columns: Vec<&str> = described["user_fields"]
                .as_array()
                .ok_or("describe returns the stored user field list")?
                .iter()
                .filter_map(|field| field["name"].as_str())
                .collect();
            if !columns.contains(&"filter_key") || !columns.contains(&"id") {
                return Err(format!("{table} did not describe its join keys: {columns:?}").into());
            }
        }

        // A bounded three-table equi-join with a fixed-width aggregation, and
        // no path selector of any kind.
        let join_sql = format!(
            "SELECT a.filter_key, COUNT(*) AS matched \
             FROM vala.bifrost.{0} a \
             JOIN vala.bifrost.{1} b ON a.filter_key = b.filter_key \
             JOIN vala.bifrost.{2} c ON a.filter_key = c.filter_key \
             GROUP BY a.filter_key ORDER BY a.filter_key",
            tables[0], tables[1], tables[2]
        );
        let joined = structured(
            client
                .call_tool(query(serde_json::json!({"sql": join_sql, "max_rows": 100})))
                .await?,
        )?;
        if joined["terminal"]["query_class"] != serde_json::json!("analytical") {
            return Err(format!("Oracle did not select Analytical: {joined}").into());
        }
        if joined["terminal"]["outcome"] != serde_json::json!("success") {
            return Err(
                format!("the analytical join did not settle successfully: {joined}").into(),
            );
        }
        let rows = joined["rows"]
            .as_array()
            .ok_or("a successful query returns positional rows")?;
        if rows.len() != usize::try_from(FIXTURE_GROUPS)? {
            return Err(format!("expected {FIXTURE_GROUPS} grouped rows, saw {rows:?}").into());
        }
        if joined["terminal"]["row_count"] != serde_json::json!(rows.len()) {
            return Err(format!("the terminal row count disagrees with the rows: {joined}").into());
        }
        let per_group = FIXTURE_ROWS / FIXTURE_GROUPS;
        let expected_matched = per_group * per_group * per_group;
        for row in rows {
            if row[1] != serde_json::json!(expected_matched) {
                return Err(format!("expected {expected_matched} matches per group: {row}").into());
            }
        }

        // Real remote stages ran on both followers; a leader-local rewrite
        // would have leased no follower graph and polled no peer body.
        for (offset, index) in PEER_FOLLOWERS.into_iter().enumerate() {
            let activated = cluster.graph_leases(index)?.0;
            if activated <= leases_before[offset] {
                return Err(format!(
                    "follower {index} leased no graph: {activated} activations, was {}",
                    leases_before[offset]
                )
                .into());
            }
        }
        let polls = cluster.peer_body_polls();
        if polls <= polls_before {
            return Err(
                format!("no peer body was polled: {polls} polls, was {polls_before}").into(),
            );
        }

        // A ceiling error remains pending through real cleanup, including a
        // Scribe ingress whose cancellation must reach its remote Oracle.
        for endpoint in [COORDINATOR, PEER_SCRIBE] {
            for (index, before) in &baseline {
                await_baseline(&cluster, *index, *before).await?;
            }
            let leader = if endpoint == COORDINATOR {
                COORDINATOR
            } else {
                baseline
                    .iter()
                    .map(|(index, _)| *index)
                    .min_by_key(|index| cluster.node_id(*index).as_uuid())
                    .ok_or("no Oracle candidate")?
            };
            let token = bearer(cluster.server(endpoint)?, &api_key).await?;
            let ingress =
                ().serve_with_lifecycle(transport(cluster.server(endpoint)?, &token)?, discover())
                    .await?;
            cluster.arm_cleanup_pause();
            let peer = ingress.peer().clone();
            let arguments =
                query(serde_json::json!({"sql": join_sql, "max_rows": 1, "deadline_ms": 15_000}));
            let result = tokio::spawn(async move { peer.call_tool(arguments).await });
            cluster.await_cleanup_paused().await?;
            tokio::time::sleep(std::time::Duration::from_millis(2_200)).await;
            let premature = result.is_finished();
            let held = cluster.ownership_snapshot(leader)?;
            cluster.release_cleanup_pause();
            let refusal =
                problem(tokio::time::timeout(std::time::Duration::from_secs(10), result).await???)?;
            for (index, before) in &baseline {
                await_baseline(&cluster, *index, *before).await?;
            }
            ingress.cancel().await?;
            assert!(
                !premature,
                "MCP final result preceded owner settlement at endpoint {endpoint}"
            );
            assert!(
                held.leader_graphs > 0 && held.root_query_active,
                "cleanup still owns its graph and grant"
            );
            assert_eq!(refusal["code"], "WYRD_VALA_413_QUERY_RESULT_TOO_LARGE");
            assert!(refusal.get("rows").is_none() && refusal.get("columns").is_none());
        }

        // Repairs and both ceilings refuse before or instead of a partial result.
        for (case, arguments, code) in [
            (
                "malformed sql",
                serde_json::json!({"sql": "SELECT FROM WHERE"}),
                "WYRD_VALA_400_QUERY_INVALID_SQL",
            ),
            (
                "unsupported statement",
                serde_json::json!({"sql": format!("DELETE FROM vala.bifrost.{}", tables[0])}),
                "WYRD_VALA_400_QUERY_INVALID_SQL",
            ),
            (
                "row ceiling",
                serde_json::json!({"sql": join_sql.clone(), "max_rows": 1}),
                "WYRD_VALA_413_QUERY_RESULT_TOO_LARGE",
            ),
            (
                "byte ceiling",
                serde_json::json!({"sql": join_sql.clone(), "max_bytes": 1}),
                "WYRD_VALA_413_QUERY_RESULT_TOO_LARGE",
            ),
        ] {
            let refusal = problem(client.call_tool(query(arguments)).await?)?;
            if refusal["code"] != serde_json::json!(code) {
                return Err(format!("{case}: expected {code}, saw {refusal}").into());
            }
            if refusal.get("rows").is_some() || refusal.get("columns").is_some() {
                return Err(format!("{case} returned a partial result: {refusal}").into());
            }
        }

        // Pause a real activated follower before source IO, so ordinary fast
        // completion cannot masquerade as cancellation.
        for (index, before) in &baseline {
            await_baseline(&cluster, *index, *before).await?;
        }
        let paused = PEER_FOLLOWERS[0];
        cluster.arm_execute_pause(paused)?;
        let handle = client
            .send_cancellable_request(
                ClientRequest::CallToolRequest(CallToolRequest::new(query(
                    serde_json::json!({"sql": join_sql, "max_rows": 100}),
                ))),
                PeerRequestOptions::no_options(),
            )
            .await?;
        cluster.await_execute_paused(paused).await?;
        let family = "oracle_query_duration_seconds";
        let labels = std::collections::BTreeMap::from([
            ("class".to_owned(), "analytical".to_owned()),
            ("outcome".to_owned(), "cancelled".to_owned()),
        ]);
        let before_cancel = cluster.metric_totals_labeled(&[family], &labels)?[family];
        handle.cancel(None).await?;
        for (index, before) in baseline {
            await_baseline(&cluster, index, before).await?;
        }
        let after_cancel = cluster.metric_totals_labeled(&[family], &labels)?[family];
        if after_cancel <= before_cancel {
            return Err(
                "the active MCP query did not record Analytical cancellation before disconnect"
                    .into(),
            );
        }
        client.cancel().await?;
        cluster.shutdown().await?;
        Ok(())
    }
}
