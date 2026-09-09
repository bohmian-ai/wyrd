//! Oracle journeys — production selection of the Analytical execution path.
//!
//! Every journey here starts at the ordinary authenticated raw-SQL operation
//! with no caller hint. One pinned physical planner builds the query once:
//! a normal returned root selects Interactive, while a `DistributedExec` root
//! selects Analytical. Planning and selected execution failures are terminal;
//! neither triggers a rebuild or an Interactive fallback.
//!
//! Module of the `oracle` binary; see `main.rs` for the capability it proves
//! and `support.rs` for the fixtures it shares.

use std::sync::Arc;

use arrow::array::{Int32Array, Int64Array};
use arrow::record_batch::RecordBatch;
use futures_util::StreamExt as _;
use vala_bifrost_redux::oracle::Oracle;
use vala_bifrost_redux::oracle::QueryIpcDecoder;
use vala_bifrost_redux::oracle::analytical::{
    AnalyticalCleanupPause, AnalyticalLiveInspection, analytical_cleanup_pause_for_test,
};
use vala_sdk::QueryClient;
use wyrd_client::WyrdClient;
use wyrd_spec::DataTenantId;
use wyrd_spec::request_id::RequestId;
use wyrd_spec::vala::api::{
    BifrostQueryRequest, FreshnessPolicy, QueryExecutionPath, VisibilityMode,
};
use wyrd_testing::bifrost::process_cluster::BifrostProcessCluster;
use wyrd_testing::bifrost::{BifrostClusterSpec, WyrdTestCluster};
use wyrd_tonic::query_conversion::QueryStreamConverter;
use wyrd_tonic::wyrd::v1 as proto;
use wyrd_tonic::wyrd::v1::bifrost_query_service_client::BifrostQueryServiceClient;

use crate::support::*;

/// Rows written into the selection fixture table.
const FIXTURE_ROWS: i64 = 12;

/// Distinct `filter_key` groups the fixture rows fall into.
const FIXTURE_GROUPS: i64 = 3;

/// Exact ordered `(left_id, right_id, right_ordinal)` rows the journey's
/// same-table self-join must return.
///
/// The left alias keeps `filter_key = 'group_0'`, which is ids `0, 3, 6, 9`.
/// The join condition carries that group onto the right alias, whose own
/// `id > 5` predicate leaves ids `6, 9`. Four left rows against two right rows
/// is eight, and those eight pairs are these pairs only while each alias binds
/// its own closure. This journey's process fixture writes all `FIXTURE_ROWS`
/// in one ingest batch, and `wyrd_row_ordinal` is zero-based within a batch, so
/// each right ordinal equals that row's own `id`; the right `id` is what
/// distinguishes the two right-side rows.
const SELF_JOIN_RESULT: [(i64, i64, i32); 8] = [
    (0, 6, 6),
    (0, 9, 9),
    (3, 6, 6),
    (3, 9, 9),
    (6, 6, 6),
    (6, 9, 9),
    (9, 6, 6),
    (9, 9, 9),
];

/// Builds one published-only strict request with the journey's deadline.
fn request(sql: &str) -> BifrostQueryRequest {
    BifrostQueryRequest {
        sql: sql.to_owned(),
        visibility: VisibilityMode::PublishedOnly,
        freshness: FreshnessPolicy::Strict,
        deadline_ms: Some(30_000),
    }
}

/// Writes and publishes one fixture table on the cluster's ingest node.
///
/// # Errors
/// Returns missing-ingest-node, registration, credential, append, flush, or
/// Oracle membership-refresh failures while preparing the published fixture.
async fn seed_table(cluster: &WyrdTestCluster, prefix: &str) -> Result<String, JourneyError> {
    let ingest = cluster
        .servers()
        .find(|server| server.bifrost_scribe().is_some())
        .ok_or("missing ingest node")?;
    let tenant = cluster.data_tenant_id();
    let table = unique_table(prefix);
    register_table(ingest, tenant, &table).await?;
    let rows = writer(ingest, &format!("{prefix}-writer")).await?;
    rows.write(
        &format!("vala.bifrost.{table}"),
        &journey_schema(),
        (0..FIXTURE_ROWS)
            .map(|id| journey_row(id, &format!("group_{}", id % FIXTURE_GROUPS)))
            .collect::<Vec<_>>(),
    )
    .await?;
    ingest.flush_bifrost().await?;
    cluster.refresh_oracle_snapshots().await?;
    Ok(table)
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

/// Waits until one pod has granted at least `units` query slot units.
///
/// A follower pause proves a graph arrived at a peer, which is not the same
/// claim as the leader having charged that graph its full admitted envelope.
/// The envelope is the condition a capacity refusal is a statement about, so
/// the journey reads the leader's own granted units rather than inferring them
/// from the pause.
///
/// # Errors
///
/// Returns the control-protocol error, or a description of what the pod held
/// when the bound expired.
async fn await_admitted(
    cluster: &mut wyrd_testing::bifrost::process_cluster::BifrostProcessCluster,
    index: usize,
    units: u32,
) -> Result<(), JourneyError> {
    for _ in 0..BASELINE_POLLS {
        if cluster.nodes_mut()[index]
            .ownership_snapshot()?
            .root_query_slot_units
            >= units
        {
            return Ok(());
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    let snapshot = cluster.nodes_mut()[index].ownership_snapshot()?;
    Err(format!("pod {index} never granted {units} slot units, holds {snapshot:?}").into())
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

    // A normal root is graphless. It owns no graph while it streams and leaves
    // none behind when its caller drops it mid-stream, so its retirement needs
    // no cleanup join at all.
    let case = "direct interactive";
    let mut stream = dispatch(&query, &plain, case).await?;
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

    prove_paused_cleanup(&cluster, &engine, tenant, query_server, &public, &grouped).await?;
    prove_paused_cleanup_over_grpc(&cluster, &engine, tenant, query_server, &public, &grouped)
        .await?;

    cluster.shutdown().await?;
    Ok(())
}

/// Opens one public query stream, retrying only a client-side send failure.
///
/// A stream this journey abandoned can leave the client's connection pool
/// holding an entry the next request cannot use, which is a property of the
/// pool rather than of the server under test.
///
/// # Errors
///
/// Returns the dispatch failure, naming the case, when the retry also fails.
async fn dispatch(
    query: &QueryClient,
    sql: &str,
    case: &str,
) -> Result<vala_sdk::QueryResultStream, JourneyError> {
    match query.query(&request(sql)).await {
        Ok(stream) => Ok(stream),
        Err(error) if is_transport(&error) => Ok(query
            .query(&request(sql))
            .await
            .map_err(|retry| format!("{case}: query dispatch failed: {error}; {retry}"))?),
        Err(error) => Err(format!("{case}: query dispatch failed: {error}").into()),
    }
}

/// Drops one selected Analytical HTTP consumer mid-stream and holds its release.
///
/// The consumer takes the schema and exactly one batch and then vanishes
/// without `settle` and without reaching a terminal, which is what a real
/// caller that closes its connection does. Every later observation is made on a
/// freshly authenticated client, so the abandoned connection the dropped stream
/// left in the pool cannot serve it.
///
/// # Errors
///
/// Returns a transport, pause, ownership, or assertion error.
async fn prove_paused_cleanup(
    cluster: &WyrdTestCluster,
    engine: &Arc<Oracle>,
    tenant: DataTenantId,
    server: &wyrd_testing::WyrdTestServer,
    public: &WyrdClient,
    sql: &str,
) -> Result<(), JourneyError> {
    let case = "http selected analytical";
    let pause = analytical_cleanup_pause_for_test();
    pause.arm();
    let mut stream = dispatch(&QueryClient::new(public), sql, case).await?;
    let request_id = stream.request_id().clone();
    stream
        .next_batch()
        .await
        .map_err(|error| format!("{case}: first batch failed: {error}"))?;
    if stream.terminal().is_some() {
        return Err(format!("{case}: the abandoned consumer reached its terminal").into());
    }
    drop(stream);
    let observer = client(server, "analytical-cleanup-http-observer").await?;
    hold_and_release(
        cluster,
        &QueryClient::new(&observer),
        &request_id,
        &pause,
        case,
    )
    .await?;
    await_retired(engine, tenant, &request_id, case).await?;
    await_clean_nodes(cluster).await?;
    Ok(())
}

/// Drops one selected Analytical gRPC consumer and its channel mid-stream.
///
/// Both the response stream and the channel that carried it are dropped after a
/// single message, so nothing this consumer owns can drain to a terminal. The
/// running-status observation opens its own channel for the same reason the
/// HTTP case opens its own client.
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

    let case = "grpc selected analytical";
    let channel = wyrd_tonic::tonic::transport::Endpoint::from_shared(
        server.grpc_url().ok_or("missing gRPC URL")?,
    )?
    .connect()
    .await?;
    let bearer = public.auth().bearer().await?;
    let mut grpc = BifrostQueryServiceClient::new(channel.clone());
    let mut grpc_request =
        wyrd_tonic::tonic::Request::new(proto::BifrostQueryRequest::from(request(sql)));
    grpc_request.metadata_mut().insert(
        "x-wyrd-access-token",
        format!("Bearer {}", bearer.expose()).parse()?,
    );

    let pause = analytical_cleanup_pause_for_test();
    pause.arm();
    let response = grpc.query(grpc_request).await?;
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
    let first = frames
        .next()
        .await
        .ok_or_else(|| format!("{case}: the stream closed before its first message"))??;
    if matches!(
        first.frame,
        Some(proto::query_stream_frame::Frame::Terminal(_))
    ) {
        return Err(format!("{case}: the abandoned consumer reached its terminal").into());
    }
    drop(frames);
    drop(grpc);
    drop(channel);

    let observer = client(server, "analytical-cleanup-grpc-observer").await?;
    hold_and_release(
        cluster,
        &QueryClient::new(&observer),
        &request_id,
        &pause,
        case,
    )
    .await?;
    await_retired(engine, tenant, &request_id, case).await?;
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

/// Slot units one admitted Analytical graph charges at the resource root.
///
/// The activation topology starts each pod with one unit more than this, so a
/// single held graph owns everything Analytical work can be granted while the
/// remaining unit stays available to the Interactive floor.
const ANALYTICAL_GRAPH_UNITS: u32 = 2;

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
            credential: Some(api_key.clone()),
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

/// Drives the journey's same-table self-join and decodes its exact rows.
///
/// Returns the ordinary public settlement alongside every decoded
/// `(left_id, right_id, right_ordinal)` tuple in the order the server streamed
/// them, which the query's `ORDER BY` makes deterministic.
///
/// # Errors
///
/// Returns a transport, protocol, Arrow, or missing-terminal error, or a
/// description when a batch does not carry the three expected column types.
async fn run_self_join(
    client: &WyrdClient,
    sql: &str,
) -> Result<(PublicSettlement, Vec<(i64, i64, i32)>), JourneyError> {
    let mut stream = QueryClient::new(client).query(&request(sql)).await?;
    let deadline_ms = stream.deadline_ms();
    let mut rows = 0_usize;
    let mut decoded = Vec::new();
    while let Some(batch) = stream.next_batch().await? {
        rows += batch.num_rows();
        let left = column::<Int64Array>(&batch, 0, "left_id")?;
        let right = column::<Int64Array>(&batch, 1, "right_id")?;
        let ordinal = column::<Int32Array>(&batch, 2, "right_ordinal")?;
        for index in 0..batch.num_rows() {
            decoded.push((left.value(index), right.value(index), ordinal.value(index)));
        }
    }
    let terminal = stream
        .terminal()
        .ok_or("public query produced no terminal frame")?;
    Ok((
        PublicSettlement {
            rows,
            terminal_rows: terminal.row_count,
            path: terminal.execution_path,
            deadline_ms,
        },
        decoded,
    ))
}

/// Borrows one batch column as its expected concrete Arrow array.
///
/// # Errors
///
/// Returns a description when the column is absent or holds another type.
fn column<'batch, A: 'static>(
    batch: &'batch RecordBatch,
    index: usize,
    name: &str,
) -> Result<&'batch A, JourneyError> {
    batch
        .columns()
        .get(index)
        .and_then(|column| column.as_any().downcast_ref::<A>())
        .ok_or_else(|| format!("self-join column {index} ({name}) is missing or mistyped").into())
}

/// Drives one failure phase and proves it entered the physical builder once.
///
/// Returns that phase's own membership digest, so a caller can prove the next
/// phase observed a different cut and is therefore not reading a stale
/// predecessor observation. The structured code and the transport check
/// together state that the caller saw the server's terminal query-execution
/// failure rather than a dropped connection that happens to look like one.
///
/// # Errors
///
/// Returns a description when the statement settles, when the SDK error is a
/// transport failure or carries another code, when the build total moves by
/// anything other than one, or when the phase records no cut.
async fn expect_single_build_failure(
    cluster: &mut wyrd_testing::bifrost::process_cluster::BifrostProcessCluster,
    client: &WyrdClient,
    sql: &str,
    case: &str,
) -> Result<String, JourneyError> {
    let before = cluster.nodes_mut()[COORDINATOR].physical_build_evidence()?;
    let failure = match run_public(client, sql).await {
        Ok(settled) => {
            return Err(format!(
                "{case}: still settled {:?} with {} rows",
                settled.path, settled.rows
            )
            .into());
        }
        Err(error) => error,
    };
    let sdk = failure
        .downcast_ref::<vala_sdk::ValaSdkError>()
        .ok_or_else(|| format!("{case}: failed outside the SDK as {failure}"))?;
    if is_transport(sdk) {
        return Err(format!("{case}: failed as a client transport error: {sdk}").into());
    }
    if sdk.code() != "WYRD_VALA_500_QUERY_EXECUTION_FAILED" {
        return Err(format!("{case}: settled code {}: {sdk}", sdk.code()).into());
    }
    let after = cluster.nodes_mut()[COORDINATOR].physical_build_evidence()?;
    if after.total != before.total + 1 {
        return Err(format!(
            "{case}: entered the physical builder {} times, expected exactly one",
            after.total - before.total
        )
        .into());
    }
    if after.latest_cut_fingerprint.is_empty() {
        return Err(format!("{case}: recorded no membership cut for its build").into());
    }
    Ok(after.latest_cut_fingerprint)
}

/// One immutable cut and one physical root decide the path, the remote capacity
/// it uses, and every terminal failure — with no second build and no fallback.
///
/// # Panics
///
/// Panics when the single-planner routing journey cannot be driven to its
/// claims.
#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
#[ignore = "requires the serialized Postgres-backed Oracle journey lane"]
async fn single_planner_root_selects_path_and_capacity() {
    prove_single_planner_routing()
        .await
        .expect("single-planner routing journey");
}

/// Drives both root shapes and both terminal failures against one live topology.
///
/// The two failures are driven through the real public entry rather than an
/// injector: a planning refusal is armed on the coordinator's own engine, and a
/// stale source is produced by deleting one published Parquet object while its
/// Iceberg metadata and manifests still reference it.
/// Local and forwarded post-pin holds also prove the original signed deadline.
///
/// # Errors
///
/// Returns the first claim that broke.
///
/// # Panics
/// Panics if the deadline helper observes extended authority, late execution,
/// incorrect rows, or physical-build/ownership evidence inconsistent with the query.
async fn prove_single_planner_routing() -> Result<(), JourneyError> {
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
        .provision_public_api_key("single-planner-caller")
        .await?;
    let table = format!("single_planner_{}", uuid::Uuid::now_v7().simple());
    cluster.nodes_mut()[PEER_SCRIBE].register_table(&table)?;
    cluster.nodes_mut()[PEER_SCRIBE].ingest_rows(&table, 0, FIXTURE_ROWS, FIXTURE_GROUPS)?;
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
    let polls_before: Vec<u64> = PEER_FOLLOWERS
        .iter()
        .map(|index| Ok(cluster.nodes_mut()[*index].peer_body_polls()?))
        .collect::<Result<_, JourneyError>>()?;

    let client = public_client(&cluster.nodes()[COORDINATOR], &api_key)?;
    let scan_sql = format!("SELECT id FROM vala.bifrost.{table} WHERE filter_key = 'group_0'");
    let grouped_sql = format!(
        "SELECT filter_key, COUNT(*) AS matched FROM vala.bifrost.{table} \
         GROUP BY filter_key ORDER BY filter_key"
    );

    for endpoint in [COORDINATOR, PEER_SCRIBE] {
        for expire in [false, true] {
            prove_preparation_deadline(&mut cluster, &api_key, &grouped_sql, endpoint, expire)
                .await?;
        }
    }

    // A normal root is entirely leader-executable and therefore Interactive.
    let scan = run_public(&client, &scan_sql).await?;
    expect_public(
        &scan,
        QueryExecutionPath::Interactive,
        usize::try_from(FIXTURE_ROWS / FIXTURE_GROUPS)?,
        "normal root",
    )?;

    // The grouped statement is the only one whose root is `DistributedExec`.
    let grouped = run_public(&client, &grouped_sql).await?;
    expect_public(
        &grouped,
        QueryExecutionPath::Analytical,
        usize::try_from(FIXTURE_GROUPS)?,
        "distributed root",
    )?;

    // Real remote stages, not a leader-local rewrite of a distributed plan.
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
    for (index, before) in baseline.clone() {
        await_baseline(&mut cluster, index, before).await?;
    }

    // A same-table self-join reaches the one registered provider twice. Each
    // alias carries its own projection and predicate closure, so the two
    // physical occurrences must bind independently: a collision would let one
    // side read the other's closure and change the result.
    let polls_before_join: Vec<u64> = PEER_FOLLOWERS
        .iter()
        .map(|index| Ok(cluster.nodes_mut()[*index].peer_body_polls()?))
        .collect::<Result<_, JourneyError>>()?;
    let self_join_sql = format!(
        "SELECT l.id AS left_id, r.id AS right_id, r.wyrd_row_ordinal AS right_ordinal \
         FROM vala.bifrost.{table} l \
         JOIN vala.bifrost.{table} r ON l.filter_key = r.filter_key \
         WHERE l.filter_key = 'group_0' AND r.id > 5 \
         ORDER BY left_id, right_id, right_ordinal"
    );
    // Driven here rather than through `run_public` because the claim is about
    // the values: a count alone cannot tell an independently bound pair of
    // occurrences from two that collided onto one closure.
    let (self_join, self_join_result) = run_self_join(&client, &self_join_sql).await?;
    expect_public(
        &self_join,
        QueryExecutionPath::Analytical,
        SELF_JOIN_RESULT.len(),
        "same-table self-join",
    )?;
    if self_join_result != SELF_JOIN_RESULT {
        return Err(format!(
            "same-table self-join returned {self_join_result:?}, expected {SELF_JOIN_RESULT:?}"
        )
        .into());
    }
    for (offset, index) in PEER_FOLLOWERS.into_iter().enumerate() {
        let polls = cluster.nodes_mut()[index].peer_body_polls()?;
        if polls <= polls_before_join[offset] {
            return Err(format!(
                "follower {index} admitted no self-join peer body: {polls} polls, was {}",
                polls_before_join[offset]
            )
            .into());
        }
    }
    for (index, before) in baseline.clone() {
        await_baseline(&mut cluster, index, before).await?;
    }

    // One armed planning refusal settles the sole attempt: there is no second
    // build to fall back to, so the caller sees a failure rather than rows.
    cluster.nodes_mut()[COORDINATOR].arm_analytical_plan_failure()?;
    let refusal_cut =
        expect_single_build_failure(&mut cluster, &client, &grouped_sql, "planning refusal")
            .await?;
    for (index, before) in baseline.clone() {
        await_baseline(&mut cluster, index, before).await?;
    }

    // A pinned object that no longer exists is terminal on the ordinary
    // normal-root scan: the pinned cut still names it, so the leader must
    // fail its one attempt rather than repin, replan, or rebuild.
    let object = pinned_parquet(&cluster, &table)?;
    cluster.remove_storage_object(&object)?;
    let stale_cut =
        expect_single_build_failure(&mut cluster, &client, &scan_sql, "missing pinned object")
            .await?;
    // Each phase froze its own membership, so an observation carried over from
    // the refusal above could not be mistaken for this phase's evidence.
    if stale_cut == refusal_cut {
        return Err(format!("both failure phases report one request-local cut {stale_cut}").into());
    }
    for (index, before) in baseline {
        await_baseline(&mut cluster, index, before).await?;
    }

    cluster.shutdown()?;
    Ok(())
}

/// A request-keyed child pause compares verified ingress authority with the
/// real gRPC response after pinning has consumed part of the original budget.
///
/// # Errors
/// Returns fixture, transport, decoding, deadline, or ownership proof failures.
///
/// # Panics
/// Panics if the response widens ingress time, preparation executes after expiry,
/// or physical-build, terminal, or returned-row evidence violates the query.
async fn prove_preparation_deadline(
    cluster: &mut BifrostProcessCluster,
    api_key: &secrecy::SecretString,
    sql: &str,
    endpoint: usize,
    expire: bool,
) -> Result<(), JourneyError> {
    let client = public_client(&cluster.nodes()[endpoint], api_key)?;
    let bearer = client.auth().bearer().await?;
    let channel = wyrd_tonic::tonic::transport::Endpoint::from_shared(format!(
        "http://{}",
        cluster.nodes()[endpoint].grpc_addr()
    ))?
    .connect()
    .await?;
    let mut grpc = BifrostQueryServiceClient::new(channel);
    let request_id = RequestId::now_v7();
    let mut query = request(sql);
    query.deadline_ms = Some(if expire { 1_000 } else { 5_000 });
    let mut query = wyrd_tonic::tonic::Request::new(proto::BifrostQueryRequest::from(query));
    query.metadata_mut().insert(
        "x-wyrd-access-token",
        format!("Bearer {}", bearer.expose()).parse()?,
    );
    query
        .metadata_mut()
        .insert("wyrd-request-id", request_id.as_str().parse()?);
    let candidates = oracle_indices(cluster);
    let baseline = candidates
        .iter()
        .map(|index| {
            Ok((
                *index,
                cluster.nodes_mut()[*index].ownership_snapshot()?,
                cluster.nodes_mut()[*index].physical_build_evidence()?.total,
            ))
        })
        .collect::<Result<Vec<_>, JourneyError>>()?;
    for index in &candidates {
        cluster.nodes_mut()[*index].arm_preparation_pause(&request_id)?;
    }
    let response = tokio::spawn(async move { grpc.query(query).await });
    let accepted = tokio::time::timeout(std::time::Duration::from_secs(4), async {
        loop {
            for index in &candidates {
                if let Some(deadline) = cluster.nodes_mut()[*index].preparation_pause_deadline()? {
                    return Ok::<_, JourneyError>(deadline);
                }
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await??;
    if expire {
        let remaining =
            u64::try_from(accepted.saturating_sub(chrono::Utc::now().timestamp_millis()))?;
        tokio::time::sleep(std::time::Duration::from_millis(remaining + 20)).await;
        let outcome = tokio::time::timeout(std::time::Duration::from_secs(1), response).await??;
        for index in &candidates {
            cluster.nodes_mut()[*index].release_preparation_pause()?;
        }
        let error = outcome.expect_err("expired preparation cannot publish a response stream");
        let problem: serde_json::Value = serde_json::from_slice(
            &error
                .metadata()
                .get_bin(wyrd_tonic::error::WYRD_ERROR_HEADER)
                .ok_or("timeout omits canonical problem")?
                .to_bytes()?,
        )?;
        assert_eq!(problem["code"], "WYRD_VALA_504_QUERY_TIMEOUT");
        for (index, ownership, before) in baseline {
            await_baseline(cluster, index, ownership).await?;
            assert_eq!(
                cluster.nodes_mut()[index].physical_build_evidence()?.total,
                before,
                "expired pinning cannot build or publish a roster to execution"
            );
        }
        return Ok(());
    }
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    assert!(
        !response.is_finished(),
        "post-pin pause must retain preparation"
    );
    for index in &candidates {
        cluster.nodes_mut()[*index].release_preparation_pause()?;
    }
    let response = response.await??;
    let advertised: i64 = response
        .metadata()
        .get("x-wyrd-query-deadline-ms")
        .ok_or("response omits deadline")?
        .to_str()?
        .parse()?;
    let mut frames = response.into_inner();
    let mut ipc = QueryIpcDecoder::new();
    let mut converter = QueryStreamConverter::new(VisibilityMode::PublishedOnly);
    let mut terminal = None;
    while let Some(frame) = frames.next().await {
        let frame = frame?;
        let rows = match frame.frame.as_ref() {
            Some(proto::query_stream_frame::Frame::Schema(schema)) => {
                ipc.accept_schema(&schema.arrow_ipc_schema)?;
                None
            }
            Some(proto::query_stream_frame::Frame::Batch(batch)) => Some(u64::try_from(
                ipc.accept_batch(&batch.arrow_ipc_batch)?.num_rows(),
            )?),
            _ => None,
        };
        if let wyrd_spec::vala::api::QueryStreamFrame::Terminal(frame) =
            converter.convert(frame, rows)?
        {
            ipc.accept_eos(&frame.arrow_ipc_eos)?;
            terminal = Some(frame);
        }
    }
    let terminal = terminal.ok_or("query omitted terminal")?;
    assert_eq!(
        terminal.outcome,
        wyrd_spec::vala::api::QueryTerminalOutcome::Success
    );
    assert_eq!(terminal.execution_path, QueryExecutionPath::Analytical);
    assert_eq!(terminal.row_count, u64::try_from(FIXTURE_GROUPS)?);
    let mut builds = 0;
    for (index, ownership, before) in baseline {
        await_baseline(cluster, index, ownership).await?;
        builds += cluster.nodes_mut()[index].physical_build_evidence()?.total - before;
    }
    assert_eq!(builds, 1, "preparation must retain one physical build");
    assert_eq!(
        advertised, accepted,
        "pinning cannot replace the verified ingress deadline"
    );
    Ok(())
}

/// Returns one published Parquet data object of `table`, relative to the store.
///
/// Only row-bearing Parquet objects are eligible, so the caller deletes data
/// while every Iceberg metadata and manifest file the snapshot depends on
/// stays intact. The process fixture seals its rows to staged hot Parquet and
/// runs no Forge compaction, so this is that hot object; the pinned cut names
/// it exactly as it names a compacted one, and the stale-source branch under
/// test does not distinguish the two.
///
/// # Errors
///
/// Returns a description when the store cannot be walked or holds no pinned
/// data object for `table`.
fn pinned_parquet(
    cluster: &wyrd_testing::bifrost::process_cluster::BifrostProcessCluster,
    table: &str,
) -> Result<String, JourneyError> {
    let root = cluster.storage_root().to_path_buf();
    let mut seen: Vec<String> = Vec::new();
    let mut pending = vec![root.clone()];
    while let Some(directory) = pending.pop() {
        for entry in std::fs::read_dir(&directory)? {
            let path = entry?.path();
            if path.is_dir() {
                pending.push(path);
                continue;
            }
            let relative = path.strip_prefix(&root)?.to_string_lossy().into_owned();
            let is_metadata = std::path::Path::new(&relative)
                .components()
                .any(|component| component.as_os_str() == "metadata");
            seen.push(relative.clone());
            if !is_metadata && relative.contains(table) && relative.ends_with(".parquet") {
                return Ok(relative);
            }
        }
    }
    Err(format!("the object store holds no pinned data object for {table}; saw {seen:?}").into())
}

/// Asserts one public settlement's selected path and exact row count.
///
/// # Errors
///
/// Returns a description naming the case, the expectation, and what was seen.
fn expect_public(
    settled: &PublicSettlement,
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
    if settled.terminal_rows != u64::try_from(settled.rows)? {
        return Err(format!(
            "{case}: terminal row count {} disagrees with {} decoded rows",
            settled.terminal_rows, settled.rows
        )
        .into());
    }
    Ok(())
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
    // Three slot units per pod is what makes the two claims below independent:
    // an Analytical graph charges two units at the resource root, so one held
    // graph owns every unit Analytical work can be granted, while the third
    // unit remains for the Interactive floor. Stating the number rather than
    // deriving it from the injected memory envelope is what makes the refusal
    // a statement about admitted capacity instead of about whatever the
    // envelope happened to divide into.
    let mut cluster =
        wyrd_testing::bifrost::process_cluster::BifrostProcessCluster::start_with_oracle_query_slot_limit(
            NODE_BINARY,
            &[
                wyrd_testing::bifrost::process_cluster::ProcessNodeTarget::Oracle,
                wyrd_testing::bifrost::process_cluster::ProcessNodeTarget::Oracle,
                wyrd_testing::bifrost::process_cluster::ProcessNodeTarget::Oracle,
                wyrd_testing::bifrost::process_cluster::ProcessNodeTarget::Scribe,
            ],
            Some(3),
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
    let suffix = uuid::Uuid::now_v7().simple();
    let table = format!("public_activation_{suffix}");
    cluster.nodes_mut()[PEER_SCRIBE].register_table(&table)?;
    cluster.nodes_mut()[PEER_SCRIBE].ingest_rows(&table, 0, 12, 3)?;
    cluster.nodes_mut()[PEER_SCRIBE].ingest_rows(&table, 0, 12, 3)?;
    // The UI table publishes one object, so its scan is a single-partition
    // leader-executable leaf and its root is normal. The Analytical table above
    // publishes two, which is what gives its grouped statement real remote work
    // to distribute.
    let ui_table = format!("public_activation_ui_{suffix}");
    cluster.nodes_mut()[PEER_SCRIBE].register_table(&ui_table)?;
    cluster.nodes_mut()[PEER_SCRIBE].ingest_rows(&ui_table, 0, 12, 3)?;
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
    let polls_before: Vec<u64> = PEER_FOLLOWERS
        .iter()
        .map(|index| Ok(cluster.nodes_mut()[*index].peer_body_polls()?))
        .collect::<Result<_, JourneyError>>()?;

    let client = public_client(&cluster.nodes()[COORDINATOR], &api_key)?;
    let analytical_sql = format!(
        "SELECT filter_key, COUNT(*) AS matched FROM vala.bifrost.{table} \
         GROUP BY filter_key ORDER BY filter_key"
    );
    let ui_sql = format!("SELECT id FROM vala.bifrost.{ui_table} WHERE filter_key = 'group_0'");

    // Held at a real follower boundary, so the Analytical class is occupied by
    // production queries while the UI query below asks for its own path. Two
    // graphs are exactly the class capacity configured above.
    let paused = PEER_FOLLOWERS[0];
    cluster.nodes_mut()[paused].arm_execute_pause()?;
    let held = {
        let client = public_client(&cluster.nodes()[COORDINATOR], &api_key)?;
        let sql = analytical_sql.clone();
        tokio::spawn(async move { run_public(&client, &sql).await })
    };
    cluster.nodes_mut()[paused].await_execute_paused()?;
    // The pause proves the graph reached a follower; the leader's own granted
    // unit count is what proves it holds the whole Analytical envelope, which
    // is the condition the refusal below is a statement about.
    await_admitted(&mut cluster, COORDINATOR, ANALYTICAL_GRAPH_UNITS).await?;

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
    if ui.rows != usize::try_from(FIXTURE_ROWS / FIXTURE_GROUPS)? {
        return Err(format!("the UI query returned {} rows", ui.rows).into());
    }

    // The pod's whole Analytical class is held, so one more Analytical
    // statement has no capacity of its own. It may wait for a held graph to
    // release or be refused outright; what it may never do is settle Analytical
    // while the class is fully owned.
    match run_public(&client, &analytical_sql).await {
        Err(error) if error.to_string().contains("query admission rejected") => {}
        Err(error) => {
            return Err(
                format!("an Analytical query beyond the class capacity failed as {error}").into(),
            );
        }
        Ok(settled) => {
            let snapshot = cluster.nodes_mut()[COORDINATOR].ownership_snapshot()?;
            return Err(format!(
                "an Analytical query settled {:?} while the class was fully held; leader holds {snapshot:?}",
                settled.path
            )
            .into());
        }
    }

    cluster.nodes_mut()[paused].release_execute_pause()?;
    let analytical = held.await??;
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

/// Fallback is a pre-selection decision only, and a selected Analytical query
/// that loses a peer fails once without ever rerunning locally.
///
/// # Panics
///
/// Panics when either half of the journey cannot be driven to its claims.
#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
#[ignore = "requires the serialized Postgres-backed Oracle journey lane"]
async fn selected_peer_failure_is_terminal() {
    prove_under_privileged_refusal()
        .await
        .expect("under-privileged refusal journey");
    prove_selected_failure_is_terminal()
        .await
        .expect("selected Analytical terminal failure journey");
}

/// An under-privileged caller is refused before any planning, peer, or source IO.
///
/// # Errors
///
/// Returns the first claim that broke.
async fn prove_under_privileged_refusal() -> Result<(), JourneyError> {
    let cluster =
        WyrdTestCluster::start_spec(BifrostClusterSpec::three_oracles_one_scribe()).await?;
    let tenant = cluster.data_tenant_id();
    let table = seed_table(&cluster, "under_privileged").await?;
    let query_server = cluster.server(0).ok_or("missing query node")?;
    let grouped = format!(
        "SELECT filter_key, COUNT(*) AS matched FROM vala.bifrost.{table} GROUP BY filter_key"
    );

    // Denied before planning, peers, or sources: a caller holding no query
    // permission must leave the node exactly as it found it.
    let bootstrap = query_server
        .bootstrap_service_in_tenant(tenant, "under-privileged-denied", &[])
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
    let attempts_before = attempt_totals(&mut cluster)?;
    let builds_before = cluster.nodes_mut()[COORDINATOR]
        .physical_build_evidence()?
        .total;
    cluster.nodes_mut()[paused].arm_execute_pause()?;
    let client = public_client(&cluster.nodes()[COORDINATOR], &api_key)?;
    let query = {
        let client = public_client(&cluster.nodes()[COORDINATOR], &api_key)?;
        let sql = sql.clone();
        // Driven here rather than through `run_public` because the claim is
        // about the failure itself: the caller must receive one structured
        // Wyrd error, not an anonymous transport break or a truncated success.
        tokio::spawn(async move {
            let mut stream = QueryClient::new(&client).query(&request(&sql)).await?;
            let mut rows = 0_usize;
            while let Some(batch) = stream.next_batch().await? {
                rows += batch.num_rows();
            }
            Ok::<usize, vala_sdk::ValaSdkError>(rows)
        })
    };
    cluster.nodes_mut()[paused].await_execute_paused()?;

    // Read while the query is still held at the follower: the cut the leader's
    // active entry retains is the one its single build ran against, so the
    // terminal failure below cannot be reporting a rebuilt or repinned
    // membership.
    let held = cluster.nodes_mut()[COORDINATOR].physical_build_evidence()?;
    let [active_cut] = held.active_cut_fingerprints.as_slice() else {
        return Err(format!(
            "the paused query must be the coordinator's sole active query, saw {:?}",
            held.active_cut_fingerprints
        )
        .into());
    };
    if *active_cut != held.latest_cut_fingerprint {
        return Err(format!(
            "the active query retains cut {active_cut}, built against {}",
            held.latest_cut_fingerprint
        )
        .into());
    }

    cluster.nodes_mut()[paused].kill()?;

    let failure = match query.await? {
        Ok(rows) => {
            return Err(
                format!("a lost peer must not produce a successful result of {rows} rows").into(),
            );
        }
        Err(failure) => failure,
    };
    if is_transport(&failure) || !failure.code().starts_with("WYRD_") {
        return Err(format!("the lost peer surfaced {failure} as {}", failure.code()).into());
    }

    // One build for the whole attempt: the lost peer produced no successor
    // ordinal, so the coordinator never re-entered the shared physical builder.
    let builds = cluster.nodes_mut()[COORDINATOR]
        .physical_build_evidence()?
        .total;
    if builds != builds_before + 1 {
        return Err(format!(
            "the lost peer entered the physical builder {} times, expected exactly one",
            builds - builds_before
        )
        .into());
    }

    // One attempt, terminally failed. A repin, a replan, or any local
    // successor would raise the total past one, and a cancellation or a
    // success would land the one attempt on another outcome entirely.
    let attempts = attempt_totals(&mut cluster)?;
    let started = attempts.total - attempts_before.total;
    let succeeded = attempts.succeeded - attempts_before.succeeded;
    if started != 1.0 || succeeded != 0.0 {
        return Err(format!(
            "the lost peer settled {started} attempts of which {succeeded} succeeded, \
             not exactly one unsuccessful attempt"
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

/// Analytical attempt totals one coordinator has recorded.
struct AttemptTotals {
    /// Attempts settled on every outcome.
    total: f64,
    /// Attempts settled on the successful outcome alone.
    succeeded: f64,
}

/// Reads the coordinator's own Analytical attempt counters.
///
/// The counter is the production family a build increments once per attempt,
/// so a differenced pair of these reads is what makes "one build, and it did
/// not succeed" an observation rather than an inference from the absence of
/// rows. A lost peer settles its one attempt as cancelled rather than failed —
/// the leader cancels the graph — so the successful outcome is the one the
/// claim excludes.
///
/// # Errors
///
/// Returns the control-protocol error unchanged.
fn attempt_totals(
    cluster: &mut wyrd_testing::bifrost::process_cluster::BifrostProcessCluster,
) -> Result<AttemptTotals, JourneyError> {
    let family = "bifrost_oracle_analytical_attempts_total";
    let total = cluster.nodes_mut()[COORDINATOR].metric_totals(&[family])?;
    let succeeded = cluster.nodes_mut()[COORDINATOR].metric_totals_labeled(
        &[family],
        &std::collections::BTreeMap::from([("outcome".to_owned(), "success".to_owned())]),
    )?;
    Ok(AttemptTotals {
        total: total.get(family).copied().unwrap_or_default(),
        succeeded: succeeded.get(family).copied().unwrap_or_default(),
    })
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
