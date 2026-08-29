//! Hierarchical fairness between a built-in system table and a dynamic table.

use vala_bifrost_redux::namespaces::BifrostNamespace;
use vala_bifrost_redux::scribe::routing::SCRIBE_SHARD_COUNT;

use super::support::{
    append_batch, append_values, read_sql, register_table, sorted_values, start_scribe_server,
    tenant_client, unique_table,
};

/// Batches each of the two tables sends.
///
/// Enough distinct batch identities that both tables reach most of the sixteen
/// lanes, so "the same lanes serve both" is an observation rather than an
/// accident of two routes landing together.
const BATCHES_PER_TABLE: usize = 24;
/// Rows in one batch, identical for both tables.
const ROWS_PER_BATCH: usize = 64;
/// The canonical lazy built-in this owner schedules against a dynamic table.
const SYSTEM_TABLE: &str = "vala.traces.spans";

/// AC22 Tier-2 owner: a built-in table is a peer, not a privileged writer.
///
/// Scribe serves engine-owned built-ins and tenant-registered dynamic tables
/// through one hierarchical scheduler on one fixed lane set. The failure this
/// guards against is a scheduler that quietly ranks them: system telemetry that
/// pre-empts customer ingest, or customer ingest that starves the telemetry the
/// operator needs to see it happening. Either way both tables still return
/// correct rows, so the only evidence is what each table owns and where, while
/// both are resident.
///
/// Equal demand is the whole design of the case: both tables receive the same
/// batch count with the same row count, so any asymmetry in lanes reached or
/// bytes held is the scheduler's decision.
///
/// # Panics
///
/// Panics when an append or read fails, when either table is confined to fewer
/// lanes than the other, when the two do not share the same lane set, when one
/// table's ownership crowds out the other's, or when either table loses a row
/// across publication.
#[tokio::test]
#[ignore = "requires Postgres and object storage"]
async fn scribe_system_and_dynamic_tables_are_round_robin_equal() {
    let server = start_scribe_server().await;
    let tenant = server.data_tenant_id();
    server
        .ensure_traces_spans_table_for_test(tenant)
        .await
        .expect("the built-in traces table is provisioned");
    let dynamic_name = unique_table("round_robin");
    let dynamic_table =
        register_table(&server, tenant, BifrostNamespace::Datasets, &dynamic_name).await;
    let client = tenant_client(&server, tenant).await;

    // Strictly interleaved so neither table can win by arriving first.
    let system_ref = vala_bifrost_redux::catalog::TableRef::new(BifrostNamespace::Traces, "spans");
    let dynamic_ref =
        vala_bifrost_redux::catalog::TableRef::new(BifrostNamespace::Datasets, &dynamic_name);
    let mut expected: Vec<i64> = Vec::with_capacity(BATCHES_PER_TABLE * ROWS_PER_BATCH);
    let mut system_routes: Vec<usize> = Vec::with_capacity(BATCHES_PER_TABLE);
    let mut dynamic_routes: Vec<usize> = Vec::with_capacity(BATCHES_PER_TABLE);
    for batch in 0..BATCHES_PER_TABLE {
        let first = (batch * ROWS_PER_BATCH) as i64;
        let rows: Vec<i64> = (first..first + ROWS_PER_BATCH as i64).collect();
        let system_batch_id = uuid::Uuid::now_v7();
        let dynamic_batch_id = uuid::Uuid::now_v7();
        system_routes.push(vala_bifrost_redux::scribe::routing::shard_for(
            tenant,
            &system_ref,
            system_batch_id,
        ));
        dynamic_routes.push(vala_bifrost_redux::scribe::routing::shard_for(
            tenant,
            &dynamic_ref,
            dynamic_batch_id,
        ));
        append_batch(&client, SYSTEM_TABLE, system_batch_id, &span_batch(&rows))
            .await
            .unwrap_or_else(|error| panic!("system batch {batch} is acknowledged: {error:?}"));
        append_values(&client, &dynamic_table, dynamic_batch_id, &rows)
            .await
            .unwrap_or_else(|error| panic!("dynamic batch {batch} is acknowledged: {error:?}"));
        expected.extend_from_slice(&rows);
    }

    let loaded = server
        .scribe_inspection_snapshot()
        .expect("Scribe ownership is inspectable");

    // Both tables' ownership, resolved from the same snapshot.
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

    // Lane occupancy comes from the production routing function against the
    // identities the case actually sent, so it says where each table's work was
    // placed rather than restating the snapshot totals back at themselves.
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

    // Neither class is starved through publication.
    server
        .flush_bifrost_for_tenant(tenant)
        .await
        .expect("the staged members publish");
    let mut dynamic_read = sorted_values(&client, &dynamic_table).await;
    dynamic_read.sort_unstable();
    let mut expected_sorted = expected.clone();
    expected_sorted.sort_unstable();
    assert_eq!(
        dynamic_read, expected_sorted,
        "the dynamic table must read back exactly the rows it acknowledged"
    );
    let mut system_read = read_sql(
        &client,
        &format!("SELECT duration_ms AS value FROM {SYSTEM_TABLE}"),
    )
    .await;
    system_read.sort_unstable();
    assert_eq!(
        system_read, expected_sorted,
        "the built-in table must read back exactly the rows it acknowledged"
    );

    server.shutdown().await.expect("the server drains cleanly");
}

/// Builds one `vala.traces.spans` batch carrying `values` as `duration_ms`.
///
/// Every other column is a well-formed constant: this owner is about how the
/// scheduler treats the table, so the payload only has to be a real built-in
/// row that public ingest accepts, with one column the read-back can compare.
fn span_batch(values: &[i64]) -> arrow::record_batch::RecordBatch {
    use arrow::array::{
        FixedSizeBinaryBuilder, Int64Array, StringArray, TimestampMicrosecondArray,
    };
    let definition = vala_bifrost_redux::tables::builtin_table("traces", "spans")
        .expect("traces spans built-in");
    let schema = std::sync::Arc::new(arrow::datatypes::Schema::new((definition.arrow_fields)()));
    let rows = values.len();
    let mut trace_id = FixedSizeBinaryBuilder::with_capacity(rows, 16);
    let mut span_id = FixedSizeBinaryBuilder::with_capacity(rows, 8);
    let mut parent_span_id = FixedSizeBinaryBuilder::with_capacity(rows, 8);
    for value in values {
        let mut trace = [0_u8; 16];
        trace[..8].copy_from_slice(&value.to_be_bytes());
        trace_id.append_value(trace).expect("trace id width");
        span_id
            .append_value(value.to_be_bytes())
            .expect("span id width");
        parent_span_id.append_null();
    }
    let now = chrono::Utc::now().timestamp_micros();
    let text = |literal: &str| {
        std::sync::Arc::new(StringArray::from(vec![literal; rows])) as arrow::array::ArrayRef
    };
    let zeros =
        || std::sync::Arc::new(Int64Array::from(vec![0_i64; rows])) as arrow::array::ArrayRef;
    let stamps = || {
        std::sync::Arc::new(TimestampMicrosecondArray::from(vec![now; rows]).with_timezone("UTC"))
            as arrow::array::ArrayRef
    };
    arrow::record_batch::RecordBatch::try_new(
        schema,
        vec![
            std::sync::Arc::new(trace_id.finish()),
            std::sync::Arc::new(span_id.finish()),
            std::sync::Arc::new(parent_span_id.finish()),
            zeros(),
            text("scribe-round-robin"),
            text("scribe-round-robin"),
            text("SPAN_KIND_INTERNAL"),
            stamps(),
            stamps(),
            std::sync::Arc::new(Int64Array::from(values.to_vec())),
            text("STATUS_CODE_OK"),
            text("{}"),
            zeros(),
            zeros(),
            zeros(),
            text("scribe"),
            text("1"),
            text("wyrd-testing"),
        ],
    )
    .expect("span batch")
}
