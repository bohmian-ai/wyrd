//! Generations sealed on several shards publish their objects exactly once.

use vala_bifrost_redux::namespaces::BifrostNamespace;
use vala_bifrost_redux::scribe::geometry::ScribeGeometry;

use super::support::{
    append_values, published_rows, register_table, sorted_values,
    start_scribe_server_with_geometry, tenant_client, unique_table,
};

/// Shards the pod runs for this case.
///
/// The default pod runs one shard, so this is the journey that proves a
/// multi-shard pod. Four is enough for twenty-four random batch identities to
/// spread one table across more than one shard.
const SHARD_COUNT: usize = 4;

/// Rows appended in one public request.
///
/// Sized so the encoded Arrow IPC envelope stays far below the production
/// ingest request ceiling while the case still reaches its row target in a
/// bounded number of requests.
const ROWS_PER_BATCH: i64 = 6_000;

/// Public requests the case sends.
///
/// `BATCHES * ROWS_PER_BATCH` passes the retired 131,072-row group bound, so a
/// writer still rolling groups at that size would close a second group here.
const BATCHES: i64 = 24;

/// Rows one Parquet row group holds before the writer rolls.
///
/// The shared writer recipe keeps parquet-rs's row ceiling beside its soft
/// 128 MiB encoded target; these tiny rows stay far below that target, so the
/// row ceiling alone decides how few groups a packed object needs.
const ROW_GROUP_ROWS: u64 = parquet::file::properties::DEFAULT_MAX_ROW_GROUP_ROW_COUNT as u64;

/// One publication covers every shard's generation exactly once, in packed
/// objects.
///
/// Batches route by `(tenant, table, batch_id)`, so one table's rows are sealed
/// as separate generations on separate shard owners. A publication assembles
/// those generations into hot objects, and each generation may end up merged
/// into an object with its peers rather than owning one. Three things have to
/// hold at once: every generation's rows reach exactly one published object, an
/// object built from members on more than one shard packs them into the fewest
/// row groups its rows allow, and a second publication pass finds nothing left
/// to do.
///
/// The group count is asserted exactly. A claim merges many staged runs, most
/// of them far smaller than a group; writing one group per run would make the
/// footer grow with the number of runs merged instead of with the object's
/// size. The fewest-groups-its-rows-allow form is the one that separates packed
/// groups from per-run groups, and it reads the sealed footer's split offsets
/// rather than trusting the object count.
///
/// The pod runs [`SHARD_COUNT`] shards and the case drives real appends
/// through the production router, so the merge it proves is the one a
/// multi-shard pod performs rather than a single-shard special case.
///
/// # Panics
///
/// Panics when a public append or read fails, when the router leaves the table
/// on one shard, when no published object was merged from members on more than
/// one shard, when such an object closed with a non-ascending or unpacked set
/// of row groups, when the published objects do not account for exactly the
/// acknowledged rows, or when a second publication pass republishes them.
#[tokio::test]
#[ignore = "requires Postgres and object storage"]
async fn scribe_cross_shard_generations_publish_packed_objects_once() {
    let server = start_scribe_server_with_geometry(
        ScribeGeometry::default()
            .with_shard_count(SHARD_COUNT)
            .expect("four shards is a valid geometry"),
    )
    .await;
    let tenant = server.data_tenant_id();
    let name = unique_table("cross_shard");
    let table = register_table(&server, tenant, BifrostNamespace::Datasets, &name).await;
    let client = tenant_client(&server, tenant).await;

    let mut expected = Vec::new();
    for batch in 0..BATCHES {
        let start = batch * ROWS_PER_BATCH;
        let rows: Vec<i64> = (start..start + ROWS_PER_BATCH).collect();
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
        .flush_bifrost()
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

    // Which shards produced an object is not a fact the file-list row or its
    // promotion record carries, so the claim observation is what makes "this
    // object is a real cross-shard merge" an assertion rather than an
    // inference from how few objects were published.
    let claims = server
        .published_scribe_claims_for_test()
        .expect("the production Scribe reports what its claims published");
    let merged: Vec<&str> = claims
        .iter()
        .filter(|claim| claim.member_shards.len() > 1)
        .flat_map(|claim| claim.object_keys.iter().map(String::as_str))
        .collect();
    assert!(
        !merged.is_empty(),
        "at least one published claim must have drawn members from more than one \
         shard: {claims:?}"
    );

    for file in &published {
        if !merged.contains(&file.object_key.as_str()) {
            continue;
        }
        let offsets = &file.promotion_record.data_file.split_offsets;
        assert!(
            offsets.windows(2).all(|pair| pair[0] < pair[1]),
            "row-group offsets must be strictly ascending in {}: {offsets:?}",
            file.object_key
        );
        let rows = file.promotion_record.data_file.record_count;
        assert_eq!(
            offsets.len() as u64,
            rows.div_ceil(ROW_GROUP_ROWS),
            "a merged object must hold the fewest groups its {rows} rows allow, \
             not one per input run, in {}: {offsets:?}",
            file.object_key
        );
    }

    // A second pass has nothing left to publish: the same rows, the same
    // objects, and the same durable file-list identities.
    server
        .flush_bifrost()
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
