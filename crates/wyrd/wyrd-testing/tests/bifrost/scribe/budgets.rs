//! Fixed sixteen-lane topology and bounded pod/tenant ownership under load.

use vala_bifrost_redux::namespaces::BifrostNamespace;
use vala_bifrost_redux::scribe::admission::GLOBAL_INFLIGHT_ITEMS;
use vala_bifrost_redux::scribe::routing::SCRIBE_SHARD_COUNT;
use wyrd_spec::DataTenantId;

use super::support::{
    append_values, register_table, sorted_values, start_scribe_server, tenant_client, unique_table,
};

/// Tenants that contend for the one pod budget at the same time.
const TENANTS: usize = 4;
/// Tables each tenant registers and writes.
const TABLES_PER_TENANT: usize = 2;
/// Distinct batch identities each tenant sends per table.
///
/// Routing is a hash of `(tenant, table, batch_id)`, so lane coverage is a
/// function of how many distinct identities the case sends. 24 per table gives
/// 192 routes across sixteen lanes, which leaves an empty lane a probability
/// under one in two hundred thousand — low enough that an empty lane means the
/// topology is narrower than sixteen, not that the case was unlucky.
const BATCHES_PER_TABLE: usize = 24;
/// Rows in one batch.
const ROWS_PER_BATCH: usize = 64;

/// AC22 Tier-2 owner: sixteen fixed lanes hold one bounded pod budget.
///
/// Scribe's topology is a fixed constant, not a function of how many tenants or
/// tables the pod happens to be serving, and its memory is one pod budget that
/// every lane and every tenant draws from. Both claims fail the same silent way
/// — a pod that grows a lane or a WAL stream per tenant, or a tenant that takes
/// the budget its neighbours needed, still returns correct rows — so this owner
/// asserts them against observed production state while four tenants are
/// concurrently resident, not after the pod has quiesced.
///
/// The case holds live ownership on purpose: none of the workload reaches a
/// rotation or dwell boundary, so every acknowledged row is still owned by a
/// writable bucket when the topology and budget assertions run.
///
/// # Panics
///
/// Panics when an append or read fails, when the topology is not exactly
/// sixteen lanes, when accounted memory disagrees with the per-shard or
/// per-bucket totals, when any reservation exceeds its ceiling, when the lanes
/// are not all carrying work, when one tenant's ownership crowds out another's,
/// or when a tenant does not read back exactly the rows it acknowledged.
#[tokio::test]
#[ignore = "requires Postgres and object storage"]
async fn scribe_sixteen_shards_obey_global_and_tenant_budgets() {
    let server = start_scribe_server().await;
    let mut tenants: Vec<DataTenantId> = vec![server.data_tenant_id()];
    for ordinal in 1..TENANTS {
        tenants.push(
            server
                .seed_tenant(&format!("scribe-budget-{ordinal}"))
                .await
                .expect("the contending tenant is seeded"),
        );
    }

    // Every tenant sends the identical shape, so any later asymmetry in owned
    // bytes is the pod's scheduling decision rather than the workload's.
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
    for batch in 0..BATCHES_PER_TABLE {
        for (index, (_, client, tables)) in residents.iter().enumerate() {
            for (table_ordinal, (_, table)) in tables.iter().enumerate() {
                let first = ((batch * TABLES_PER_TENANT + table_ordinal) * ROWS_PER_BATCH) as i64;
                let rows: Vec<i64> = (first..first + ROWS_PER_BATCH as i64).collect();
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

    // Topology: fixed by the contract, not by tenant or table cardinality.
    assert_eq!(
        loaded.shard_task_count, SCRIBE_SHARD_COUNT,
        "the pod must own exactly sixteen shard tasks whatever it is serving"
    );
    assert_eq!(
        loaded.shard_channel_count, SCRIBE_SHARD_COUNT,
        "the pod must own exactly sixteen shard channels whatever it is serving"
    );
    assert!(
        loaded.open_wal_stream_count <= SCRIBE_SHARD_COUNT,
        "WAL streams are per lane, not per tenant or table: {} open for {TENANTS} tenants",
        loaded.open_wal_stream_count
    );
    assert!(
        loaded.queued_items <= GLOBAL_INFLIGHT_ITEMS,
        "accepted work must stay inside the pod in-flight ceiling: {} queued",
        loaded.queued_items
    );

    // Budget: one accounted total, reconciled from both directions, under every
    // ceiling it is charged against.
    let by_shard: usize = loaded.memory_by_shard.iter().sum();
    let by_bucket: usize = loaded
        .memory_by_bucket
        .iter()
        .map(|bucket| bucket.writable_bytes + bucket.immutable_bytes)
        .sum();
    // Every acknowledged byte is owned by one bucket on one lane, so the two
    // axes of the same ownership must agree exactly. Comparing them to each
    // other rather than to the governor's total is what makes this sensitive:
    // the pod is at rest here, so a lane that lost or double-counted a bucket
    // moves one side of this equality and nothing else in the snapshot.
    assert_eq!(
        by_shard, by_bucket,
        "per-lane ownership {by_shard} must account for exactly the buckets {by_bucket}"
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

    // Lanes: all sixteen are load-bearing, not sixteen declared and one used.
    let idle: Vec<usize> = (0..SCRIBE_SHARD_COUNT)
        .filter(|shard| loaded.memory_by_shard[*shard] == 0)
        .collect();
    assert!(
        idle.is_empty(),
        "every fixed lane must carry admitted work; idle lanes {idle:?} of {:?}",
        loaded.memory_by_shard
    );

    // Tenant budget: equal demand gets comparable ownership, and no tenant is
    // squeezed out of the pod budget by a neighbour.
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
        "every contending tenant must own the rows it was acknowledged for: {owned:?}"
    );
    assert!(
        largest <= smallest.saturating_mul(2),
        "equal demand must not produce lopsided tenant ownership: {owned:?}"
    );

    // The budget is a loan: publication returns every byte of it.
    server
        .flush_bifrost_for_tenant(tenants[0])
        .await
        .expect("the staged members publish");
    let settled = server
        .scribe_inspection_snapshot()
        .expect("Scribe ownership is inspectable");
    assert_eq!(
        settled.writable_bucket_count, 0,
        "publication must leave no writable bucket owning published rows"
    );
    assert_eq!(
        settled.immutable_bucket_count, 0,
        "publication must leave no immutable bucket owning published rows"
    );
    assert_eq!(
        settled.shard_task_count, SCRIBE_SHARD_COUNT,
        "the fixed topology must survive publication unchanged"
    );

    // Exactness: contending for one budget never cost a tenant a row.
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
