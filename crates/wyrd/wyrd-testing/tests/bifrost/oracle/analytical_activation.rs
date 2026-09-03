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
use vala_bifrost_redux::oracle::{AuthorizedQueryContext, Oracle, OracleQueryStream, QueryIpcDecoder};
use vala_sdk::BifrostGrpcTransport;
use wyrd_client::WyrdClient;
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
async fn run(engine: &Oracle, context: AuthorizedQueryContext, sql: &str) -> Result<Settled, JourneyError> {
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
    let cluster = WyrdTestCluster::start_spec(BifrostClusterSpec::three_oracles_one_scribe()).await?;
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
    expect(&plain, QueryExecutionPath::Interactive, usize::try_from(FIXTURE_ROWS / FIXTURE_GROUPS)?, "non-candidate")?;
    await_clean_nodes(&cluster).await?;

    // An unsupported candidate is refused by the pre-distribution predicate.
    // The armed planner refusal is still armed afterwards, which is the proof
    // that validation ran before the pinned distributed build.
    engine.fail_next_analytical_plan_for_test();
    let unsupported = run(
        &engine,
        query_context(tenant)?,
        &format!(
            "SELECT id, ROW_NUMBER() OVER (ORDER BY id) AS ordinal FROM vala.bifrost.{table}"
        ),
    )
    .await?;
    expect(
        &unsupported,
        QueryExecutionPath::Interactive,
        usize::try_from(FIXTURE_ROWS)?,
        "unsupported candidate",
    )?;
    if !engine.analytical_plan_failure_armed_for_test() {
        return Err("unsupported candidate reached the pinned distributed planner".into());
    }
    await_clean_nodes(&cluster).await?;

    // A supported candidate whose distributed build refuses stays Interactive
    // and consumes the armed refusal.
    let refused = run(&engine, query_context(tenant)?, &grouped).await?;
    expect(&refused, QueryExecutionPath::Interactive, usize::try_from(FIXTURE_GROUPS)?, "planner-refused candidate")?;
    if engine.analytical_plan_failure_armed_for_test() {
        return Err("supported candidate never reached the pinned distributed planner".into());
    }
    await_clean_nodes(&cluster).await?;

    // A supported candidate with nothing scannable produces no exchange.
    let no_exchange = run(
        &engine,
        query_context(tenant)?,
        &format!("SELECT filter_key, COUNT(*) AS matched FROM vala.bifrost.{empty} GROUP BY filter_key"),
    )
    .await?;
    expect(&no_exchange, QueryExecutionPath::Interactive, 0, "no-exchange candidate")?;
    await_clean_nodes(&cluster).await?;

    // A surviving real exchange is the only thing that selects Analytical, on
    // both the public leader entry and the forwarded leader entry.
    let selected = run(&engine, query_context(tenant)?, &grouped).await?;
    expect(&selected, QueryExecutionPath::Analytical, usize::try_from(FIXTURE_GROUPS)?, "supported exchange")?;
    await_clean_nodes(&cluster).await?;

    let forwarded = run_forwarded(&engine, query_context(tenant)?, &grouped).await?;
    expect(&forwarded, QueryExecutionPath::Analytical, usize::try_from(FIXTURE_GROUPS)?, "forwarded supported exchange")?;
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
