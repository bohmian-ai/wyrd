//! Task-15 fixed-topology and ownership inspection.

use std::sync::Arc;

use arrow::array::{Int64Array, TimestampMicrosecondArray};
use arrow::datatypes::{DataType, Field, Schema, TimeUnit};
use arrow::record_batch::RecordBatch;
use uuid::Uuid;
use vala_bifrost_redux::catalog::TableRef;
use vala_bifrost_redux::contracts::{Scribe, ScribeAppend};
use vala_bifrost_redux::namespaces::BifrostNamespace;
use vala_bifrost_redux::schema::SchemaFingerprint;
use wyrd_runtime::{PermissionSet, Principal, PrincipalKind};
use wyrd_spec::auth::PrincipalId;
use wyrd_spec::request_id::RequestId;
use wyrd_testing::bifrost::BifrostHarness;

#[tokio::test]
#[ignore = "requires the embedded Postgres and object-store fixtures"]
async fn scribe_inspection_has_fixed_topology_and_reconciled_memory() {
    let harness = BifrostHarness::start(1, 1)
        .await
        .expect("one-pod Bifrost harness");
    let snapshots = harness
        .inspection_snapshots()
        .expect("Scribe inspection snapshots");
    let snapshot = snapshots.first().expect("one Scribe snapshot");

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

#[tokio::test]
#[ignore = "requires the embedded Postgres and object-store fixtures"]
async fn one_thousand_dynamic_keys_do_not_change_shard_topology() {
    let harness = BifrostHarness::start(1, 1)
        .await
        .expect("one-pod Bifrost harness");
    let scribe = harness.scribes().first().expect("one Scribe");
    let tenant = *harness.tenants().first().expect("one tenant");
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
    let principal = Principal {
        id: PrincipalId::new(Uuid::now_v7()),
        kind: PrincipalKind::User,
        tenant_id: tenant,
        roles: Vec::new(),
        effective_permissions: PermissionSet::new(),
    };

    for index in 0..1_000_usize {
        let table = TableRef::new(
            BifrostNamespace::Bifrost,
            format!("task15_cardinality_{index}"),
        );
        scribe
            .append_durable(ScribeAppend {
                principal: principal.clone(),
                table,
                rows: rows.clone(),
                schema_fingerprint: fingerprint,
                request_id: RequestId::now_v7(),
                batch_id: Uuid::now_v7(),
                measured_wire_bytes: 0,
            })
            .await
            .expect("dynamic-key durable ACK");
    }

    let snapshot = harness
        .inspection_snapshots()
        .expect("Scribe inspection snapshots")
        .remove(0);
    assert_eq!(snapshot.shard_task_count, 16);
    assert_eq!(snapshot.shard_channel_count, 16);
    assert!(snapshot.open_wal_stream_count <= 16);
    assert_eq!(snapshot.writable_bucket_count, 1_000);
    assert_eq!(snapshot.memory_by_bucket.len(), 1_000);
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
