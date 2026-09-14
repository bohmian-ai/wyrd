use vala_bifrost_redux::namespaces::BifrostNamespace;

use super::support::{
    await_persistence_drained, published_object_count, register_table, sorted_values,
    start_scribe_server, tenant_client, unique_table,
};

/// Every authoritative source transition is exact.
///
/// A row acknowledged by Scribe is owned in turn by an active memtable, a
/// durable staged member, and finally a published hot object. Each handoff is
/// a place a row can be lost, duplicated, or become briefly invisible, and a
/// strict fused read must not be able to tell which owner answered it.
///
/// The case walks the two production boundaries one at a time so a failure
/// localizes to one seam: the freeze that turns writable buckets into durable
/// staged members, and the publication that turns staged members into
/// committed hot objects. Between them it asserts the *negative* fact that
/// makes the first boundary real — the rows are already durable and readable
/// while nothing at all has been published.
///
/// # Panics
///
/// Panics when a public append or read fails, when a boundary does not move
/// the observed ownership, when publication happens before it is asked for, or
/// when any read returns a row set other than exactly the rows appended.
#[tokio::test]
#[ignore = "requires Postgres and object storage"]
async fn scribe_local_stage_to_hot_source_transition_is_exact() {
    let server = start_scribe_server().await;
    let tenant = server.data_tenant_id();
    let name = unique_table("stage_to_hot");
    let table = register_table(&server, tenant, BifrostNamespace::Datasets, &name).await;
    let client = tenant_client(&server, tenant).await;

    let expected: Vec<i64> = (0..64).collect();
    for (ordinal, chunk) in expected.chunks(16).enumerate() {
        super::support::append_values(&client, &table, uuid::Uuid::now_v7(), chunk)
            .await
            .unwrap_or_else(|error| panic!("append {ordinal} is acknowledged: {error:?}"));
    }

    // Active authority: the rows are live in writable buckets and nothing has
    // been staged or published yet.
    let snapshot = server
        .scribe_inspection_snapshot()
        .expect("Scribe ownership is inspectable");
    assert!(
        snapshot.writable_bucket_count > 0,
        "acknowledged rows must be owned by a writable bucket before any freeze"
    );
    assert_eq!(
        sorted_values(&client, &table).await,
        expected,
        "a strict read of the active source must return exactly the acknowledged rows"
    );
    assert_eq!(
        published_object_count(&server, tenant, BifrostNamespace::Datasets, &name).await,
        0,
        "no hot object may be published before the run asks for publication"
    );

    // First boundary: freeze. The writable buckets become durable staged
    // members. Publication is a separate decision and must not have happened.
    let scribe = server.bifrost_scribe().expect("the server owns a Scribe");
    scribe
        .flush_writable_for_test()
        .await
        .expect("every writable bucket freezes");
    await_persistence_drained(&scribe).await;

    let staged = server
        .scribe_inspection_snapshot()
        .expect("Scribe ownership is inspectable");
    let frozen = super::support::journey_buckets(&staged);
    assert!(
        frozen.iter().all(|bucket| bucket.writable_bytes == 0),
        "the freeze must leave no writable bucket owning acknowledged rows; \
         surviving: {frozen:?}"
    );
    assert_eq!(
        published_object_count(&server, tenant, BifrostNamespace::Datasets, &name).await,
        0,
        "the freeze is a local staging boundary and may not publish a hot object"
    );
    assert_eq!(
        sorted_values(&client, &table).await,
        expected,
        "a strict read of the staged source must return exactly the acknowledged rows"
    );

    // Second boundary: publication. The staged members become committed hot
    // objects and the same read must still be exact.
    server
        .flush_bifrost()
        .await
        .expect("the staged members publish");
    let published = server
        .published_hot_files_for_test(tenant, BifrostNamespace::Datasets.as_str(), &name)
        .await
        .expect("published hot files are inspectable");
    assert!(
        !published.is_empty(),
        "publication must commit at least one hot object"
    );
    let published_rows: u64 = published.iter().map(|file| file.row_count).sum();
    assert_eq!(
        published_rows,
        expected.len() as u64,
        "the published objects must account for every acknowledged row exactly once"
    );
    for file in &published {
        file.promotion_record
            .data_file()
            .expect("every published object rebuilds its Iceberg data file");
    }
    assert_eq!(
        sorted_values(&client, &table).await,
        expected,
        "a strict read of the published-hot source must return exactly the acknowledged rows"
    );

    server.shutdown().await.expect("the server drains cleanly");
}
