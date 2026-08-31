//! Oracle journeys — the inactive distributed Analytical engine.
//!
//! Every journey here begins at raw SQL and runs the complete production
//! attempt path — validation, classification, the participant cut, providers,
//! audit, admission, terminal stream assembly — and differs from a routed query
//! in exactly one place: the execution lease installs the query-owned runtime,
//! the frozen worker set, and the signing channel resolver that make the plan
//! distribute across followers through the real private transport.
//!
//! Nothing in routing can select this path. That is the point: the distributed
//! engine is proved from raw SQL to drained result and back to a clean node
//! before it is ever reachable.
//!
//! Module of the `oracle` binary; see `main.rs` for the capability it proves
//! and `support.rs` for the fixtures it shares.

use std::sync::Arc;

use arrow::array::{Array as _, Int64Array, StringArray};
use arrow::datatypes::{DataType, Field, Schema};
use arrow::ipc::writer::StreamWriter;
use arrow::record_batch::RecordBatch;
use futures_util::StreamExt as _;
use vala_bifrost_redux::oracle::QueryIpcDecoder;
use vala_bifrost_redux::oracle::analytical::{
    AnalyticalAttemptContext, DataFusionQueryId, PublicQueryId,
};
use vala_sdk::BifrostGrpcTransport;
use wyrd_client::WyrdClient;
use wyrd_runtime::permission::PermissionSet;
use wyrd_runtime::{Permission, Principal, PrincipalKind};
use wyrd_spec::DataTenantId;
use wyrd_spec::auth::PrincipalId;
use wyrd_spec::request_id::RequestId;
use wyrd_spec::vala::api::{
    AuthMethod, BifrostQueryRequest, FreshnessPolicy, QueryStreamFrame, QueryTerminalFrame,
    VisibilityMode,
};
use wyrd_testing::WyrdTestServer;
use wyrd_testing::bifrost::{BifrostClusterSpec, WyrdTestCluster};

use crate::support::*;

/// One drained inactive Analytical result: its rows and its terminal frame.
struct AnalyticalOutcome {
    /// Every decoded Arrow batch the distributed plan produced, in order.
    batches: Vec<RecordBatch>,
    /// The one terminal frame the production stream owner emitted.
    terminal: QueryTerminalFrame,
}

impl AnalyticalOutcome {
    /// Total rows across every decoded batch.
    fn rows(&self) -> usize {
        self.batches.iter().map(RecordBatch::num_rows).sum()
    }
}

/// Builds one authenticated in-process query context for the fixture tenant.
///
/// The public SDK cannot reach the inactive path, so the journey authenticates
/// the same way the public query service does — a tenant-bound principal
/// holding exactly `bifrost:query:read` — and hands Oracle the identical
/// context its own gRPC surface would have built.
fn query_context(
    tenant: DataTenantId,
) -> Result<vala_bifrost_redux::oracle::AuthorizedQueryContext, JourneyError> {
    let permission = Permission::bifrost_query_read();
    let principal = Principal::new(
        PrincipalId::new(uuid::Uuid::now_v7()),
        PrincipalKind::User,
        tenant,
        Vec::new(),
        PermissionSet::from_iter([permission.clone()]),
    );
    Ok(vala_bifrost_redux::oracle::AuthorizedQueryContext::try_new(
        principal,
        tenant,
        RequestId::now_v7(),
        None,
        AuthMethod::Internal,
        permission.to_string(),
    )?)
}

/// Allocates the per-query identities one inactive attempt is leased under.
///
/// The two query identities are allocated independently on purpose: a leaked
/// public identity into the distributed graph, or the reverse, is exactly what
/// the stage authority's identity isolation exists to refuse.
fn attempt_context() -> AnalyticalAttemptContext {
    AnalyticalAttemptContext {
        public_query_id: PublicQueryId::from_uuid(uuid::Uuid::now_v7()),
        datafusion_query_id: DataFusionQueryId::from_uuid(uuid::Uuid::now_v7()),
        snapshot_digest: format!("snapshot-{}", uuid::Uuid::now_v7().simple()),
        reservation_id: format!("reservation-{}", uuid::Uuid::now_v7().simple()),
        permission_digest: format!("permission-{}", uuid::Uuid::now_v7().simple()),
    }
}

/// Runs one raw SQL statement through the inactive Analytical path and drains it.
///
/// The stream is drained to its terminal frame rather than dropped early, so
/// the graph and attempt guards it carries settle before the caller inspects
/// the node. A journey that asserted a clean node without draining would be
/// asserting on ownership that had not yet been asked to release.
///
/// # Errors
///
/// Returns a role-unavailable, admission, planning, execution, decode, or
/// terminal-contract error surfaced by the attempt.
async fn execute_inactive_analytical(
    server: &WyrdTestServer,
    tenant: DataTenantId,
    sql: &str,
) -> Result<AnalyticalOutcome, JourneyError> {
    let engine = Arc::clone(
        server
            .state()
            .bifrost_query()
            .ok_or("query node composed no Oracle")?
            .engine(),
    );
    let mut stream = engine
        .query_sql_inactive_analytical(
            query_context(tenant)?,
            BifrostQueryRequest {
                sql: sql.to_owned(),
                visibility: VisibilityMode::PublishedOnly,
                freshness: FreshnessPolicy::Strict,
                deadline_ms: Some(30_000),
            },
            attempt_context(),
        )
        .await?;
    let mut decoder = QueryIpcDecoder::new();
    let mut batches = Vec::new();
    let mut terminal = None;
    while let Some(frame) = stream.frames.next().await {
        match frame? {
            QueryStreamFrame::Schema(schema) => {
                decoder.accept_schema(&schema.arrow_ipc_schema)?;
            }
            QueryStreamFrame::Batch(batch) => {
                batches.push(decoder.accept_batch(&batch.arrow_ipc_batch)?);
            }
            QueryStreamFrame::Terminal(frame) => {
                terminal = Some(frame);
            }
        }
    }
    Ok(AnalyticalOutcome {
        batches,
        terminal: terminal.ok_or("inactive analytical stream emitted no terminal frame")?,
    })
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

/// Rows written into every fixture table, chosen so a grouped aggregate has
/// more than one non-trivial group and a join has matching keys on both sides.
const FIXTURE_ROWS: i64 = 12;

/// Number of distinct `filter_key` groups the fixture rows fall into.
const FIXTURE_GROUPS: i64 = 3;

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

/// Raw SQL joining two published tables and aggregating the result executes on
/// followers and returns the same rows a single node would have produced.
#[tokio::test]
#[ignore = "requires the serialized Postgres-backed Oracle journey lane"]
async fn pg_inactive_analytical_raw_sql_executes_join_and_partial_final_aggregate_on_followers() {
    prove_join_and_aggregate()
        .await
        .expect("inactive analytical join and aggregate journey");
}

/// Drives the join and aggregate proof against a six-node cluster.
///
/// # Errors
///
/// Returns a cluster, ingest, execution, or assertion error.
async fn prove_join_and_aggregate() -> Result<(), JourneyError> {
    let cluster = WyrdTestCluster::start_spec(BifrostClusterSpec::six_capacity()).await?;
    let tenant = cluster.data_tenant_id();
    let left = seed_table(&cluster, "analytical_left").await?;
    let right = seed_table(&cluster, "analytical_right").await?;
    let query_server = cluster.server(0).ok_or("missing query node")?;

    let outcome = execute_inactive_analytical(
        query_server,
        tenant,
        &format!(
            "SELECT l.filter_key, COUNT(*) AS matched \
             FROM vala.bifrost.{left} AS l \
             JOIN vala.bifrost.{right} AS r ON l.id = r.id \
             GROUP BY l.filter_key ORDER BY l.filter_key"
        ),
    )
    .await?;

    let groups = i64::try_from(outcome.rows())?;
    if groups != FIXTURE_GROUPS {
        return Err(format!("expected {FIXTURE_GROUPS} groups, saw {groups}").into());
    }
    let matched: i64 = outcome
        .batches
        .iter()
        .map(|batch| {
            batch
                .column(1)
                .as_any()
                .downcast_ref::<Int64Array>()
                .map(|counts| (0..counts.len()).map(|row| counts.value(row)).sum::<i64>())
                .unwrap_or_default()
        })
        .sum();
    if matched != FIXTURE_ROWS {
        return Err(format!("expected {FIXTURE_ROWS} matched rows, saw {matched}").into());
    }
    if outcome.terminal.row_count != u64::try_from(outcome.rows())? {
        return Err("terminal row count differs from the decoded Arrow frames".into());
    }

    cluster.shutdown().await?;
    Ok(())
}
