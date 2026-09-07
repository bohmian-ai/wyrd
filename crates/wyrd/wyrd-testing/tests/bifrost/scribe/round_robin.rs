use std::sync::{Arc, Mutex};

use vala_bifrost_redux::namespaces::BifrostNamespace;
use vala_bifrost_redux::scribe::admission::AdmissionConfig;
use vala_bifrost_redux::scribe::geometry::ScribeArtifactPolicy;
use vala_bifrost_redux::scribe::routing::SCRIBE_SHARD_COUNT;

use super::support::{
    INGEST_BUSY, append_batch, append_values, read_sql, register_table, sorted_values, span_batch,
    start_scribe_server_with_admission, tenant_client, unique_table, until_admitted,
};

/// Batches each of the two tables sends in the interleaving phase.
///
/// Enough distinct batch identities that both tables reach most of the sixteen
/// lanes, so "the same lanes serve both" is an observation rather than an
/// accident of two routes landing together, and enough turns that a scheduler
/// which favours one class has room to show it.
const BATCHES_PER_TABLE: usize = 24;
/// Rows in one batch, identical for both tables.
const ROWS_PER_BATCH: usize = 64;
/// The canonical lazy built-in this owner schedules against a dynamic table.
const SYSTEM_TABLE: &str = "vala.traces.spans";
/// Consecutive acknowledgements one table may take while its peer still waits.
///
/// One is what strict rotation produces: a refused table keeps a live demand
/// record, and an incumbent may not reacquire the vector ahead of it. Two is
/// allowed only for the turn a table takes before its peer has ever been
/// refused, which is the one moment no queue exists yet.
const MAX_CONSECUTIVE_TURNS: usize = 2;

/// A built-in table is a scheduling peer, not a privileged writer.
///
/// Scribe serves engine-owned built-ins and tenant-registered dynamic tables
/// through one hierarchical scheduler on one fixed lane set. The failure this
/// guards against is a scheduler that quietly ranks them: system telemetry that
/// pre-empts customer ingest, or customer ingest that starves the telemetry the
/// operator needs to see it happening. Either way both tables still return
/// correct rows, so the only evidence is which table is given the next turn
/// while the other is waiting for it.
///
/// The pod is started with its Scribe child budget set to exactly one complete
/// lifecycle vector, because a turn is only observable when there is one to
/// give. Contention is placed with the production ingest barrier rather than
/// with timing: it holds one already-admitted request at the seam where it owns
/// its reserve and has not yet touched the WAL or a shard mailbox.
///
/// Equal demand is the rest of the design: both tables receive the same batch
/// count with the same row count, so any asymmetry in turns taken, lanes
/// reached, or bytes held is the scheduler's decision.
///
/// # Panics
///
/// Panics when the pod does not derive a single-vector ceiling, when the
/// over-share dynamic table does not push the built-in into typed pressure,
/// when the released vector goes back to the incumbent ahead of the waiting
/// built-in, when either table takes more consecutive turns than rotation
/// allows, when either table's own batches are acknowledged out of order, when
/// the two do not share the one fixed lane set, when one table's ownership
/// crowds out the other's, or when either table loses a row across publication.
#[tokio::test]
#[ignore = "requires Postgres and object storage"]
async fn scribe_system_and_dynamic_tables_are_round_robin_equal() {
    let admission = AdmissionConfig {
        scribe_memory_limit_bytes: Some(
            ScribeArtifactPolicy::default().minimum_scribe_memory_bytes(),
        ),
        ..AdmissionConfig::default()
    };
    let server = start_scribe_server_with_admission(admission).await;
    assert_eq!(
        server
            .scribe_ownership_ceiling_for_test()
            .expect("the pod reports its derived ownership ceiling"),
        1,
        "a turn is only observable on a pod whose measured capacity completes \
         exactly one table at a time"
    );

    let tenant = server.data_tenant_id();
    server
        .ensure_traces_spans_table_for_test(tenant)
        .await
        .expect("the built-in traces table is provisioned");
    let dynamic_name = unique_table("round_robin");
    let dynamic_table =
        register_table(&server, tenant, BifrostNamespace::Datasets, &dynamic_name).await;
    let client = Arc::new(tenant_client(&server, tenant).await);

    // 1. The dynamic table borrows all the capacity available to its tenant
    //    while no peer is waiting for any of it.
    let held: Vec<i64> = (0..ROWS_PER_BATCH as i64).collect();
    let stall = server
        .stall_next_bifrost_write()
        .expect("the production ingest barrier is installable");
    let borrowed = tokio::spawn({
        let client = Arc::clone(&client);
        let table = dynamic_table.clone();
        let rows = held.clone();
        async move { append_values(&client, &table, uuid::Uuid::now_v7(), &rows).await }
    });
    stall.wait_entered().await;
    assert_eq!(
        server
            .scribe_contention_totals_for_test()
            .expect("the contention registry is readable")
            .live_vectors(),
        1,
        "an uncontended dynamic table must be lent the whole idle vector"
    );

    // 2. A real built-in request under that pressure is refused with the stable
    //    retryable refusal rather than silently queued behind the incumbent.
    let system_pressure = append_batch(
        &client,
        SYSTEM_TABLE,
        uuid::Uuid::now_v7(),
        &span_batch(&held),
    )
    .await
    .expect_err("a built-in table must feel the same pressure a dynamic table causes");
    assert_eq!(
        system_pressure.code(),
        INGEST_BUSY,
        "a queued built-in must be told to retry, not have its work absorbed: {system_pressure:?}"
    );

    // 3. The released vector goes to the built-in that waited, not back to the
    //    dynamic incumbent that had just been over its share.
    stall.release();
    borrowed
        .await
        .expect("the barrier-held request task completes")
        .expect("the barrier-held request is acknowledged once released");
    let reacquisition = append_values(&client, &dynamic_table, uuid::Uuid::now_v7(), &held)
        .await
        .expect_err("the dynamic incumbent must not reacquire the vector the built-in is owed");
    assert_eq!(
        reacquisition.code(),
        INGEST_BUSY,
        "an incumbent held behind a queued peer is backpressure, not a fault: {reacquisition:?}"
    );
    append_batch(
        &client,
        SYSTEM_TABLE,
        uuid::Uuid::now_v7(),
        &span_batch(&held),
    )
    .await
    .expect("the built-in that waited must be admitted on the released vector");

    // 4. Both tables continue for many rounds. The order in which the pod
    //    acknowledges them is the only record of whose turn it kept giving
    //    away, so it is the record this phase asserts on.
    let acknowledgements = Arc::new(Mutex::new(Vec::<Turn>::new()));
    let mut expected_dynamic: Vec<i64> = held.clone();
    let mut expected_system: Vec<i64> = held.clone();
    let mut dynamic_routes: Vec<usize> = Vec::with_capacity(BATCHES_PER_TABLE);
    let mut system_routes: Vec<usize> = Vec::with_capacity(BATCHES_PER_TABLE);
    let system_ref = vala_bifrost_redux::catalog::TableRef::new(BifrostNamespace::Traces, "spans");
    let dynamic_ref =
        vala_bifrost_redux::catalog::TableRef::new(BifrostNamespace::Datasets, &dynamic_name);
    let mut dynamic_batches = Vec::with_capacity(BATCHES_PER_TABLE);
    let mut system_batches = Vec::with_capacity(BATCHES_PER_TABLE);
    for batch in 0..BATCHES_PER_TABLE {
        let first = ((batch + 1) * ROWS_PER_BATCH) as i64;
        let rows: Vec<i64> = (first..first + ROWS_PER_BATCH as i64).collect();
        let dynamic_batch_id = uuid::Uuid::now_v7();
        let system_batch_id = uuid::Uuid::now_v7();
        dynamic_routes.push(vala_bifrost_redux::scribe::routing::shard_for(
            tenant,
            &dynamic_ref,
            dynamic_batch_id,
        ));
        system_routes.push(vala_bifrost_redux::scribe::routing::shard_for(
            tenant,
            &system_ref,
            system_batch_id,
        ));
        expected_dynamic.extend_from_slice(&rows);
        expected_system.extend_from_slice(&rows);
        dynamic_batches.push((dynamic_batch_id, rows.clone()));
        system_batches.push((system_batch_id, rows));
    }

    let dynamic_turns = tokio::spawn({
        let client = Arc::clone(&client);
        let table = dynamic_table.clone();
        let log = Arc::clone(&acknowledgements);
        async move {
            for (ordinal, (batch_id, rows)) in dynamic_batches.into_iter().enumerate() {
                retry_until_admitted(TableClass::Dynamic, ordinal, &log, || {
                    let client = Arc::clone(&client);
                    let table = table.clone();
                    let rows = rows.clone();
                    async move { append_values(&client, &table, batch_id, &rows).await }
                })
                .await;
            }
        }
    });
    let system_turns = tokio::spawn({
        let client = Arc::clone(&client);
        let log = Arc::clone(&acknowledgements);
        async move {
            for (ordinal, (batch_id, rows)) in system_batches.into_iter().enumerate() {
                retry_until_admitted(TableClass::System, ordinal, &log, || {
                    let client = Arc::clone(&client);
                    let batch = span_batch(&rows);
                    async move { append_batch(&client, SYSTEM_TABLE, batch_id, &batch).await }
                })
                .await;
            }
        }
    });
    dynamic_turns.await.expect("dynamic table turns complete");
    system_turns.await.expect("built-in table turns complete");
    let turns = acknowledgements
        .lock()
        .expect("the acknowledgement log is readable")
        .clone();
    assert_fifo_within_each_table(&turns);
    assert_rotation_between_tables(&turns);

    // 5. Ownership, lane sharing and exact public read-back for both classes.
    let loaded = server
        .scribe_inspection_snapshot()
        .expect("Scribe ownership is inspectable");
    let mut system_bytes = 0_usize;
    let mut dynamic_bytes = 0_usize;
    for bucket in &loaded.memory_by_bucket {
        let bytes = bucket.writable_bytes + bucket.immutable_bytes;
        if bucket.seal_key.table.name == "spans" {
            system_bytes += bytes;
        } else if bucket.seal_key.table.name == dynamic_name {
            dynamic_bytes += bytes;
        }
    }
    assert!(
        system_bytes > 0 && dynamic_bytes > 0,
        "both a built-in and a dynamic table must own the rows they acknowledged: system {system_bytes}, dynamic {dynamic_bytes}"
    );
    let smaller = system_bytes.min(dynamic_bytes);
    let larger = system_bytes.max(dynamic_bytes);
    assert!(
        larger <= smaller.saturating_mul(8),
        "equal demand must not produce a lopsided split between a built-in and a dynamic table: system {system_bytes}, dynamic {dynamic_bytes}"
    );

    let lane_set = |routes: &[usize]| {
        let mut lanes = [false; SCRIBE_SHARD_COUNT];
        for lane in routes {
            lanes[*lane] = true;
        }
        lanes
    };
    let system_lanes = lane_set(&system_routes);
    let dynamic_lanes = lane_set(&dynamic_routes);
    let shared: usize = (0..SCRIBE_SHARD_COUNT)
        .filter(|lane| system_lanes[*lane] && dynamic_lanes[*lane])
        .count();
    assert!(
        shared >= SCRIBE_SHARD_COUNT / 2,
        "the two table classes must share the one fixed lane set, not split it: {shared} shared lanes of {SCRIBE_SHARD_COUNT}"
    );
    let unowned: Vec<usize> = (0..SCRIBE_SHARD_COUNT)
        .filter(|lane| {
            (system_lanes[*lane] || dynamic_lanes[*lane]) && loaded.memory_by_shard[*lane] == 0
        })
        .collect();
    assert!(
        unowned.is_empty(),
        "every lane the two tables routed to must own their rows; empty lanes {unowned:?} of {:?}",
        loaded.memory_by_shard
    );

    server
        .flush_bifrost()
        .await
        .expect("the staged members publish");
    expected_dynamic.sort_unstable();
    expected_system.sort_unstable();
    assert_eq!(
        sorted_values(&client, &dynamic_table).await,
        expected_dynamic,
        "the dynamic table must read back exactly the rows it acknowledged"
    );
    let mut system_read = read_sql(
        &client,
        &format!("SELECT duration_nano AS value FROM {SYSTEM_TABLE}"),
    )
    .await;
    system_read.sort_unstable();
    assert_eq!(
        system_read, expected_system,
        "the built-in table must read back exactly the rows it acknowledged"
    );

    server.shutdown().await.expect("the server drains cleanly");
}

/// Which of the two scheduling peers took one acknowledged turn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TableClass {
    /// The engine-owned built-in `vala.traces.spans`.
    System,
    /// The tenant-registered dynamic table.
    Dynamic,
}

/// One acknowledged turn, in the order the pod granted it.
///
/// The pair is the whole evidence base for this owner: which class was admitted
/// and which of that class's ordered batches it was. Nothing here is derived
/// from a routing hash or a configured weight.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Turn {
    /// The scheduling peer that was admitted.
    class: TableClass,
    /// The zero-based position of the batch within that peer's own sequence.
    ordinal: usize,
}

/// Sends one batch until the pod admits it, recording the turn it was given.
///
/// Waiting is the shared bounded rule in [`until_admitted`]: a refusal backs off
/// geometrically and a batch that is still refused at the shared deadline fails
/// the case as starvation. Nothing here treats elapsed time as evidence — the
/// order of the recorded turns is the whole measurement — but a table that
/// spins on refusals issues attempts in proportion to how often it is
/// scheduled, which would make request throughput an input to a fairness
/// result.
///
/// # Panics
///
/// Panics when a batch is refused for anything other than capacity pressure, or
/// when it is still refused at the shared admission deadline, which is
/// starvation rather than a turn that has not come round yet.
async fn retry_until_admitted<F, Fut>(
    class: TableClass,
    ordinal: usize,
    log: &Mutex<Vec<Turn>>,
    send: F,
) where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<(), wyrd_spec::error::WyrdError>>,
{
    until_admitted(&format!("{class:?} batch {ordinal}"), send).await;
    log.lock()
        .expect("the acknowledgement log is writable")
        .push(Turn { class, ordinal });
}

/// Asserts each table's own batches were acknowledged in the order they were sent.
///
/// # Panics
///
/// Panics when either table's acknowledged ordinals are not the contiguous
/// ascending sequence that table submitted.
fn assert_fifo_within_each_table(turns: &[Turn]) {
    for class in [TableClass::System, TableClass::Dynamic] {
        let ordinals: Vec<usize> = turns
            .iter()
            .filter(|turn| turn.class == class)
            .map(|turn| turn.ordinal)
            .collect();
        let expected: Vec<usize> = (0..BATCHES_PER_TABLE).collect();
        assert_eq!(
            ordinals, expected,
            "{class:?} must be acknowledged in the order it submitted, not reordered by the scheduler"
        );
    }
}

/// Asserts neither table kept the vector while the other was still waiting.
///
/// Only the prefix in which both tables still have work outstanding is
/// examined: once one table has finished its whole demand, the other is
/// entitled to every remaining turn and a long run there is correct.
///
/// # Panics
///
/// Panics when one table takes more than [`MAX_CONSECUTIVE_TURNS`] turns in a
/// row while its peer still has batches to send.
fn assert_rotation_between_tables(turns: &[Turn]) {
    let last_contested = turns
        .iter()
        .enumerate()
        .filter(|(_, turn)| turn.ordinal + 1 == BATCHES_PER_TABLE)
        .map(|(index, _)| index)
        .min()
        .unwrap_or(turns.len());
    let mut run = 0_usize;
    let mut previous: Option<TableClass> = None;
    for turn in turns.iter().take(last_contested) {
        run = if previous == Some(turn.class) {
            run + 1
        } else {
            1
        };
        previous = Some(turn.class);
        assert!(
            run <= MAX_CONSECUTIVE_TURNS,
            "{:?} took {run} turns in a row while its peer was still waiting: {turns:?}",
            turn.class
        );
    }
}
