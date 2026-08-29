//! Tenant, partition, and schema fences around one Scribe table name.

use vala_bifrost_redux::namespaces::BifrostNamespace;

use super::support::{
    append_batch, append_values, append_values_at, published_rows, read_sql, register_table,
    sorted_values, start_scribe_server, tenant_client, unique_table,
};

/// AC22/AC26 Tier-2 owner: one table name is three different fences.
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
/// # Panics
///
/// Panics when a public append or read fails, when either tenant observes the
/// other's rows, when the client's event time does not decide the physical
/// partition, or when a mismatched schema is accepted or leaves rows behind.
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
    let late = chrono::Utc::now();
    let early = late - chrono::Duration::hours(1);
    let early_rows: Vec<i64> = (200..208).collect();
    let late_rows: Vec<i64> = (300..312).collect();
    append_values_at(&client, &table, uuid::Uuid::now_v7(), &early_rows, early)
        .await
        .expect("the early-partition append is acknowledged");
    append_values_at(&client, &table, uuid::Uuid::now_v7(), &late_rows, late)
        .await
        .expect("the late-partition append is acknowledged");
    server
        .flush_bifrost_for_tenant(first_tenant)
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

/// Returns the hour-partition boundary one event time belongs to.
///
/// The default physical layout partitions by hour, so this is the exact
/// `partition.start_utc` the promotion record must carry for rows stamped with
/// `at`.
fn hour_start(at: chrono::DateTime<chrono::Utc>) -> chrono::DateTime<chrono::Utc> {
    use chrono::Timelike;
    at.with_minute(0)
        .and_then(|value| value.with_second(0))
        .and_then(|value| value.with_nanosecond(0))
        .expect("an hour boundary is a valid instant")
}
