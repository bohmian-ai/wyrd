//! Generations sealed on several shards publish their objects exactly once.

use vala_bifrost_redux::namespaces::BifrostNamespace;

use super::support::{
    append_values, published_rows, register_table, sorted_values, start_scribe_server,
    tenant_client, unique_table,
};

/// AC22 Tier-2 owner: one publication covers every shard's generation once.
///
/// Batches route by `(tenant, table, batch_id)`, so one table's rows are sealed
/// as separate generations on separate shard owners. A publication assembles
/// those generations into hot objects, and each generation may end up merged
/// into an object with its peers rather than owning one. Two things have to
/// hold at once: every generation's rows reach exactly one published object,
/// and a second publication pass finds nothing left to do.
///
/// The case drives real appends until the production router has spread the
/// table across several shards, so the merge it proves is the one production
/// performs rather than a single-lane special case.
///
/// # Panics
///
/// Panics when a public append or read fails, when the router leaves the table
/// on one shard, when the published objects do not account for exactly the
/// acknowledged rows, or when a second publication pass republishes them.
#[tokio::test]
#[ignore = "requires Postgres and object storage"]
async fn scribe_cross_shard_generations_publish_multi_group_objects_once() {
    let server = start_scribe_server().await;
    let tenant = server.data_tenant_id();
    let name = unique_table("cross_shard");
    let table = register_table(&server, tenant, BifrostNamespace::Datasets, &name).await;
    let client = tenant_client(&server, tenant).await;

    let mut expected = Vec::new();
    for batch in 0..24_i64 {
        let rows: Vec<i64> = (batch * 100..batch * 100 + 8).collect();
        append_values(&client, &table, uuid::Uuid::now_v7(), &rows)
            .await
            .expect("every batch is acknowledged");
        expected.extend_from_slice(&rows);
    }
    expected.sort_unstable();

    let occupied_shards = server
        .scribe_inspection_snapshot()
        .expect("the production Scribe is inspectable")
        .memory_by_shard
        .iter()
        .filter(|bytes| **bytes > 0)
        .count();
    assert!(
        occupied_shards > 1,
        "the production router must spread one table across shards, not {occupied_shards}"
    );
    assert_eq!(
        sorted_values(&client, &table).await,
        expected,
        "every shard's writable generation must be readable before publication"
    );

    server
        .flush_bifrost_for_tenant(tenant)
        .await
        .expect("the cross-shard generations publish");
    let published = server
        .published_hot_files_for_test(tenant, BifrostNamespace::Datasets.as_str(), &name)
        .await
        .expect("published hot files are inspectable");
    assert_eq!(
        published_rows(&server, tenant, &name).await,
        expected.len() as u64,
        "the published objects must account for exactly the acknowledged rows"
    );
    assert!(
        published.len() < occupied_shards,
        "a publication must merge peer generations into shared objects, \
         not publish {} objects for {occupied_shards} shards",
        published.len()
    );
    let distinct_objects = published
        .iter()
        .map(|file| file.object_key.as_str())
        .collect::<std::collections::HashSet<_>>();
    assert_eq!(
        distinct_objects.len(),
        published.len(),
        "no two published rows may name the same object"
    );
    let distinct_checksums = published
        .iter()
        .map(|file| file.file_checksum.as_str())
        .collect::<std::collections::HashSet<_>>();
    assert_eq!(
        distinct_checksums.len(),
        published.len(),
        "two published objects with identical bytes would mean one was published twice"
    );
    assert_eq!(
        sorted_values(&client, &table).await,
        expected,
        "published objects must read back exactly the acknowledged rows"
    );

    // A second pass has nothing left to publish: the same rows, the same
    // objects, and the same durable file-list identities.
    server
        .flush_bifrost_for_tenant(tenant)
        .await
        .expect("a second publication pass succeeds");
    let republished = server
        .published_hot_files_for_test(tenant, BifrostNamespace::Datasets.as_str(), &name)
        .await
        .expect("published hot files are inspectable");
    assert_eq!(
        republished.iter().map(|file| file.id).collect::<Vec<_>>(),
        published.iter().map(|file| file.id).collect::<Vec<_>>(),
        "a second publication pass may not republish an already published claim"
    );
    assert_eq!(
        sorted_values(&client, &table).await,
        expected,
        "a second publication pass may not change what the table reads back"
    );

    server.shutdown().await.expect("the server drains cleanly");
}
