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

use arrow::array::{Int64Array, StringArray};
use arrow::datatypes::{DataType, Field, Schema};
use arrow::ipc::writer::StreamWriter;
use arrow::record_batch::RecordBatch;
use futures_util::StreamExt as _;
use vala_bifrost_redux::oracle::QueryIpcDecoder;
use vala_bifrost_redux::oracle::analytical::DataFusionQueryId;
use vala_bifrost_redux::oracle::analytical_supervisor::AnalyticalAttemptKey;
use vala_bifrost_redux::oracle::telemetry::AnalyticalAttemptOutcome;
use vala_sdk::BifrostGrpcTransport;
use wyrd_client::WyrdClient;
use wyrd_spec::DataTenantId;
use wyrd_spec::vala::api::{
    BifrostQueryRequest, FreshnessPolicy, QueryStreamFrame, VisibilityMode,
};
use wyrd_testing::WyrdTestServer;
use wyrd_testing::bifrost::{BifrostClusterSpec, WyrdTestCluster};

use crate::support::*;

/// One drained inactive Analytical result.
struct AnalyticalOutcome {
    /// Every decoded Arrow batch the distributed plan produced, in order.
    batches: Vec<RecordBatch>,
}

impl AnalyticalOutcome {
    /// Total rows across every decoded batch.
    fn rows(&self) -> usize {
        self.batches.iter().map(RecordBatch::num_rows).sum()
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
    // Drained rather than dropped: a stream that ended without a terminal did
    // not settle its graph, whatever its rows say.
    terminal.ok_or("inactive analytical stream emitted no terminal frame")?;
    Ok(AnalyticalOutcome { batches })
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
        .insert_batch(table, ipc_row(id, filter_key))
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

/// Builds one published-only strict request with the journey's deadline.
fn request(sql: &str) -> BifrostQueryRequest {
    BifrostQueryRequest {
        sql: sql.to_owned(),
        visibility: VisibilityMode::PublishedOnly,
        freshness: FreshnessPolicy::Strict,
        deadline_ms: Some(30_000),
    }
}

/// Cancellation, an expired deadline, and a consumer that walks away each leave
/// all six nodes owning no Analytical graph, attempt, or reservation.
#[tokio::test]
#[ignore = "requires the serialized Postgres-backed Oracle journey lane"]
async fn pg_inactive_analytical_cancel_deadline_and_slow_consumer_leave_six_clean_nodes() {
    prove_terminal_cleanup()
        .await
        .expect("inactive analytical terminal cleanup journey");
}

/// Drives the three abnormal terminals and asserts a clean cluster after each.
///
/// The assertion is made after every terminal rather than once at the end, so a
/// leak is attributed to the terminal that caused it instead of to whichever
/// one happened to run last.
///
/// # Errors
///
/// Returns a cluster, execution, or retained-ownership error.
async fn prove_terminal_cleanup() -> Result<(), JourneyError> {
    let cluster = WyrdTestCluster::start_spec(BifrostClusterSpec::six_capacity()).await?;
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
    let sql = format!("SELECT id, filter_key FROM vala.bifrost.{table} ORDER BY id");

    // Explicit cancellation: the owner signals and drains under its own bound.
    let stream = engine
        .query_sql_inactive_analytical(query_context(tenant)?, request(&sql), attempt_context())
        .await?;
    stream.cancel().await;
    await_clean_nodes(&cluster).await?;

    // An expired deadline: the attempt never gets to produce a full result.
    let expired = engine
        .query_sql_inactive_analytical(
            query_context(tenant)?,
            BifrostQueryRequest {
                deadline_ms: Some(1),
                ..request(&sql)
            },
            attempt_context(),
        )
        .await;
    if let Ok(mut stream) = expired {
        while stream.frames.next().await.is_some() {}
    }
    await_clean_nodes(&cluster).await?;

    // A consumer that walks away mid-stream: the frames owner is dropped
    // without a terminal, which is the case a Drop-only cleanup would miss.
    let mut stream = engine
        .query_sql_inactive_analytical(query_context(tenant)?, request(&sql), attempt_context())
        .await?;
    let _first = stream.frames.next().await;
    drop(stream);
    await_clean_nodes(&cluster).await?;

    cluster.shutdown().await?;
    Ok(())
}

/// A settled attempt and a sibling graph under the same public query can
/// neither settle, cancel, nor attach work to a live attempt.
#[tokio::test]
#[ignore = "requires the serialized Postgres-backed Oracle journey lane"]
async fn pg_inactive_analytical_stale_attempt_and_sibling_graph_cannot_emit_or_cancel() {
    prove_stale_and_sibling_fencing()
        .await
        .expect("inactive analytical fencing journey");
}

/// Drives the stale-attempt and sibling-graph refusals against a live attempt.
///
/// Every refusal here is asserted against a supervisor that is simultaneously
/// holding one legitimately live attempt, so a refusal cannot be an artifact of
/// an empty supervisor refusing everything.
///
/// # Errors
///
/// Returns a cluster, lease, or assertion error.
async fn prove_stale_and_sibling_fencing() -> Result<(), JourneyError> {
    let cluster = WyrdTestCluster::start_spec(BifrostClusterSpec::six_capacity()).await?;
    let tenant = cluster.data_tenant_id();
    let table = seed_table(&cluster, "analytical_fencing").await?;
    let query_server = cluster.server(0).ok_or("missing query node")?;
    let engine = Arc::clone(
        query_server
            .state()
            .bifrost_query()
            .ok_or("query node composed no Oracle")?
            .engine(),
    );
    let handle = Arc::clone(
        engine
            .analytical_execution()
            .ok_or("Oracle composed no Analytical handle")?,
    );
    let supervisor = Arc::clone(handle.supervisor());
    let sql = format!("SELECT id FROM vala.bifrost.{table} ORDER BY id");

    let attempt = attempt_context();
    let (_session, ownership) = engine
        .lease_inactive_analytical_attempt(query_context(tenant)?, request(&sql), &attempt)
        .await?;
    let live = ownership.key();

    // A stage of this same graph that was never admitted: it can settle
    // nothing and attach nothing. A graph has one attempt, so an unadmitted
    // sibling stage — not a successor ordinal — is what "stale" now means.
    let stale = AnalyticalAttemptKey::new(
        live.public_query_id,
        live.datafusion_query_id,
        vala_bifrost_redux::oracle::analytical_supervisor::StageId::new(7),
        live.task,
        live.attempt,
    );
    if supervisor
        .finish_attempt(stale, AnalyticalAttemptOutcome::Cancelled)
        .await
        .is_ok()
    {
        return Err("a stale attempt settled work it never owned".into());
    }
    if supervisor
        .retain_driver(stale, tokio::spawn(async {}))
        .is_ok()
    {
        return Err("a stale attempt attached a driver".into());
    }

    // A sibling graph under the same public query: a different private plan
    // identity, which must not resolve, settle, or attach to this graph.
    let sibling = AnalyticalAttemptKey::new(
        live.public_query_id,
        DataFusionQueryId::from_uuid(uuid::Uuid::now_v7()),
        live.stage,
        live.task,
        live.attempt,
    );
    if supervisor.graph_runtime(sibling.graph()).is_ok() {
        return Err("a sibling graph resolved this graph's query-owned runtime".into());
    }
    if supervisor
        .finish_attempt(sibling, AnalyticalAttemptOutcome::Cancelled)
        .await
        .is_ok()
    {
        return Err("a sibling graph settled this graph's attempt".into());
    }
    if supervisor
        .retain_driver(sibling, tokio::spawn(async {}))
        .is_ok()
    {
        return Err("a sibling graph attached a driver to this graph".into());
    }

    // The refusals changed nothing: the real attempt is still live and still
    // the only one, and its cancellation child was never signalled.
    if supervisor.live_attempts()? != 1 {
        return Err("a refused fencing attempt disturbed live supervision".into());
    }
    if ownership.cancellation().is_cancelled() {
        return Err("a refused fencing attempt cancelled the live attempt".into());
    }

    ownership.settle(AnalyticalAttemptOutcome::Success).await?;
    await_clean_nodes(&cluster).await?;
    cluster.shutdown().await?;
    Ok(())
}

/// A selective predicate and a narrow projection prune the distributed physical
/// read, the exchange carries real follower result data, and the attempt's spill
/// is confined to this node's own bounded scratch.
#[tokio::test]
#[ignore = "requires the serialized Postgres-backed Oracle journey lane"]
async fn pg_inactive_analytical_raw_sql_proves_pushdown_exchange_and_qualified_spill() {
    prove_pushdown_exchange_and_spill()
        .await
        .expect("inactive analytical pushdown, exchange, and spill journey");
}

/// Drives the pushdown, exchange, and qualified-spill proofs.
///
/// The spill half is exercised through the attempt's own disk manager rather
/// than by starving a sort into spilling. That is deliberate: what this journey
/// must prove is that a spilling operator's files are *qualified* — bounded by
/// the query's admitted scratch share and confined to this node's disposable
/// Oracle-owned child — and starving a sort proves only that some spill
/// happened, on a threshold `DataFusion` owns and may retune. The exact-share
/// refusal itself is proved in the spill owner's own tests.
///
/// # Errors
///
/// Returns a cluster, execution, telemetry, or assertion error.
async fn prove_pushdown_exchange_and_spill() -> Result<(), JourneyError> {
    let cluster = WyrdTestCluster::start_spec(BifrostClusterSpec::six_capacity()).await?;
    let tenant = cluster.data_tenant_id();
    let table = seed_table(&cluster, "analytical_pushdown").await?;
    let query_server = cluster.server(0).ok_or("missing query node")?;
    let engine = Arc::clone(
        query_server
            .state()
            .bifrost_query()
            .ok_or("query node composed no Oracle")?
            .engine(),
    );

    let broad_checkpoint = cluster
        .telemetry()
        .checkpoint()
        .map_err(|e| e.to_string())?;
    let broad = execute_inactive_analytical(
        query_server,
        tenant,
        &format!("SELECT id, filter_key, unused_payload FROM vala.bifrost.{table}"),
    )
    .await?;
    let broad_delta = cluster
        .telemetry()
        .delta_since(&broad_checkpoint)
        .map_err(|e| e.to_string())?;
    let broad_bytes = sum_metric(&broad_delta, "oracle_query_bytes_scanned_total");

    let narrow_checkpoint = cluster
        .telemetry()
        .checkpoint()
        .map_err(|e| e.to_string())?;
    let narrow = execute_inactive_analytical(
        query_server,
        tenant,
        &format!("SELECT id FROM vala.bifrost.{table} WHERE filter_key = 'group_0'"),
    )
    .await?;
    let narrow_delta = cluster
        .telemetry()
        .delta_since(&narrow_checkpoint)
        .map_err(|e| e.to_string())?;
    let narrow_bytes = sum_metric(&narrow_delta, "oracle_query_bytes_scanned_total");

    if i64::try_from(broad.rows())? != FIXTURE_ROWS {
        return Err(format!(
            "broad scan expected {FIXTURE_ROWS} rows, saw {}",
            broad.rows()
        )
        .into());
    }
    let expected_narrow = FIXTURE_ROWS / FIXTURE_GROUPS;
    if i64::try_from(narrow.rows())? != expected_narrow {
        return Err(format!(
            "selective scan expected {expected_narrow} rows, saw {}",
            narrow.rows()
        )
        .into());
    }
    if narrow_bytes >= broad_bytes {
        return Err(format!(
            "predicate and projection pushdown moved no physical bytes: \
             broad={broad_bytes} narrow={narrow_bytes}"
        )
        .into());
    }
    // The exchange is read from a grouped statement of its own rather than from
    // either scan above. A projection scan of one cut is entirely
    // leader-executable, so the single planner returns a normal root for it and
    // the query settles Interactive; only a statement carrying a shuffle
    // produces the follower stages whose result data this claim is about. The
    // same seeded cut is read either way, so this is the same physical source.
    let exchange_checkpoint = cluster
        .telemetry()
        .checkpoint()
        .map_err(|e| e.to_string())?;
    execute_inactive_analytical(
        query_server,
        tenant,
        &format!(
            "SELECT filter_key, count(*) AS matched FROM vala.bifrost.{table} \
             GROUP BY filter_key"
        ),
    )
    .await?;
    let exchange_delta = cluster
        .telemetry()
        .delta_since(&exchange_checkpoint)
        .map_err(|e| e.to_string())?;
    let exchange_batches = sum_metric(
        &exchange_delta,
        "bifrost_oracle_analytical_exchange_batches_total",
    );
    let exchange_bytes = sum_metric(
        &exchange_delta,
        "bifrost_oracle_analytical_exchange_bytes_total",
    );
    if exchange_batches <= 0.0 || exchange_bytes <= 0.0 {
        return Err(format!(
            "distributed exchange carried no follower result data: \
             batches={exchange_batches} bytes={exchange_bytes}"
        )
        .into());
    }

    // Qualified spill: the attempt's own runtime, not a process default.
    let attempt = attempt_context();
    let sql = format!("SELECT id FROM vala.bifrost.{table} ORDER BY id");
    let (session, ownership) = engine
        .lease_inactive_analytical_attempt(query_context(tenant)?, request(&sql), &attempt)
        .await?;
    let spill_root = engine.analytical_spill_root().to_path_buf();
    let scratch = session
        .runtime_env()
        .disk_manager
        .create_tmp_file("analytical journey")?;
    let path = scratch
        .path()
        .ok_or("attempt spill file reported no path")?
        .to_path_buf();
    if !path.starts_with(&spill_root) {
        return Err(format!(
            "attempt spill escaped this node's owned scratch: {} is not under {}",
            path.display(),
            spill_root.display()
        )
        .into());
    }
    drop(scratch);
    ownership.settle(AnalyticalAttemptOutcome::Success).await?;

    await_clean_nodes(&cluster).await?;
    cluster.shutdown().await?;
    Ok(())
}

/// Production telemetry covers the whole Analytical hot path, and every
/// in-flight gauge returns to its baseline on both a drained and a cancelled
/// terminal.
#[tokio::test]
#[ignore = "requires the serialized Postgres-backed Oracle journey lane"]
async fn pg_inactive_analytical_production_telemetry_covers_every_hot_path() {
    prove_production_telemetry()
        .await
        .expect("inactive analytical production telemetry journey");
}

/// Drives one drained and one cancelled attempt and reads the production
/// telemetry both left behind.
///
/// Every assertion here is made against the production `BifrostTelemetryCapture`
/// reading the server's own Prometheus handle and trace capture — there is no
/// test-only recorder and no Analytical-specific telemetry owner. Identities
/// stay in scrubbed spans rather than metric labels, so the journey asserts
/// closed label domains on the metrics and identity on the spans.
///
/// # Errors
///
/// Returns a cluster, execution, telemetry, or assertion error.
async fn prove_production_telemetry() -> Result<(), JourneyError> {
    let cluster = WyrdTestCluster::start_spec(BifrostClusterSpec::six_capacity()).await?;
    let tenant = cluster.data_tenant_id();
    let table = seed_table(&cluster, "analytical_telemetry").await?;
    let query_server = cluster.server(0).ok_or("missing query node")?;
    let engine = Arc::clone(
        query_server
            .state()
            .bifrost_query()
            .ok_or("query node composed no Oracle")?
            .engine(),
    );
    let sql = format!(
        "SELECT filter_key, count(*) AS rows FROM vala.bifrost.{table} GROUP BY filter_key"
    );

    let checkpoint = cluster
        .telemetry()
        .checkpoint()
        .map_err(|e| e.to_string())?;
    let drained = execute_inactive_analytical(query_server, tenant, &sql).await?;
    if i64::try_from(drained.rows())? != FIXTURE_GROUPS {
        return Err(format!(
            "grouped aggregate expected {FIXTURE_GROUPS} rows, saw {}",
            drained.rows()
        )
        .into());
    }
    let mut cancelled = engine
        .query_sql_inactive_analytical(query_context(tenant)?, request(&sql), attempt_context())
        .await?;
    let _first = cancelled.frames.next().await;
    cancelled.cancel().await;
    await_clean_nodes(&cluster).await?;
    let delta = cluster
        .telemetry()
        .delta_since(&checkpoint)
        .map_err(|e| e.to_string())?;

    // Stage authorization and dispatch: both halves of the protocol are
    // separately observable, and nothing was refused.
    for operation in ["set_plan", "execute_task"] {
        let authorized = labelled_metric(
            &delta,
            "bifrost_oracle_analytical_stage_authority_total",
            &[("operation", operation), ("outcome", "authorized")],
        );
        if authorized <= 0.0 {
            return Err(format!("no authorized {operation} stage operation was observed").into());
        }
        let dispatched = labelled_metric(
            &delta,
            "bifrost_oracle_analytical_stage_operations_total",
            &[("operation", operation)],
        );
        if dispatched <= 0.0 {
            return Err(format!("no dispatched {operation} stage operation was observed").into());
        }
        for outcome in ["signature", "binding", "body", "expired", "replay"] {
            let refused = labelled_metric(
                &delta,
                "bifrost_oracle_analytical_stage_authority_total",
                &[("operation", operation), ("outcome", outcome)],
            );
            if refused != 0.0 {
                return Err(format!(
                    "an authorized journey recorded a {outcome} refusal on {operation}"
                )
                .into());
            }
        }
    }

    // Exchange: follower result data actually crossed the network.
    if sum_metric(&delta, "bifrost_oracle_analytical_exchange_batches_total") <= 0.0
        || sum_metric(&delta, "bifrost_oracle_analytical_exchange_bytes_total") <= 0.0
    {
        return Err("the distributed exchange published no transfer telemetry".into());
    }

    // Terminal settlement: both terminals are attributed to their own outcome,
    // and no attempt settled as an unexplained failure.
    let succeeded = labelled_metric(
        &delta,
        "bifrost_oracle_analytical_attempts_total",
        &[("outcome", "success")],
    );
    let cancelled_attempts = labelled_metric(
        &delta,
        "bifrost_oracle_analytical_attempts_total",
        &[("outcome", "cancelled")],
    );
    if succeeded <= 0.0 || cancelled_attempts <= 0.0 {
        return Err(format!(
            "attempt terminals were not attributed: success={succeeded} cancelled={cancelled_attempts}"
        )
        .into());
    }

    // Resource release: every in-flight gauge is back at its baseline.
    for gauge in [
        "bifrost_oracle_analytical_attempts_active",
        "bifrost_oracle_analytical_exchanges_active",
        "oracle_queries_active",
        "oracle_queries_queued",
    ] {
        let retained = final_gauge(&delta, gauge);
        if retained != 0.0 {
            return Err(format!("{gauge} did not return to baseline, saw {retained}").into());
        }
    }

    // Correlated traces: the leader query span and the attempt span are both
    // published, and the attempt span carries its identities as scrubbed span
    // fields rather than as metric labels.
    let attempts = delta
        .spans
        .iter()
        .filter(|span| span.name == "bifrost.oracle.analytical.attempt")
        .collect::<Vec<_>>();
    if attempts.is_empty() {
        return Err("no Analytical attempt span reached the production trace capture".into());
    }
    if !attempts.iter().any(|span| {
        span.attributes.contains_key("public_query_id")
            && span.attributes.contains_key("datafusion_query_id")
            && span
                .attributes
                .get("outcome")
                .is_some_and(|o| o == "success")
    }) {
        return Err("no attempt span carried both identities and a success outcome".into());
    }
    if !delta
        .spans
        .iter()
        .any(|span| span.name == "bifrost.oracle.query")
    {
        return Err("the Analytical journey published no leader query span".into());
    }

    cluster.shutdown().await?;
    Ok(())
}

/// Sums one production metric family restricted to an exact label set.
///
/// Analytical telemetry keeps every label closed, so a journey asserting on a
/// specific operation or outcome must select by label rather than by family
/// alone; a family-wide sum would let a refusal masquerade as an authorization.
fn labelled_metric(
    delta: &wyrd_testing::bifrost::telemetry::BifrostTelemetryDelta,
    family: &str,
    labels: &[(&str, &str)],
) -> f64 {
    delta
        .metrics
        .iter()
        .filter(|sample| sample.family == family)
        .filter(|sample| {
            labels
                .iter()
                .all(|(key, value)| sample.labels.get(*key).is_some_and(|held| held == value))
        })
        .map(|sample| sample.value)
        .sum()
}

/// Sums one production gauge family's closing values across every label set.
///
/// Read from the render that closed the capture window rather than from the
/// sampled maxima: what a terminal-cleanup assertion needs is where the gauge
/// came to rest, not how high it went while the query was in flight.
fn final_gauge(
    delta: &wyrd_testing::bifrost::telemetry::BifrostTelemetryDelta,
    family: &str,
) -> f64 {
    delta
        .gauge_final
        .iter()
        .filter(|sample| sample.family == family)
        .map(|sample| sample.value)
        .sum()
}
