//! Oracle journeys — production selection of the Analytical execution path.
//!
//! Every journey here starts at the ordinary authenticated raw-SQL operation
//! with no caller hint. What it proves is the selection sequence: one locally
//! executable physical plan, the unchanged closed support predicate, and only
//! then the pinned distributed build. Interactive is the default and is also
//! the outcome of every pre-selection refusal, and only a surviving real
//! exchange transfers this query's envelope into an Analytical graph.
//!
//! Module of the `oracle` binary; see `main.rs` for the capability it proves
//! and `support.rs` for the fixtures it shares.

use std::sync::Arc;

use arrow::array::{Int64Array, StringArray};
use arrow::datatypes::{DataType, Field, Schema};
use arrow::ipc::writer::StreamWriter;
use arrow::record_batch::RecordBatch;
use futures_util::StreamExt as _;
use vala_bifrost_redux::oracle::analytical::{
    AnalyticalCleanupPause, AnalyticalLiveInspection, analytical_cleanup_pause_for_test,
};
use vala_bifrost_redux::oracle::{
    AuthorizedQueryContext, Oracle, OracleQueryStream, QueryIpcDecoder,
};
use vala_sdk::{BifrostGrpcTransport, QueryClient};
use wyrd_client::WyrdClient;
use wyrd_spec::DataTenantId;
use wyrd_spec::request_id::RequestId;
use wyrd_spec::vala::api::{
    BifrostQueryRequest, FreshnessPolicy, QueryExecutionPath, QueryStreamFrame, VisibilityMode,
};
use wyrd_testing::bifrost::{BifrostClusterSpec, WyrdTestCluster};

use crate::support::*;

/// Rows written into the selection fixture table.
const FIXTURE_ROWS: i64 = 12;

/// Distinct `filter_key` groups the fixture rows fall into.
const FIXTURE_GROUPS: i64 = 3;

/// What one drained production query settled to.
struct Settled {
    /// Total rows across every decoded Arrow batch.
    rows: usize,
    /// Execution path the server itself selected for this query.
    path: QueryExecutionPath,
}

/// Builds one published-only strict request with the journey's deadline.
fn request(sql: &str) -> BifrostQueryRequest {
    BifrostQueryRequest {
        sql: sql.to_owned(),
        visibility: VisibilityMode::PublishedOnly,
        freshness: FreshnessPolicy::Strict,
        deadline_ms: Some(30_000),
    }
}

/// Drains one already-opened Oracle stream to its terminal frame.
///
/// The terminal is required rather than optional: a stream that ended without
/// one never settled whatever ownership it held, so its selected path would be
/// a claim about an attempt that is still running.
///
/// # Errors
///
/// Returns a frame, decode, or missing-terminal error.
async fn drain(mut stream: OracleQueryStream) -> Result<Settled, JourneyError> {
    let mut decoder = QueryIpcDecoder::new();
    let mut rows = 0_usize;
    let mut terminal = None;
    while let Some(frame) = stream.frames.next().await {
        match frame? {
            QueryStreamFrame::Schema(schema) => {
                decoder.accept_schema(&schema.arrow_ipc_schema)?;
            }
            QueryStreamFrame::Batch(batch) => {
                rows += decoder.accept_batch(&batch.arrow_ipc_batch)?.num_rows();
            }
            QueryStreamFrame::Terminal(frame) => terminal = Some(frame),
        }
    }
    let terminal = terminal.ok_or("production query stream emitted no terminal frame")?;
    Ok(Settled {
        rows,
        path: terminal.execution_path,
    })
}

/// Runs one statement through the ordinary public leader entry.
///
/// # Errors
///
/// Returns any stable query error, or a drain failure.
async fn run(
    engine: &Oracle,
    context: AuthorizedQueryContext,
    sql: &str,
) -> Result<Settled, JourneyError> {
    drain(engine.query_sql(context, request(sql)).await?).await
}

/// Runs one statement through the forwarded leader entry with a production cut.
///
/// # Errors
///
/// Returns any stable query error, or a drain failure.
async fn run_forwarded(
    engine: &Oracle,
    context: AuthorizedQueryContext,
    sql: &str,
) -> Result<Settled, JourneyError> {
    let request = request(sql);
    let (cut, planned) = engine
        .prepare_query_attempt_for_test(&context, &request)
        .await?;
    let query_class = planned.query_class();
    drain(
        engine
            .query_sql_with_participant_cut(context, request, cut, query_class, Some(planned))
            .await?,
    )
    .await
}

/// Sends one Arrow IPC batch carrying a single fixture row.
async fn ingest_row(
    client: &WyrdClient,
    table: &str,
    id: i64,
    filter_key: &str,
) -> Result<(), JourneyError> {
    BifrostGrpcTransport::connect(client)
        .await?
        .insert_batch(
            table,
            uuid::Uuid::now_v7().into_bytes(),
            ipc_row(id, filter_key),
        )
        .await?;
    Ok(())
}

/// Encodes one deterministic `(id, filter_key, unused_payload)` row as Arrow IPC.
fn ipc_row(id: i64, filter_key: &str) -> Vec<u8> {
    let schema = Arc::new(Schema::new(vec![
        Field::new("id", DataType::Int64, false),
        Field::new("filter_key", DataType::Utf8, false),
        Field::new("unused_payload", DataType::Utf8, false),
    ]));
    let batch = RecordBatch::try_new(
        Arc::clone(&schema),
        vec![
            Arc::new(Int64Array::from(vec![id])),
            Arc::new(StringArray::from(vec![filter_key])),
            Arc::new(StringArray::from(vec![unused_payload(id)])),
        ],
    )
    .expect("fixed journey arrays share a length");
    let mut bytes = Vec::new();
    let mut writer = StreamWriter::try_new(&mut bytes, schema.as_ref()).expect("valid schema");
    writer.write(&batch).expect("in-memory IPC write");
    writer.finish().expect("in-memory IPC finish");
    bytes
}

/// Writes and publishes one fixture table on the cluster's ingest node.
async fn seed_table(cluster: &WyrdTestCluster, prefix: &str) -> Result<String, JourneyError> {
    let ingest = cluster
        .servers()
        .find(|server| server.bifrost_scribe().is_some())
        .ok_or("missing ingest node")?;
    let tenant = cluster.data_tenant_id();
    let table = unique_table(prefix);
    register_table(ingest, tenant, &table).await?;
    let writer = client(ingest, &format!("{prefix}-writer")).await?;
    for id in 0..FIXTURE_ROWS {
        let group = format!("group_{}", id % FIXTURE_GROUPS);
        ingest_row(&writer, &format!("vala.bifrost.{table}"), id, &group).await?;
    }
    ingest.flush_bifrost().await?;
    cluster.refresh_oracle_snapshots().await?;
    Ok(table)
}

/// Analytical selection happens only after a locally executable plan passes the
/// unchanged support predicate and the pinned distributed build keeps an
/// exchange; every earlier refusal stays Interactive and graphless.
#[tokio::test]
#[ignore = "requires the serialized Postgres-backed Oracle journey lane"]
async fn analytical_selection_requires_supported_physical_exchange() {
    prove_selection()
        .await
        .expect("production analytical selection journey");
}

/// Drives the complete selection matrix against one real cluster.
///
/// # Errors
///
/// Returns a cluster, execution, ownership, or assertion error.
async fn prove_selection() -> Result<(), JourneyError> {
    let cluster =
        WyrdTestCluster::start_spec(BifrostClusterSpec::three_oracles_one_scribe()).await?;
    let tenant = cluster.data_tenant_id();
    let table = seed_table(&cluster, "analytical_selection").await?;
    let empty = {
        let ingest = cluster
            .servers()
            .find(|server| server.bifrost_scribe().is_some())
            .ok_or("missing ingest node")?;
        let name = unique_table("analytical_selection_empty");
        register_table(ingest, tenant, &name).await?;
        cluster.refresh_oracle_snapshots().await?;
        name
    };
    let query_server = cluster.server(0).ok_or("missing query node")?;
    let engine = Arc::clone(
        query_server
            .state()
            .bifrost_query()
            .ok_or("query node composed no Oracle")?
            .engine(),
    );

    let grouped = format!(
        "SELECT filter_key, COUNT(*) AS matched FROM vala.bifrost.{table} GROUP BY filter_key"
    );

    // A non-candidate never reaches selection at all.
    let plain = run(
        &engine,
        query_context(tenant)?,
        &format!("SELECT id FROM vala.bifrost.{table} WHERE filter_key = 'group_0'"),
    )
    .await?;
    expect(
        &plain,
        QueryExecutionPath::Interactive,
        usize::try_from(FIXTURE_ROWS / FIXTURE_GROUPS)?,
        "non-candidate",
    )?;
    await_clean_nodes(&cluster).await?;

    // A supported candidate whose distributed build refuses stays Interactive
    // and consumes the armed refusal.
    engine.fail_next_analytical_plan_for_test();
    let refused = run(&engine, query_context(tenant)?, &grouped).await?;
    expect(
        &refused,
        QueryExecutionPath::Interactive,
        usize::try_from(FIXTURE_GROUPS)?,
        "planner-refused candidate",
    )?;
    if engine.analytical_plan_failure_armed_for_test() {
        return Err("supported candidate never reached the pinned distributed planner".into());
    }
    await_clean_nodes(&cluster).await?;

    // A supported candidate with nothing scannable produces no exchange.
    let no_exchange = run(
        &engine,
        query_context(tenant)?,
        &format!(
            "SELECT filter_key, COUNT(*) AS matched FROM vala.bifrost.{empty} GROUP BY filter_key"
        ),
    )
    .await?;
    expect(
        &no_exchange,
        QueryExecutionPath::Interactive,
        0,
        "no-exchange candidate",
    )?;
    await_clean_nodes(&cluster).await?;

    // A surviving real exchange is the only thing that selects Analytical, on
    // both the public leader entry and the forwarded leader entry.
    let selected = run(&engine, query_context(tenant)?, &grouped).await?;
    expect(
        &selected,
        QueryExecutionPath::Analytical,
        usize::try_from(FIXTURE_GROUPS)?,
        "supported exchange",
    )?;
    await_clean_nodes(&cluster).await?;

    let forwarded = run_forwarded(&engine, query_context(tenant)?, &grouped).await?;
    expect(
        &forwarded,
        QueryExecutionPath::Analytical,
        usize::try_from(FIXTURE_GROUPS)?,
        "forwarded supported exchange",
    )?;
    await_clean_nodes(&cluster).await?;

    cluster.shutdown().await?;
    Ok(())
}

/// Asserts one settled query's selected path and exact row count.
///
/// # Errors
///
/// Returns a description naming the case, the expectation, and what was seen.
fn expect(
    settled: &Settled,
    path: QueryExecutionPath,
    rows: usize,
    case: &str,
) -> Result<(), JourneyError> {
    if settled.path != path {
        return Err(format!(
            "{case}: expected {path:?} execution path, saw {:?}",
            settled.path
        )
        .into());
    }
    if settled.rows != rows {
        return Err(format!("{case}: expected {rows} rows, saw {}", settled.rows).into());
    }
    Ok(())
}

/// Bounded polls the journey waits for a retired public running entry.
const RETIREMENT_POLLS: usize = 50;

/// Waits, bounded, until one request identity is no longer a running query.
///
/// Retirement is what proves the server released the owners the stream held.
/// It is polled rather than read once because the transport drop that triggers
/// it is observed by the server, not published by the client. The registry is
/// read in process: the public status surface is proved while the graph is
/// still held, and an abandoned stream can leave this client's connection pool
/// holding an entry no later probe can use.
///
/// # Errors
///
/// Returns a description naming the case whose entry was still running when
/// the bound expired, with the lifecycle state that names why.
async fn await_retired(
    engine: &Arc<Oracle>,
    tenant: DataTenantId,
    request_id: &RequestId,
    case: &str,
) -> Result<(), JourneyError> {
    for _ in 0..RETIREMENT_POLLS {
        if engine.running_queries().get(tenant, request_id).is_none() {
            return Ok(());
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    let state = engine
        .running_queries()
        .list(tenant)
        .into_iter()
        .find(|summary| &summary.request_id == request_id)
        .map(|summary| format!("{:?}", summary.state));
    Err(format!(
        "{case}: running entry was never retired; state={}",
        state.unwrap_or_else(|| "<absent>".to_owned())
    )
    .into())
}

/// Reports whether one SDK error is a client-side transport send failure.
///
/// Only that class is retried by this journey: a dropped stream can leave the
/// client's connection pool holding an entry the next request cannot use.
fn is_transport(error: &vala_sdk::ValaSdkError) -> bool {
    error.code() == "WYRD_SPEC_500_INTERNAL" && error.to_string().contains("transport error")
}

/// Reports whether every Oracle node currently retains no Analytical ownership.
///
/// # Errors
///
/// Returns the first node inspection error.
fn nodes_clean(cluster: &WyrdTestCluster) -> Result<bool, JourneyError> {
    Ok(live_ownership(cluster)?
        .iter()
        .all(AnalyticalLiveInspection::is_clean))
}

/// Interactive streams are graphless and settle on their own; only a selected
/// Analytical query keeps its running entry and graph ownership until the
/// leader lifecycle's cleanup join completes.
#[tokio::test]
#[ignore = "requires the serialized Postgres-backed Oracle journey lane"]
async fn transport_drop_retains_running_status_until_cleanup_joins() {
    prove_cleanup_ownership()
        .await
        .expect("production analytical cleanup ownership journey");
}

/// Drives the graphless-Interactive and paused-Analytical cases on one cluster.
///
/// # Errors
///
/// Returns a cluster, execution, ownership, transport, or assertion error.
async fn prove_cleanup_ownership() -> Result<(), JourneyError> {
    let cluster =
        WyrdTestCluster::start_spec(BifrostClusterSpec::three_oracles_one_scribe()).await?;
    let tenant = cluster.data_tenant_id();
    let table = seed_table(&cluster, "analytical_cleanup").await?;
    let empty = {
        let ingest = cluster
            .servers()
            .find(|server| server.bifrost_scribe().is_some())
            .ok_or("missing ingest node")?;
        let name = unique_table("analytical_cleanup_empty");
        register_table(ingest, tenant, &name).await?;
        cluster.refresh_oracle_snapshots().await?;
        name
    };
    let query_server = cluster.server(0).ok_or("missing query node")?;
    let engine = Arc::clone(
        query_server
            .state()
            .bifrost_query()
            .ok_or("query node composed no Oracle")?
            .engine(),
    );
    let public = client(query_server, "analytical-cleanup-reader").await?;
    let query = QueryClient::new(&public);

    let plain = format!("SELECT id FROM vala.bifrost.{table} WHERE filter_key = 'group_0'");
    let grouped = format!(
        "SELECT filter_key, COUNT(*) AS matched FROM vala.bifrost.{table} GROUP BY filter_key"
    );
    let no_exchange = format!(
        "SELECT filter_key, COUNT(*) AS matched FROM vala.bifrost.{empty} GROUP BY filter_key"
    );

    for (case, sql, refuse) in [
        ("direct interactive", plain.as_str(), false),
        ("unsupported fallback", grouped.as_str(), true),
        ("no-exchange fallback", no_exchange.as_str(), false),
    ] {
        if refuse {
            engine.fail_next_analytical_plan_for_test();
        }
        let mut stream = match query.query(&request(sql)).await {
            Ok(stream) => stream,
            // Same abandoned-connection hazard as the retirement probe: the
            // first dispatch after a dropped stream may fail to send.
            Err(error) if is_transport(&error) => query
                .query(&request(sql))
                .await
                .map_err(|retry| format!("{case}: query dispatch failed: {error}; {retry}"))?,
            Err(error) => return Err(format!("{case}: query dispatch failed: {error}").into()),
        };
        let request_id = stream.request_id().clone();
        stream
            .next_batch()
            .await
            .map_err(|error| format!("{case}: first batch failed: {error}"))?;
        if !nodes_clean(&cluster)? {
            return Err(format!("{case}: Interactive attempt registered a graph").into());
        }
        drop(stream);
        await_retired(&engine, tenant, &request_id, case).await?;
        if !nodes_clean(&cluster)? {
            return Err(format!("{case}: Interactive retirement left graph ownership").into());
        }
    }

    prove_paused_cleanup(&cluster, &engine, tenant, &query, &grouped).await?;
    prove_paused_cleanup_over_grpc(&cluster, &engine, tenant, query_server, &public, &grouped)
        .await?;

    cluster.shutdown().await?;
    Ok(())
}

/// Holds one selected Analytical graph on its clean release over HTTP.
///
/// # Errors
///
/// Returns a transport, pause, ownership, or assertion error.
async fn prove_paused_cleanup(
    cluster: &WyrdTestCluster,
    engine: &Arc<Oracle>,
    tenant: DataTenantId,
    query: &QueryClient,
    sql: &str,
) -> Result<(), JourneyError> {
    let pause = analytical_cleanup_pause_for_test();
    pause.arm();
    let mut stream = query
        .query(&request(sql))
        .await
        .map_err(|error| format!("http selected analytical: query dispatch failed: {error}"))?;
    let request_id = stream.request_id().clone();
    let drain = tokio::spawn(async move {
        while stream.next_batch().await?.is_some() {}
        Ok::<_, vala_sdk::ValaSdkError>(stream.terminal().map(|frame| frame.execution_path))
    });
    hold_and_release(
        cluster,
        query,
        &request_id,
        &pause,
        "http selected analytical",
    )
    .await?;
    let path = drain
        .await??
        .ok_or("http selected analytical: stream emitted no terminal frame")?;
    if path != QueryExecutionPath::Analytical {
        return Err(format!("http selected analytical: settled on {path:?}").into());
    }
    await_retired(engine, tenant, &request_id, "http selected analytical").await?;
    await_clean_nodes(cluster).await?;
    Ok(())
}

/// Holds one selected Analytical graph on its clean release over gRPC.
///
/// # Errors
///
/// Returns a transport, pause, ownership, or assertion error.
async fn prove_paused_cleanup_over_grpc(
    cluster: &WyrdTestCluster,
    engine: &Arc<Oracle>,
    tenant: DataTenantId,
    server: &wyrd_testing::WyrdTestServer,
    public: &WyrdClient,
    sql: &str,
) -> Result<(), JourneyError> {
    use wyrd_tonic::wyrd::v1 as proto;
    use wyrd_tonic::wyrd::v1::bifrost_query_service_client::BifrostQueryServiceClient;

    let channel = wyrd_tonic::tonic::transport::Endpoint::from_shared(
        server.grpc_url().ok_or("missing gRPC URL")?,
    )?
    .connect()
    .await?;
    let bearer = public.auth().bearer().await?;
    let mut grpc = BifrostQueryServiceClient::new(channel);
    let mut request =
        wyrd_tonic::tonic::Request::new(proto::BifrostQueryRequest::from(request(sql)));
    request.metadata_mut().insert(
        "x-wyrd-access-token",
        format!("Bearer {}", bearer.expose()).parse()?,
    );

    let pause = analytical_cleanup_pause_for_test();
    pause.arm();
    let response = grpc.query(request).await?;
    let deadline_ms: i64 = response
        .metadata()
        .get("x-wyrd-query-deadline-ms")
        .ok_or("gRPC query response omits x-wyrd-query-deadline-ms")?
        .to_str()?
        .parse()?;
    if deadline_ms < 0 {
        return Err(format!("gRPC query deadline {deadline_ms} is negative").into());
    }
    let request_id = RequestId::parse(
        response
            .metadata()
            .get("x-wyrd-request-id")
            .ok_or("gRPC query response omits x-wyrd-request-id")?
            .to_str()?,
    )?;
    let mut frames = response.into_inner();
    let drain = tokio::spawn(async move {
        let mut path = None;
        while let Some(frame) = frames.next().await {
            if let Some(proto::query_stream_frame::Frame::Terminal(terminal)) = frame?.frame {
                path = Some(terminal.execution_path);
            }
        }
        Ok::<_, JourneyError>(path)
    });
    let query = QueryClient::new(public);
    hold_and_release(
        cluster,
        &query,
        &request_id,
        &pause,
        "grpc selected analytical",
    )
    .await?;
    let path = drain
        .await??
        .ok_or("grpc selected analytical: stream emitted no terminal frame")?;
    if path != proto::QueryExecutionPath::Analytical as i32 {
        return Err(format!("grpc selected analytical: settled on path {path}").into());
    }
    await_retired(engine, tenant, &request_id, "grpc selected analytical").await?;
    await_clean_nodes(cluster).await?;
    Ok(())
}

/// Asserts running status and graph ownership survive the paused release.
///
/// # Errors
///
/// Returns a timeout, transport, ownership, or assertion error naming the case.
async fn hold_and_release(
    cluster: &WyrdTestCluster,
    query: &QueryClient,
    request_id: &RequestId,
    pause: &AnalyticalCleanupPause,
    case: &str,
) -> Result<(), JourneyError> {
    tokio::time::timeout(std::time::Duration::from_secs(120), pause.wait_entered())
        .await
        .map_err(|_| format!("{case}: cleanup never reached the graph release pause"))?;
    query
        .status(request_id)
        .await
        .map_err(|error| format!("{case}: running entry retired before cleanup joined: {error}"))?;
    if nodes_clean(cluster)? {
        return Err(format!("{case}: graph ownership was released before cleanup joined").into());
    }
    pause.release();
    Ok(())
}

/// Path of the compiled child every simulated pod in this journey runs.
const NODE_BINARY: &str = env!("CARGO_BIN_EXE_bifrost_peer_test_node");

/// Index of the pod public queries are addressed to.
const COORDINATOR: usize = 0;

/// Indices of the pods that follow the coordinator's distributed graph.
const PEER_FOLLOWERS: [usize; 2] = [1, 2];

/// Index of the pod that publishes the data every Oracle reads.
const PEER_SCRIBE: usize = 3;

/// Bounded polls the journey waits for a pod to return to its baseline.
const BASELINE_POLLS: usize = 50;

/// Builds one public client against one pod's public HTTP and gRPC listeners.
///
/// # Errors
///
/// Returns the client configuration error.
fn public_client(
    node: &wyrd_testing::bifrost::process_cluster::ProcessNode,
    api_key: &secrecy::SecretString,
) -> Result<WyrdClient, JourneyError> {
    Ok(WyrdClient::with_config(
        wyrd_client::config::ClientConfig {
            grpc: wyrd_client::transport::GrpcConfig {
                endpoint: format!("http://{}", node.grpc_addr()),
                connect_retries: 0,
                ..wyrd_client::transport::GrpcConfig::default()
            },
            http: wyrd_client::transport::HttpConfig {
                base_url: format!("http://{}", node.http_addr()),
                ..wyrd_client::transport::HttpConfig::default()
            },
            api_key: Some(api_key.clone()),
            ..wyrd_client::config::ClientConfig::default()
        },
    )?)
}

/// What one public query settled to over the real public HTTP surface.
struct PublicSettlement {
    /// Rows the caller decoded.
    rows: usize,
    /// Row count the server's own terminal frame reported.
    terminal_rows: u64,
    /// Server-selected execution path.
    path: QueryExecutionPath,
    /// Absolute deadline the response header pinned.
    deadline_ms: i64,
}

/// Drives one complete public query through `vala-sdk` and settles it.
///
/// # Errors
///
/// Returns a transport, protocol, Arrow, or missing-terminal error.
async fn run_public(client: &WyrdClient, sql: &str) -> Result<PublicSettlement, JourneyError> {
    let mut stream = QueryClient::new(client).query(&request(sql)).await?;
    let deadline_ms = stream.deadline_ms();
    let mut rows = 0_usize;
    while let Some(batch) = stream.next_batch().await? {
        rows += batch.num_rows();
    }
    let terminal = stream
        .terminal()
        .ok_or("public query produced no terminal frame")?;
    Ok(PublicSettlement {
        rows,
        terminal_rows: terminal.row_count,
        path: terminal.execution_path,
        deadline_ms,
    })
}

/// A public UI query holds the Interactive floor while a public data-scientist
/// query runs Analytical across real pods, and every pod returns to baseline.
///
/// # Panics
///
/// Panics when the public activation journey cannot be driven to its claims.
#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
#[ignore = "requires the serialized Postgres-backed Oracle journey lane"]
async fn public_query_selects_both_paths_and_preserves_interactive_floor() {
    prove_public_activation()
        .await
        .expect("public analytical activation journey");
}

/// Drives both public paths against one live four-process topology.
///
/// # Errors
///
/// Returns the first claim that broke.
async fn prove_public_activation() -> Result<(), JourneyError> {
    let mut cluster = wyrd_testing::bifrost::process_cluster::BifrostProcessCluster::start(
        NODE_BINARY,
        &[
            wyrd_testing::bifrost::process_cluster::ProcessNodeTarget::Oracle,
            wyrd_testing::bifrost::process_cluster::ProcessNodeTarget::Oracle,
            wyrd_testing::bifrost::process_cluster::ProcessNodeTarget::Oracle,
            wyrd_testing::bifrost::process_cluster::ProcessNodeTarget::Scribe,
        ],
    )
    .await?;

    let pids: std::collections::BTreeSet<u32> =
        cluster.nodes().iter().map(|node| node.pid()).collect();
    if pids.len() != cluster.nodes().len() {
        return Err(format!("four pods must be four processes, saw pids {pids:?}").into());
    }

    // A public request carries no execution-path field: the path is server
    // state a caller can read on the terminal and never ask for.
    let probe = serde_json::to_value(request("SELECT 1"))?;
    let fields: Vec<&String> = probe
        .as_object()
        .ok_or("public query request is not a JSON object")?
        .keys()
        .collect();
    if fields.iter().any(|field| field.contains("path")) {
        return Err(format!("public query request exposes a path field: {fields:?}").into());
    }

    let api_key = cluster
        .provision_public_api_key("analytical-activation-caller")
        .await?;
    let table = format!("public_activation_{}", uuid::Uuid::now_v7().simple());
    cluster.nodes_mut()[PEER_SCRIBE].register_table(&table)?;
    cluster.nodes_mut()[PEER_SCRIBE].ingest_rows(&table, 0, 12, 3)?;
    cluster.nodes_mut()[PEER_SCRIBE].ingest_rows(&table, 0, 12, 3)?;
    for index in [
        COORDINATOR,
        PEER_FOLLOWERS[0],
        PEER_FOLLOWERS[1],
        PEER_SCRIBE,
    ] {
        cluster.nodes_mut()[index].refresh_snapshot()?;
    }

    let baseline: Vec<_> = oracle_indices(&cluster)
        .into_iter()
        .map(|index| Ok((index, cluster.nodes_mut()[index].ownership_snapshot()?)))
        .collect::<Result<_, JourneyError>>()?;
    let selections_before = selection_total(&mut cluster.nodes_mut()[COORDINATOR])?;
    let polls_before: Vec<u64> = PEER_FOLLOWERS
        .iter()
        .map(|index| Ok(cluster.nodes_mut()[*index].peer_body_polls()?))
        .collect::<Result<_, JourneyError>>()?;

    let client = public_client(&cluster.nodes()[COORDINATOR], &api_key)?;
    let analytical_sql = format!(
        "SELECT filter_key, COUNT(*) AS matched FROM vala.bifrost.{table} \
         GROUP BY filter_key ORDER BY filter_key"
    );
    let ui_sql = format!("SELECT id FROM vala.bifrost.{table} WHERE filter_key = 'group_0'");

    // Held at a real follower boundary, so the Analytical grant is occupied by
    // a production query while the UI query below asks for its own path.
    let paused = PEER_FOLLOWERS[0];
    cluster.nodes_mut()[paused].arm_execute_pause()?;
    let analytical = {
        let client = public_client(&cluster.nodes()[COORDINATOR], &api_key)?;
        let sql = analytical_sql.clone();
        tokio::spawn(async move { run_public(&client, &sql).await })
    };
    cluster.nodes_mut()[paused].await_execute_paused()?;

    // Served, exactly, while an Analytical graph on the same pod holds its
    // admitted envelope and both followers hold their leases: the Interactive
    // floor is what makes that concurrency possible rather than a queue.
    let ui = run_public(&client, &ui_sql).await?;
    if ui.path != QueryExecutionPath::Interactive {
        return Err(format!(
            "a UI query alongside a held Analytical graph must stay Interactive, settled {:?}",
            ui.path
        )
        .into());
    }
    if ui.rows != usize::try_from(FIXTURE_ROWS / FIXTURE_GROUPS)? * 2 {
        return Err(format!("the UI query returned {} rows", ui.rows).into());
    }

    cluster.nodes_mut()[paused].release_execute_pause()?;
    let analytical = analytical.await??;
    if analytical.path != QueryExecutionPath::Analytical {
        return Err(format!(
            "the supported data-scientist query must select Analytical, settled {:?}",
            analytical.path
        )
        .into());
    }
    if analytical.rows != usize::try_from(FIXTURE_GROUPS)? {
        return Err(format!("the Analytical query returned {} rows", analytical.rows).into());
    }
    for settled in [&ui, &analytical] {
        if settled.terminal_rows != u64::try_from(settled.rows)? {
            return Err(format!(
                "terminal row count {} disagrees with {} decoded rows",
                settled.terminal_rows, settled.rows
            )
            .into());
        }
        if settled.deadline_ms <= 0 {
            return Err(format!(
                "the response pinned no absolute deadline, carried {}",
                settled.deadline_ms
            )
            .into());
        }
    }

    // Finite result pressure fails safely rather than truncating.
    match QueryClient::new(&client)
        .collect_bounded(
            &request(&analytical_sql),
            vala_sdk::CollectedQueryLimits {
                max_rows: 1,
                max_encoded_bytes: usize::MAX,
            },
        )
        .await
    {
        Err(vala_sdk::ValaSdkError::ResultTooLarge) => {}
        Err(error) => return Err(format!("bounded collection failed as {error}").into()),
        Ok(result) => {
            return Err(
                format!("bounded collection returned {} truncated rows", result.rows).into(),
            );
        }
    }

    // Real peer-socket work, not a local rewrite of a distributed plan.
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

    let selections_after = selection_total(&mut cluster.nodes_mut()[COORDINATOR])?;
    if selections_after <= selections_before {
        return Err(format!(
            "selection telemetry did not advance: {selections_after} totals, was {selections_before}"
        )
        .into());
    }

    for (index, before) in baseline {
        await_baseline(&mut cluster, index, before).await?;
    }
    cluster.shutdown()?;
    Ok(())
}

/// Returns every Oracle pod index in one process topology.
fn oracle_indices(
    cluster: &wyrd_testing::bifrost::process_cluster::BifrostProcessCluster,
) -> Vec<usize> {
    cluster
        .nodes()
        .iter()
        .enumerate()
        .filter(|(_, node)| {
            node.target() == wyrd_testing::bifrost::process_cluster::ProcessNodeTarget::Oracle
        })
        .map(|(index, _)| index)
        .collect()
}

/// Totals one pod's bounded Analytical selection counter across every outcome.
///
/// # Errors
///
/// Returns the control-protocol error.
fn selection_total(
    node: &mut wyrd_testing::bifrost::process_cluster::ProcessNode,
) -> Result<f64, JourneyError> {
    Ok(node
        .metric_totals(&["oracle_query_analytical_selection_total"])?
        .values()
        .sum())
}

/// Waits, bounded, until one pod's ownership returns to its recorded baseline.
///
/// # Errors
///
/// Returns the control-protocol error, or a description of what the pod still
/// retained when the bound expired.
async fn await_baseline(
    cluster: &mut wyrd_testing::bifrost::process_cluster::BifrostProcessCluster,
    index: usize,
    before: wyrd_testing::bifrost::process_cluster::OracleOwnershipSnapshot,
) -> Result<(), JourneyError> {
    for _ in 0..BASELINE_POLLS {
        if cluster.nodes_mut()[index].ownership_snapshot()? == before {
            return Ok(());
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    let after = cluster.nodes_mut()[index].ownership_snapshot()?;
    Err(format!("pod {index} did not return to {before:?}, holds {after:?}").into())
}

/// Fallback is a pre-selection decision only, and a selected Analytical query
/// that loses a peer fails once without ever rerunning locally.
///
/// # Panics
///
/// Panics when either half of the journey cannot be driven to its claims.
#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
#[ignore = "requires the serialized Postgres-backed Oracle journey lane"]
async fn fallback_is_preselection_only_and_failure_is_terminal() {
    prove_preselection_fallback()
        .await
        .expect("pre-selection fallback journey");
    prove_selected_failure_is_terminal()
        .await
        .expect("selected Analytical terminal failure journey");
}

/// Both refusable candidates fall back Interactive, and an under-privileged
/// caller is refused before any planning, peer, or source IO.
///
/// # Errors
///
/// Returns the first claim that broke.
async fn prove_preselection_fallback() -> Result<(), JourneyError> {
    let cluster =
        WyrdTestCluster::start_spec(BifrostClusterSpec::three_oracles_one_scribe()).await?;
    let tenant = cluster.data_tenant_id();
    let table = seed_table(&cluster, "preselection_fallback").await?;
    let empty = {
        let ingest = cluster
            .servers()
            .find(|server| server.bifrost_scribe().is_some())
            .ok_or("missing ingest node")?;
        let name = unique_table("preselection_fallback_empty");
        register_table(ingest, tenant, &name).await?;
        cluster.refresh_oracle_snapshots().await?;
        name
    };
    let query_server = cluster.server(0).ok_or("missing query node")?;
    let engine = Arc::clone(
        query_server
            .state()
            .bifrost_query()
            .ok_or("query node composed no Oracle")?
            .engine(),
    );

    let grouped = format!(
        "SELECT filter_key, COUNT(*) AS matched FROM vala.bifrost.{table} GROUP BY filter_key"
    );
    engine.fail_next_analytical_plan_for_test();
    let refused = run(&engine, query_context(tenant)?, &grouped).await?;
    expect(
        &refused,
        QueryExecutionPath::Interactive,
        usize::try_from(FIXTURE_GROUPS)?,
        "planner-refused candidate",
    )?;
    if !nodes_clean(&cluster)? {
        return Err("a refused candidate registered a graph".into());
    }
    await_clean_nodes(&cluster).await?;

    let no_exchange = run(
        &engine,
        query_context(tenant)?,
        &format!(
            "SELECT filter_key, COUNT(*) AS matched FROM vala.bifrost.{empty} GROUP BY filter_key"
        ),
    )
    .await?;
    expect(
        &no_exchange,
        QueryExecutionPath::Interactive,
        0,
        "no-exchange candidate",
    )?;
    if !nodes_clean(&cluster)? {
        return Err("a no-exchange candidate registered a graph".into());
    }
    await_clean_nodes(&cluster).await?;

    // Denied before planning, peers, or sources: a caller holding no query
    // permission must leave the node exactly as it found it.
    let bootstrap = query_server
        .bootstrap_service_in_tenant(tenant, "preselection-denied", &[])
        .await?;
    let denied = client_from_bootstrap(query_server, bootstrap).await?;
    let error = QueryClient::new(&denied)
        .query(&request(&grouped))
        .await
        .err()
        .ok_or("an under-privileged caller ran a query")?;
    if error.status() != 403 {
        return Err(format!(
            "the denied caller was refused as {} instead",
            error.status()
        )
        .into());
    }
    if !QueryClient::new(&denied)
        .running()
        .await
        .is_err_and(|error| error.status() == 403)
    {
        return Err("the denied caller could still list running queries".into());
    }
    if !nodes_clean(&cluster)? {
        return Err("a denied query left Analytical ownership behind".into());
    }

    cluster.shutdown().await?;
    Ok(())
}

/// A selected Analytical query that loses a follower fails once and terminally.
///
/// # Errors
///
/// Returns the first claim that broke.
async fn prove_selected_failure_is_terminal() -> Result<(), JourneyError> {
    let mut cluster = wyrd_testing::bifrost::process_cluster::BifrostProcessCluster::start(
        NODE_BINARY,
        &[
            wyrd_testing::bifrost::process_cluster::ProcessNodeTarget::Oracle,
            wyrd_testing::bifrost::process_cluster::ProcessNodeTarget::Oracle,
            wyrd_testing::bifrost::process_cluster::ProcessNodeTarget::Oracle,
            wyrd_testing::bifrost::process_cluster::ProcessNodeTarget::Scribe,
        ],
    )
    .await?;
    let api_key = cluster
        .provision_public_api_key("selected-failure-caller")
        .await?;
    let table = format!("selected_failure_{}", uuid::Uuid::now_v7().simple());
    cluster.nodes_mut()[PEER_SCRIBE].register_table(&table)?;
    cluster.nodes_mut()[PEER_SCRIBE].ingest_rows(&table, 0, 12, 3)?;
    cluster.nodes_mut()[PEER_SCRIBE].ingest_rows(&table, 0, 12, 3)?;
    for index in [
        COORDINATOR,
        PEER_FOLLOWERS[0],
        PEER_FOLLOWERS[1],
        PEER_SCRIBE,
    ] {
        cluster.nodes_mut()[index].refresh_snapshot()?;
    }

    let sql = format!(
        "SELECT filter_key, COUNT(*) AS matched FROM vala.bifrost.{table} \
         GROUP BY filter_key ORDER BY filter_key"
    );
    let paused = PEER_FOLLOWERS[0];
    let survivor = PEER_FOLLOWERS[1];
    cluster.nodes_mut()[paused].arm_execute_pause()?;
    let client = public_client(&cluster.nodes()[COORDINATOR], &api_key)?;
    let query = {
        let client = public_client(&cluster.nodes()[COORDINATOR], &api_key)?;
        let sql = sql.clone();
        tokio::spawn(async move { run_public(&client, &sql).await })
    };
    cluster.nodes_mut()[paused].await_execute_paused()?;
    cluster.nodes_mut()[paused].kill()?;

    if let Ok(settled) = query.await? {
        return Err(format!(
            "a lost peer must not produce a successful {:?} result of {} rows",
            settled.path, settled.rows
        )
        .into());
    }

    // One activation on the survivor is the whole no-rerun claim: a local
    // successor attempt would have reserved and activated a second graph.
    let (activated, live) = await_released_lease(&mut cluster, survivor).await?;
    if activated != 1 {
        return Err(format!(
            "the survivor must be addressed by exactly one attempt, activated {activated}"
        )
        .into());
    }
    if live != 0 {
        return Err(format!("the survivor still holds {live} graph leases").into());
    }
    let idle = wyrd_testing::bifrost::process_cluster::OracleOwnershipSnapshot {
        leader_attempts: 0,
        leader_graphs: 0,
        leader_cleanup_failures: 0,
        follower_attempts: 0,
        follower_graphs: 0,
        follower_cleanup_failures: 0,
        active_queries: 0,
        queued_queries: 0,
        reserved_memory_bytes: 0,
        reserved_spill_bytes: 0,
        peer_pending: 0,
        peer_running: 0,
        root_active_queries: 0,
        root_analytical_queries: 0,
        root_query_slot_units: 0,
        root_query_memory_used_bytes: 0,
        root_query_scratch_used_bytes: 0,
        root_query_active: false,
        scratch: wyrd_testing::bifrost::process_cluster::ScratchUsage {
            entries: 0,
            bytes: 0,
        },
        attempts_active: 0.0,
        exchanges_active: 0.0,
        fragments_active: 0.0,
    };
    await_baseline(&mut cluster, COORDINATOR, idle).await?;

    // The pod this journey killed has no stdin left, so explicit shutdown
    // reports it; asserting on that proves every other pod was still joined.
    let killed = cluster.nodes()[paused].label().to_owned();
    drop(client);
    match cluster.shutdown() {
        Err(reported) if reported.to_string().contains(&killed) => Ok(()),
        Err(reported) => Err(format!("shutdown reported {reported}, not {killed}").into()),
        Ok(()) => Err(format!("shutdown did not report the killed pod {killed}").into()),
    }
}

/// Waits, bounded, until one pod has released every graph lease it activated.
///
/// A follower settles on its own stage-operation path rather than with the
/// leader's stream, so the release is a bounded convergence rather than an
/// instantaneous read.
///
/// # Errors
///
/// Returns the control-protocol error, or a description of the lease the pod
/// still held when the bound expired.
async fn await_released_lease(
    cluster: &mut wyrd_testing::bifrost::process_cluster::BifrostProcessCluster,
    index: usize,
) -> Result<(u64, usize), JourneyError> {
    for _ in 0..BASELINE_POLLS {
        let leases = cluster.nodes_mut()[index].graph_leases()?;
        if leases.1 == 0 {
            return Ok(leases);
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    let (activated, live) = cluster.nodes_mut()[index].graph_leases()?;
    Err(format!("pod {index} activated {activated} leases and still holds {live}").into())
}
