//! The pod's fixed topology and its one shared budget.
//!
//! The topology and accounting owner runs on a production-shaped pod, because
//! that is the pod whose shard count and memory identities are being asserted.
//! Tenant-then-table rotation is a shard scheduler decision proven by the
//! scheduler's own tests.

use vala_bifrost_redux::namespaces::BifrostNamespace;
use vala_bifrost_redux::scribe::admission::GLOBAL_INFLIGHT_ITEMS;
use vala_bifrost_redux::scribe::geometry::DEFAULT_SHARD_COUNT;
use wyrd_spec::DataTenantId;

use super::support::{
    append_values, register_table, sorted_values, start_scribe_server, tenant_client, unique_table,
};

/// Tenants that are concurrently resident on the topology pod.
const TENANTS: usize = 4;
/// Tables each resident tenant registers and writes.
const TABLES_PER_TENANT: usize = 2;
/// Rows in one topology batch.
const ROWS_PER_BATCH: usize = 64;
/// Batches each resident table appends.
const BATCHES_PER_TABLE: usize = 2;

/// The configured shards share one bounded pod budget that is lent, not divided.
///
/// Scribe's shard count is configuration, not a function of how many tenants or
/// tables the pod happens to be serving, and its memory is one pod budget that
/// every shard and every tenant draws from. The failures this owner guards
/// against are silent ones: a pod that grows a shard or a WAL stream per
/// tenant, accounting that does not add up across shards and buckets, or a
/// tenant that keeps a share its neighbours were owed. Every one of them still
/// returns correct rows.
///
/// The pod runs on production admission and the production default shard
/// count. This owner asserts what a real pod's topology and accounting look
/// like, so starving it would be asserting against a pod no deployment runs.
///
/// # Panics
///
/// Panics when the topology differs from the configured shard count, when WAL
/// streams or queued work exceed their per-pod ceilings, when accounted memory
/// disagrees with the per-shard or per-bucket totals, when equal demand
/// produces lopsided tenant ownership, when publication leaves a bucket
/// behind, or when a tenant does not read back exactly what it acknowledged.
#[tokio::test]
#[ignore = "requires Postgres and object storage"]
async fn scribe_shards_obey_global_and_tenant_budgets() {
    let server = start_scribe_server().await;

    let mut tenants: Vec<DataTenantId> = vec![server.data_tenant_id()];
    for ordinal in 1..TENANTS {
        tenants.push(
            server
                .seed_tenant(&format!("scribe-budget-{ordinal}"))
                .await
                .expect("the resident tenant is seeded"),
        );
    }

    let mut residents = Vec::with_capacity(TENANTS);
    for tenant in &tenants {
        let client = tenant_client(&server, *tenant).await;
        let mut tables = Vec::with_capacity(TABLES_PER_TENANT);
        for table_ordinal in 0..TABLES_PER_TENANT {
            let name = unique_table(&format!("budget_{table_ordinal}"));
            let table = register_table(&server, *tenant, BifrostNamespace::Datasets, &name).await;
            tables.push((name, table));
        }
        residents.push((*tenant, client, tables));
    }

    let mut expected: Vec<Vec<i64>> = vec![Vec::new(); TENANTS];
    for (index, (_, client, tables)) in residents.iter().enumerate() {
        for (table_ordinal, (_, table)) in tables.iter().enumerate() {
            let owner = index * TABLES_PER_TENANT + table_ordinal;
            for batch in 0..BATCHES_PER_TABLE {
                let first = i64::try_from((owner * BATCHES_PER_TABLE + batch) * ROWS_PER_BATCH)
                    .expect("test row ordinals fit i64");
                let rows: Vec<i64> = (first..).take(ROWS_PER_BATCH).collect();
                append_values(client, table, uuid::Uuid::now_v7(), &rows)
                    .await
                    .unwrap_or_else(|error| {
                        panic!("tenant {index} batch {batch} is acknowledged: {error:?}")
                    });
                expected[index].extend_from_slice(&rows);
            }
        }
    }

    let loaded = server
        .scribe_inspection_snapshot()
        .expect("Scribe ownership is inspectable");

    assert_eq!(
        loaded.shard_task_count, DEFAULT_SHARD_COUNT,
        "the pod must own exactly its configured shard tasks whatever it is serving"
    );
    assert_eq!(
        loaded.shard_channel_count, DEFAULT_SHARD_COUNT,
        "the pod must own exactly its configured shard channels whatever it is serving"
    );
    assert!(
        loaded.open_wal_stream_count <= DEFAULT_SHARD_COUNT,
        "WAL streams are per shard, not per tenant or table: {} open for {TENANTS} tenants",
        loaded.open_wal_stream_count
    );
    assert!(
        loaded.queued_items <= GLOBAL_INFLIGHT_ITEMS,
        "accepted work must stay inside the pod in-flight ceiling: {} queued",
        loaded.queued_items
    );

    let by_shard: usize = loaded.memory_by_shard.iter().sum();
    let by_bucket: usize = loaded
        .memory_by_bucket
        .iter()
        .map(|bucket| bucket.writable_bytes + bucket.immutable_bytes)
        .sum();
    assert_eq!(
        by_shard, by_bucket,
        "per-shard ownership {by_shard} must account for exactly the buckets {by_bucket}"
    );
    assert!(
        by_bucket > 0 && by_bucket <= loaded.total_accounted_memory,
        "bucket ownership {by_bucket} must be live and inside the accounted pod total {}",
        loaded.total_accounted_memory
    );
    assert!(
        loaded.scribe_used_memory <= loaded.scribe_memory_limit,
        "Scribe's reservation {} exceeded its child ceiling {}",
        loaded.scribe_used_memory,
        loaded.scribe_memory_limit
    );
    assert!(
        loaded.ingress_used_memory <= loaded.ingress_memory_limit,
        "ingress occupancy {} exceeded its ceiling {}",
        loaded.ingress_used_memory,
        loaded.ingress_memory_limit
    );
    assert!(
        loaded.parent_used_memory <= loaded.parent_memory_limit,
        "Bifrost reservation {} exceeded the parent ceiling {}",
        loaded.parent_used_memory,
        loaded.parent_memory_limit
    );

    let mut owned = vec![0_usize; TENANTS];
    for bucket in &loaded.memory_by_bucket {
        if let Some(index) = tenants
            .iter()
            .position(|tenant| *tenant == bucket.seal_key.tenant)
        {
            owned[index] += bucket.writable_bytes + bucket.immutable_bytes;
        }
    }
    let smallest = owned.iter().copied().min().unwrap_or_default();
    let largest = owned.iter().copied().max().unwrap_or_default();
    assert!(
        smallest > 0,
        "every resident tenant must own the rows it was acknowledged for: {owned:?}"
    );
    assert!(
        largest <= smallest.saturating_mul(2),
        "equal demand must not produce lopsided tenant ownership: {owned:?}"
    );

    server
        .flush_bifrost()
        .await
        .expect("the staged members publish");
    let settled = server
        .scribe_inspection_snapshot()
        .expect("Scribe ownership is inspectable");
    let owned = super::support::journey_buckets(&settled);
    assert!(
        owned.is_empty(),
        "publication must leave no bucket owning published rows; surviving: {owned:?}"
    );
    assert_eq!(
        settled.shard_task_count, DEFAULT_SHARD_COUNT,
        "the configured topology must survive publication unchanged"
    );

    for (index, (_, client, tables)) in residents.iter().enumerate() {
        let mut read_back = Vec::new();
        for (_, table) in tables {
            read_back.extend(sorted_values(client, table).await);
        }
        read_back.sort_unstable();
        let mut mine = expected[index].clone();
        mine.sort_unstable();
        assert_eq!(
            read_back, mine,
            "tenant {index} must read back exactly the rows it acknowledged"
        );
    }

    server.shutdown().await.expect("the server drains cleanly");
}
