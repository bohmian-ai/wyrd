//! Bifrost fixed-topology and ownership inspection.

use std::sync::Arc;

use arrow::array::{Int64Array, TimestampMicrosecondArray};
use arrow::datatypes::{DataType, Field, Schema, TimeUnit};
use arrow::record_batch::RecordBatch;
use uuid::Uuid;
use vala_bifrost_redux::catalog::TableRef;
use vala_bifrost_redux::contracts::{Scribe, ScribeAppend};
use vala_bifrost_redux::namespaces::BifrostNamespace;
use vala_bifrost_redux::schema::SchemaFingerprint;
use vala_bifrost_redux::scribe::routing::{SCRIBE_SHARD_COUNT, shard_for};
use wyrd_runtime::{PermissionSet, Principal, PrincipalKind};
use wyrd_spec::auth::PrincipalId;
use wyrd_spec::request_id::RequestId;
use wyrd_testing::bifrost::BifrostHarness;

/// Finds a table name whose shard-routing probe (using `Uuid::nil` as the stable
/// batch-id sentinel) lands on `target_shard`.
///
/// After T35 (batch-spread routing), the actual runtime shard depends on both
/// the table and the per-request `batch_id`; this helper uses a nil UUID as a
/// deterministic probe so the fixture can construct a predictable topology for
/// inspection purposes.  Real appends use per-request UUIDs and will spread
/// across shards regardless.
fn table_for_shard(
    tenant: wyrd_spec::DataTenantId,
    target_shard: usize,
    prefix: &str,
    candidate: &mut usize,
) -> TableRef {
    let probe = Uuid::nil();
    loop {
        let table = TableRef::new(
            BifrostNamespace::Bifrost,
            format!("{prefix}_{}", *candidate),
        );
        *candidate += 1;
        if shard_for(tenant, &table, probe) == target_shard {
            return table;
        }
    }
}

#[tokio::test]
#[ignore = "requires the embedded Postgres and object-store fixtures"]
async fn scribe_owner_fixed_topology_and_memory_reconciliation() {
    let harness = BifrostHarness::start(1, 1)
        .await
        .expect("one-pod Bifrost harness");
    let snapshots = harness
        .inspection_snapshots()
        .expect("Scribe inspection snapshots");
    let snapshot = snapshots.first().expect("one Scribe snapshot");
    let server = harness
        .cluster()
        .server(0)
        .expect("one Bifrost test server");
    let state = server.state();
    let parent_snapshot = state
        .bifrost_resources
        .as_ref()
        .expect("shared Bifrost parent governor")
        .snapshot()
        .expect("root snapshot");
    assert!(
        state
            .bifrost_resources
            .as_ref()
            .and_then(vala_bifrost_redux::resources::BifrostRoleResources::oracle)
            .is_some()
    );
    assert!(state.forge().is_some());
    let scribe_snapshot = harness
        .scribes()
        .first()
        .expect("one Scribe")
        .memory_snapshot();
    assert_eq!(
        scribe_snapshot.bifrost_limit_bytes,
        parent_snapshot.bifrost_limit_bytes
    );

    assert_eq!(snapshot.shard_task_count, 16);
    assert_eq!(snapshot.shard_channel_count, 16);
    assert!(snapshot.open_wal_stream_count <= 16);
    assert_eq!(
        snapshot.memory_by_shard.iter().copied().sum::<usize>(),
        snapshot.total_accounted_memory
    );
    assert_eq!(
        snapshot
            .memory_by_bucket
            .iter()
            .map(|bucket| bucket.writable_bytes + bucket.immutable_bytes)
            .sum::<usize>(),
        snapshot.memory_by_category[4] + snapshot.memory_by_category[5]
    );

    harness.shutdown().await.expect("harness shutdown");
}

/// Exercises the T35 batch-spread routing contract end to end through the
/// Scribe append path and pod-local shard inspection.
///
/// The shard key is `(tenant, table, batch_id)`, so this journey proves two
/// complementary properties against real memory-by-shard accounting: (a)
/// appends that share one `(tenant, table, batch_id)` — a client retrying a
/// single logical batch — occupy exactly the one lane `shard_for` selects, so
/// dedup state stays lane-local; and (b) a hot `(tenant, table)` pair that
/// mints a fresh `batch_id` per request spreads its write load across more than
/// one lane. It then reasserts the fixed 16-shard topology and the
/// 1000-bucket memory-accounting invariants for a mixed three-tenant workload.
#[tokio::test]
#[ignore = "requires the embedded Postgres and object-store fixtures"]
async fn scribe_owner_dynamic_keys_preserve_topology() {
    let harness = BifrostHarness::start(1, 3)
        .await
        .expect("one-pod, three-tenant Bifrost harness");
    let scribe = harness.scribes().first().expect("one Scribe");
    let tenants = harness.tenants();
    let schema = Arc::new(Schema::new(vec![
        Field::new("value", DataType::Int64, false),
        Field::new(
            "wyrd_event_time",
            DataType::Timestamp(TimeUnit::Microsecond, Some("UTC".into())),
            false,
        ),
    ]));
    let timestamp = chrono::Utc::now().timestamp_micros();
    let rows = RecordBatch::try_new(
        schema.clone(),
        vec![
            Arc::new(Int64Array::from(vec![1_i64])),
            Arc::new(TimestampMicrosecondArray::from(vec![timestamp]).with_timezone("UTC")),
        ],
    )
    .expect("inspection batch");
    let fingerprint = SchemaFingerprint::from_arrow_schema(schema.as_ref());
    let principals = tenants
        .iter()
        .copied()
        .map(|tenant_id| Principal {
            id: PrincipalId::new(Uuid::now_v7()),
            kind: PrincipalKind::User,
            tenant_id,
            roles: Vec::new(),
            effective_permissions: PermissionSet::new(),
        })
        .collect::<Vec<_>>();

    let concentrated_tenant = tenants[0];
    let concentrated_table =
        TableRef::new(BifrostNamespace::Bifrost, "bifrost_owner_concentrated_0");
    // A fixed batch id models a client retrying one logical batch. Every attempt
    // must route to the same shard lane so the lane's dedup state absorbs it.
    let shared_batch = Uuid::now_v7();
    let shared_lane = shard_for(concentrated_tenant, &concentrated_table, shared_batch);
    for _ in 0..3_usize {
        scribe
            .append_durable(ScribeAppend {
                principal: principals[0].clone(),
                table: concentrated_table.clone(),
                rows: rows.clone(),
                schema_fingerprint: fingerprint,
                request_id: RequestId::now_v7(),
                batch_id: shared_batch,
                measured_wire_bytes: 0,
            })
            .await
            .expect("retried same-key durable ACK");
    }

    // (a) Appends sharing one (tenant, table, batch_id) occupy exactly one
    // non-empty lane — the lane `shard_for` selects for that key.
    let shared_snapshot = harness
        .inspection_snapshots()
        .expect("shared-key inspection snapshot")
        .remove(0);
    assert_eq!(
        shared_snapshot
            .memory_by_shard
            .iter()
            .filter(|bytes| **bytes > 0)
            .count(),
        1,
        "one (tenant, table, batch_id) must occupy exactly one shard lane"
    );
    assert!(
        shared_snapshot.memory_by_shard[shared_lane] > 0,
        "the occupied lane must be the shard_for-selected lane"
    );

    // (b) Distinct batch ids for the same (tenant, table) spread that hot pair
    // across more than one lane. Re-appending `concentrated_table` under fresh
    // batch ids keeps it a single bucket while dispersing its write load.
    for _ in 0..64_usize {
        scribe
            .append_durable(ScribeAppend {
                principal: principals[0].clone(),
                table: concentrated_table.clone(),
                rows: rows.clone(),
                schema_fingerprint: fingerprint,
                request_id: RequestId::now_v7(),
                batch_id: Uuid::now_v7(),
                measured_wire_bytes: 0,
            })
            .await
            .expect("batch-spread durable ACK");
    }

    // Capture the spread snapshot before any non-concentrated append lands, so
    // the >1-lane count reflects only `concentrated_table`. A route-by-(tenant,
    // table) regression that ignored `batch_id` would collapse this hot table
    // onto a single lane and fail here; taking the snapshot after the fill loop
    // below would mask that, since the fill's distinct table FQNs spread lanes
    // on their own.
    let concentrated_snapshot = harness
        .inspection_snapshots()
        .expect("concentrated inspection snapshot")
        .remove(0);
    let concentrated_lane_count = concentrated_snapshot
        .memory_by_shard
        .iter()
        .filter(|bytes| **bytes > 0)
        .count();
    assert!(
        concentrated_lane_count > 1,
        "distinct batch ids must spread the hot table across more than one lane"
    );

    // Fill the concentrated section to 400 distinct buckets so the aggregate
    // bucket-count contract below (400 concentrated + 600 dispersed) holds.
    for index in 1..400_usize {
        let table = TableRef::new(
            BifrostNamespace::Bifrost,
            format!("bifrost_owner_concentrated_{index}"),
        );
        scribe
            .append_durable(ScribeAppend {
                principal: principals[0].clone(),
                table,
                rows: rows.clone(),
                schema_fingerprint: fingerprint,
                request_id: RequestId::now_v7(),
                batch_id: Uuid::now_v7(),
                measured_wire_bytes: 0,
            })
            .await
            .expect("concentrated fill durable ACK");
    }
    let mut candidate = 0_usize;

    for index in 0..600_usize {
        let tenant_index = index % tenants.len();
        let target_shard = index % SCRIBE_SHARD_COUNT;
        let table = table_for_shard(
            tenants[tenant_index],
            target_shard,
            "bifrost_owner_dispersed",
            &mut candidate,
        );
        scribe
            .append_durable(ScribeAppend {
                principal: principals[tenant_index].clone(),
                table,
                rows: rows.clone(),
                schema_fingerprint: fingerprint,
                request_id: RequestId::now_v7(),
                batch_id: Uuid::now_v7(),
                measured_wire_bytes: 0,
            })
            .await
            .expect("dispersed-key durable ACK");
    }

    let snapshot = harness
        .inspection_snapshots()
        .expect("Scribe inspection snapshots")
        .remove(0);
    assert_eq!(snapshot.shard_task_count, 16);
    assert_eq!(snapshot.shard_channel_count, 16);
    assert!(snapshot.memory_by_shard.iter().all(|bytes| *bytes > 0));
    assert!(snapshot.open_wal_stream_count <= 16);
    let expected_bucket_count = 999 + concentrated_lane_count;
    assert_eq!(snapshot.writable_bucket_count, expected_bucket_count);
    assert_eq!(snapshot.memory_by_bucket.len(), expected_bucket_count);
    assert_eq!(
        snapshot.memory_by_shard.iter().copied().sum::<usize>(),
        snapshot.total_accounted_memory
    );
    assert_eq!(
        snapshot
            .memory_by_bucket
            .iter()
            .map(|bucket| bucket.writable_bytes + bucket.immutable_bytes)
            .sum::<usize>(),
        snapshot.memory_by_category[4] + snapshot.memory_by_category[5]
    );

    harness.shutdown().await.expect("harness shutdown");
}
