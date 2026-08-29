//! Tenant, partition, and schema fences around one Scribe table name.

use vala_bifrost_redux::namespaces::BifrostNamespace;

use super::support::{
    hour_start,
    append_batch, append_values, append_values_at, await_persistence_drained, published_rows,
    read_sql, register_table, sorted_values, start_scribe_server, tenant_client, unique_table,
};

/// One table name is three independent fences.
///
/// The same logical table name means different things to different callers,
/// and Scribe has to keep all three separations exact at once. Two tenants may
/// register the identical name and neither may ever observe the other's rows.
/// One tenant's rows split across physical partitions by the event time the
/// client supplied, not by when Scribe happened to see them. And a batch whose
/// ingress schema is not the registered schema is refused outright rather than
/// half-written.
///
/// The three are one owner because they share a failure: a fence that leaks
/// shows up as extra rows in a strict read, and the read cannot say which
/// fence let them in unless the case holds all three at once against the same
/// name.
///
/// The partition fence is held under the state that makes it hard. WAL records
/// are numbered from one node-global counter while members are sealed per
/// partition, so publishing one partition produces bounds that enclose records
/// a still-live neighbouring member owns. The case builds that enclosure on
/// purpose and then reads across both partitions: the published object serves
/// its own rows, the enclosed member serves its own, and the union is exact.
/// A cut that treats numeric containment as proof of ownership drops the
/// enclosed member's acknowledged rows here.
///
/// # Panics
///
/// Panics when a public append or read fails, when either tenant observes the
/// other's rows, when the client's event time does not decide the physical
/// partition, when the fixture fails to build the enclosing topology, when a
/// published envelope suppresses a live member it does not own, or when a
/// mismatched schema is accepted or leaves rows behind.
#[tokio::test]
#[ignore = "requires Postgres and object storage"]
async fn scribe_tenant_partition_schema_fencing() {
    let server = start_scribe_server().await;
    let first_tenant = server.data_tenant_id();
    let second_tenant = server
        .seed_tenant("scribe-fencing-neighbour")
        .await
        .expect("the second tenant is seeded");
    let name = unique_table("fencing");
    let table = register_table(&server, first_tenant, BifrostNamespace::Datasets, &name).await;
    let neighbour_table =
        register_table(&server, second_tenant, BifrostNamespace::Datasets, &name).await;
    assert_eq!(
        table, neighbour_table,
        "both tenants must be registering the identical table name"
    );
    let client = tenant_client(&server, first_tenant).await;
    let neighbour = tenant_client(&server, second_tenant).await;

    // Tenant fence: identical name, disjoint rows, neither read crossing.
    let mine: Vec<i64> = (0..16).collect();
    let theirs: Vec<i64> = (100..116).collect();
    append_values(&client, &table, uuid::Uuid::now_v7(), &mine)
        .await
        .expect("the first tenant's append is acknowledged");
    append_values(&neighbour, &table, uuid::Uuid::now_v7(), &theirs)
        .await
        .expect("the second tenant's append is acknowledged");
    assert_eq!(
        sorted_values(&client, &table).await,
        mine,
        "a tenant must read exactly its own rows from a shared table name"
    );
    assert_eq!(
        sorted_values(&neighbour, &table).await,
        theirs,
        "the neighbouring tenant must read exactly its own rows"
    );

    // Schema fence: the registered schema is one Int64 column, and a batch
    // that adds a column is not that table's data.
    let widened = arrow::record_batch::RecordBatch::try_new(
        std::sync::Arc::new(arrow::datatypes::Schema::new(vec![
            arrow::datatypes::Field::new("value", arrow::datatypes::DataType::Int64, false),
            arrow::datatypes::Field::new("extra", arrow::datatypes::DataType::Utf8, false),
        ])),
        vec![
            std::sync::Arc::new(arrow::array::Int64Array::from(vec![900_i64])),
            std::sync::Arc::new(arrow::array::StringArray::from(vec!["unregistered"])),
        ],
    )
    .expect("widened batch");
    append_batch(&client, &table, uuid::Uuid::now_v7(), &widened)
        .await
        .expect_err("a batch that is not the registered schema is refused");
    assert_eq!(
        sorted_values(&client, &table).await,
        mine,
        "a refused schema may not leave any of its rows behind"
    );

    // Partition fence: the client's own event time decides the partition.
    // Ingest fences event time to a window around now, so the two partitions
    // are the current hour and the one before it rather than fixed literals.
    //
    // The two partitions are appended alternately so the node-global WAL
    // counter interleaves them. That is what makes the earlier partition's
    // published bounds enclose WAL positions the later partition owns, which is
    // the topology a cut must read as "not mine" rather than as ownership.
    let late = chrono::Utc::now();
    let early = late - chrono::Duration::hours(1);
    let early_rows: Vec<i64> = (200..208).collect();
    let late_rows: Vec<i64> = (300..312).collect();
    for (index, chunk) in early_rows.chunks(4).zip(late_rows.chunks(6)).enumerate() {
        let (early_chunk, late_chunk) = chunk;
        if index == 0 {
            append_values_at(&client, &table, uuid::Uuid::now_v7(), early_chunk, early)
                .await
                .expect("the early-partition append is acknowledged");
            append_values_at(&client, &table, uuid::Uuid::now_v7(), late_chunk, late)
                .await
                .expect("the late-partition append is acknowledged");
        } else {
            append_values_at(&client, &table, uuid::Uuid::now_v7(), late_chunk, late)
                .await
                .expect("the late-partition append is acknowledged");
            append_values_at(&client, &table, uuid::Uuid::now_v7(), early_chunk, early)
                .await
                .expect("the early-partition append is acknowledged");
        }
    }

    // Freeze both partitions, then publish only the earlier one. The later
    // partition keeps its live authority, so the pinned cut names one hot
    // object whose WAL envelope spans records the still-live member owns.
    let scribe = server.bifrost_scribe().expect("the pod owns a Scribe");
    server
        .seal_bifrost_writable_for_test()
        .await
        .expect("both partitions freeze");
    await_persistence_drained(&scribe).await;
    let early_partition = vala_bifrost_redux::catalog::TimeGranularity::Hour
        .bucket(early)
        .expect("the earlier event time has an hour partition");
    let late_partition = vala_bifrost_redux::catalog::TimeGranularity::Hour
        .bucket(late)
        .expect("the current event time has an hour partition");
    assert!(
        server
            .publish_bifrost_partition_for_test(early_partition)
            .await
            .expect("the earlier partition publishes")
            > 0,
        "the fixture must publish at least one earlier-partition claim"
    );

    let authorities = server
        .bifrost_live_authorities_for_test()
        .expect("the pod reports where its generations are readable");
    let enclosed: Vec<(u64, u64)> = authorities
        .iter()
        .filter(|live| live.key.partition == late_partition)
        .filter_map(|live| match &live.authority {
            vala_bifrost_redux::scribe::hot_source::HotAuthority::StagedRun { wal, .. } => {
                Some((wal.0.as_u64(), wal.1.as_u64()))
            }
            _ => None,
        })
        .collect();
    assert!(
        !enclosed.is_empty(),
        "the fixture requires the later partition to still be served by a staged member: {authorities:?}"
    );
    let published = server
        .published_hot_files_for_test(first_tenant, BifrostNamespace::Datasets.as_str(), &name)
        .await
        .expect("published hot files are inspectable");
    assert!(
        published.iter().any(|file| {
            enclosed
                .iter()
                .any(|(min, max)| file.wal_lsn_min <= *min && *max <= file.wal_lsn_max)
        }),
        "the fixture requires a published envelope that encloses the unpublished member: \
         published={published:?} enclosed={enclosed:?}"
    );

    // The strict read spans both partitions while their authorities differ. It
    // must return the exact union: the earlier partition from its hot object
    // and the enclosed later partition from its live authority, once each.
    assert_eq!(
        sorted_values(&client, &table).await,
        {
            let mut union = mine.clone();
            union.extend(early_rows.iter().copied());
            union.extend(late_rows.iter().copied());
            union.sort_unstable();
            union
        },
        "a published envelope may not suppress a live member it does not own"
    );

    server
        .flush_bifrost()
        .await
        .expect("the first tenant's rows publish");

    let published = server
        .published_hot_files_for_test(first_tenant, BifrostNamespace::Datasets.as_str(), &name)
        .await
        .expect("published hot files are inspectable");
    let partition_rows = |start: chrono::DateTime<chrono::Utc>| -> u64 {
        published
            .iter()
            .filter(|file| file.promotion_record.partition.start_utc == start)
            .map(|file| file.row_count)
            .sum()
    };
    assert_eq!(
        partition_rows(hour_start(early)),
        early_rows.len() as u64,
        "rows carrying the earlier event time belong to the earlier hour partition"
    );
    assert_eq!(
        partition_rows(hour_start(late)),
        (mine.len() + late_rows.len()) as u64,
        "rows carrying the current event time belong to the current hour partition"
    );
    let mut before_this_hour = read_sql(
        &client,
        &format!(
            "SELECT value FROM {table} WHERE wyrd_event_time < TIMESTAMP '{}'",
            hour_start(late).format("%Y-%m-%d %H:%M:%S")
        ),
    )
    .await;
    before_this_hour.sort_unstable();
    assert_eq!(
        before_this_hour, early_rows,
        "a partition-qualified read must return only the rows in that window"
    );

    // The neighbouring tenant is unmoved by everything the first one did. The
    // flush is a node operation, so its rows publish too; what the fence
    // requires is that each tenant's objects account for exactly its own rows.
    assert_eq!(
        sorted_values(&neighbour, &table).await,
        theirs,
        "publishing one tenant's rows may not change what another tenant reads"
    );
    assert_eq!(
        published_rows(&server, second_tenant, &name).await,
        theirs.len() as u64,
        "a tenant's published objects must account for exactly its own rows"
    );
    assert_eq!(
        published_rows(&server, first_tenant, &name).await,
        (mine.len() + early_rows.len() + late_rows.len()) as u64,
        "a tenant's published objects may not absorb another tenant's rows"
    );

    server.shutdown().await.expect("the server drains cleanly");
}
