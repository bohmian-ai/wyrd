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
use vala_bifrost_redux::scribe::routing::{SCRIBE_SHARD_COUNT, shard_for};
use wyrd_runtime::{PermissionSet, Principal, PrincipalKind};
use wyrd_spec::auth::PrincipalId;
use wyrd_spec::request_id::RequestId;
use wyrd_testing::bifrost::BifrostHarness;

fn table_for_shard(
    tenant: wyrd_spec::DataTenantId,
    target_shard: usize,
    prefix: &str,
    candidate: &mut usize,
) -> TableRef {
    loop {
        let table = TableRef::new(
            BifrostNamespace::Bifrost,
            format!("{prefix}_{}", *candidate),
        );
        *candidate += 1;
        if shard_for(tenant, &table) == target_shard {
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
        .bifrost_memory
        .as_ref()
        .expect("shared Bifrost parent governor")
        .snapshot();
    assert!(state.bifrost_query_memory.is_some());
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
    let concentrated_target = shard_for(
        concentrated_tenant,
        &TableRef::new(BifrostNamespace::Bifrost, "task15_concentrated_seed"),
    );
    let mut candidate = 0_usize;

    for _ in 0..400_usize {
        let table = table_for_shard(
            concentrated_tenant,
            concentrated_target,
            "task15_concentrated",
            &mut candidate,
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
            .expect("dynamic-key durable ACK");
    }

    let concentrated_snapshot = harness
        .inspection_snapshots()
        .expect("concentrated inspection snapshot")
        .remove(0);
    assert_eq!(
        concentrated_snapshot
            .memory_by_shard
            .iter()
            .filter(|bytes| **bytes > 0)
            .count(),
        1,
        "concentrated keys must stay on one shard"
    );
    assert!(concentrated_snapshot.memory_by_shard[concentrated_target] > 0);

    for index in 0..600_usize {
        let tenant_index = index % tenants.len();
        let target_shard = index % SCRIBE_SHARD_COUNT;
        let table = table_for_shard(
            tenants[tenant_index],
            target_shard,
            "task15_dispersed",
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
