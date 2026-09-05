//! Tier-1 journey: an agent runs a distributed analytical query over MCP.
//!
//! Everything else in this binary drives Oracle through the typed public
//! surfaces. This module drives it the way an agent actually does — an `rmcp`
//! client against one pod's real `/mcp` endpoint on a live four-process
//! topology — because that is the only path where discovery, the closed input
//! bounds, the single settled result, and Oracle's own path selection have to
//! agree at once. The MCP adapter contributes no plan hint, so an Analytical
//! result here is Oracle's decision reaching an agent unchanged.

use rmcp::model::{CallToolRequest, CallToolRequestParams, CallToolResult, ClientRequest};
use rmcp::service::PeerRequestOptions;
use rmcp::transport::StreamableHttpClientTransport;
use rmcp::transport::streamable_http_client::StreamableHttpClientTransportConfig;
use wyrd_testing::bifrost::process_cluster::{
    BifrostProcessCluster, ProcessNode, ProcessNodeTarget,
};

use crate::support::{JourneyError, await_baseline};

/// The pod-per-process test node every journey in this binary launches.
const NODE_BINARY: &str = env!("CARGO_BIN_EXE_bifrost_peer_test_node");

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
/// Returns the transport error, or a description when the auth route refuses
/// the key or answers with a body that is not a token response.
async fn bearer(
    node: &ProcessNode,
    api_key: &secrecy::SecretString,
) -> Result<String, JourneyError> {
    use secrecy::ExposeSecret as _;

    let response = reqwest::Client::new()
        .post(format!("http://{}/auth/token", node.http_addr()))
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
/// Returns an error when the bearer cannot be encoded as a header value.
fn transport(
    node: &ProcessNode,
    bearer: &str,
) -> Result<StreamableHttpClientTransport<reqwest::Client>, JourneyError> {
    let mut config =
        StreamableHttpClientTransportConfig::with_uri(format!("http://{}/mcp", node.http_addr()));
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
        BifrostProcessCluster, COORDINATOR, CallToolRequest, CallToolRequestParams, ClientRequest,
        FIXTURE_GROUPS, FIXTURE_ROWS, JourneyError, NODE_BINARY, PEER_FOLLOWERS, PEER_SCRIBE,
        PeerRequestOptions, ProcessNodeTarget, await_baseline, bearer, discover, problem, query,
        structured, transport,
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
    /// cancellation against one live four-process topology.
    ///
    /// # Errors
    ///
    /// Returns the first claim that broke.
    async fn prove_analytical_mcp_journey() -> Result<(), JourneyError> {
        let mut cluster = BifrostProcessCluster::start(
            NODE_BINARY,
            &[
                ProcessNodeTarget::Oracle,
                ProcessNodeTarget::Oracle,
                ProcessNodeTarget::Oracle,
                ProcessNodeTarget::Scribe,
            ],
        )
        .await?;
        let api_key = cluster
            .provision_public_api_key("mcp-analytical-agent")
            .await?;

        let tables: Vec<String> = (0..3)
            .map(|index| format!("mcp_join_{index}_{}", uuid::Uuid::now_v7().simple()))
            .collect();
        for table in &tables {
            cluster.nodes_mut()[PEER_SCRIBE].register_table(table)?;
            cluster.nodes_mut()[PEER_SCRIBE].ingest_rows(table, 0, FIXTURE_ROWS, FIXTURE_GROUPS)?;
        }
        for index in [
            COORDINATOR,
            PEER_FOLLOWERS[0],
            PEER_FOLLOWERS[1],
            PEER_SCRIBE,
        ] {
            cluster.nodes_mut()[index].refresh_snapshot()?;
        }

        let baseline: Vec<_> = [COORDINATOR, PEER_FOLLOWERS[0], PEER_FOLLOWERS[1]]
            .into_iter()
            .map(|index| Ok((index, cluster.nodes_mut()[index].ownership_snapshot()?)))
            .collect::<Result<_, JourneyError>>()?;
        let polls_before: Vec<u64> = PEER_FOLLOWERS
            .iter()
            .map(|index| Ok(cluster.nodes_mut()[*index].peer_body_polls()?))
            .collect::<Result<_, JourneyError>>()?;

        let token = bearer(&cluster.nodes()[COORDINATOR], &api_key).await?;
        let client = ()
            .serve_with_lifecycle(
                transport(&cluster.nodes()[COORDINATOR], &token)?,
                discover(),
            )
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
            let columns: Vec<&str> = described["fields"]
                .as_array()
                .ok_or("describe returns the stored field list")?
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
        if joined["terminal"]["execution_path"] != serde_json::json!("analytical") {
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

        // Real remote stages ran; a leader-local rewrite would not have polled.
        for (offset, index) in PEER_FOLLOWERS.into_iter().enumerate() {
            let polls = cluster.nodes_mut()[index].peer_body_polls()?;
            if polls <= polls_before[offset] {
                return Err(format!(
                    "follower {index} admitted no peer body: {polls} polls, was {}",
                    polls_before[offset]
                )
                .into());
            }
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
            await_baseline(&mut cluster, *index, *before).await?;
        }
        let paused = PEER_FOLLOWERS[0];
        cluster.nodes_mut()[paused].arm_execute_pause()?;
        let handle = client
            .send_cancellable_request(
                ClientRequest::CallToolRequest(CallToolRequest::new(query(
                    serde_json::json!({"sql": join_sql, "max_rows": 100}),
                ))),
                PeerRequestOptions::no_options(),
            )
            .await?;
        cluster.nodes_mut()[paused].await_execute_paused()?;
        let family = "oracle_query_duration_seconds";
        let labels = std::collections::BTreeMap::from([
            ("class".to_owned(), "analytical".to_owned()),
            ("outcome".to_owned(), "cancelled".to_owned()),
        ]);
        let before_cancel =
            cluster.nodes_mut()[COORDINATOR].metric_totals_labeled(&[family], &labels)?[family];
        handle.cancel(None).await?;
        cluster.nodes_mut()[paused].release_execute_pause()?;
        for (index, before) in baseline {
            await_baseline(&mut cluster, index, before).await?;
        }
        let after_cancel =
            cluster.nodes_mut()[COORDINATOR].metric_totals_labeled(&[family], &labels)?[family];
        if after_cancel <= before_cancel {
            return Err(
                "the active MCP query did not record Analytical cancellation before disconnect"
                    .into(),
            );
        }
        client.cancel().await?;
        cluster.shutdown()?;
        Ok(())
    }
}
