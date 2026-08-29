//! Disk-pressure refusal, preserved admitted work, and recovery for every tenant.

use vala_bifrost_redux::namespaces::BifrostNamespace;

use vala_bifrost_redux::scribe::geometry::{
    DEFAULT_GENERATION_ROTATION_CEILING_BYTES, ScribeGeometry,
};

use super::support::{
    append_values, register_table, sorted_values, start_scribe_server_with_geometry, tenant_client,
    unique_table,
};

/// Stable public code the WAL-full refusal must carry.
const WAL_DISK_FULL: &str = "WYRD_VALA_507_WAL_DISK_FULL";
/// Rows in one append.
///
/// Each row is one `i64`, so an append is about 16 KiB of WAL payload.
const ROWS_PER_BATCH: usize = 2_048;
/// Appends each tenant sends before the disk-full condition is injected.
///
/// The recovery half of this owner depends on retirement having real WAL bytes
/// to reclaim, so both tenants have to leave durable segments behind on several
/// lanes before the breaker trips. Sixteen appends is enough for that and small
/// enough that the case stays a seconds-long owner rather than a load test.
const BATCHES_BEFORE_PRESSURE: usize = 8;
/// WAL segment size for this case.
///
/// The only scaled control: production rolls at a size no test can reach
/// cheaply, and this owner needs closed segments to exist so retirement has
/// something to reclaim. Every other control stays production.
const WAL_SEGMENT_BYTES: u64 = 64 * 1024;

/// Pressure refuses fairly, pod-wide, and the pod recovers afterwards.
///
/// A full WAL disk is the one condition Scribe cannot write through, so it
/// refuses at admission before touching the disk. Two things about that refusal
/// are load-bearing and neither is visible in a correct row count. It must be
/// pod-wide rather than aimed at whichever tenant happened to arrive when the
/// disk filled, and it must end: a breaker that latches until restart turns one
/// transient ENOSPC into an outage on a pod whose disk has since drained.
///
/// The case therefore drives the refusal through the public route for two
/// tenants, checks their already-acknowledged rows are still exactly readable
/// while the pod is refusing new work, reclaims WAL space through the ordinary
/// publication path, and requires both tenants to be served again.
///
/// # Panics
///
/// Panics when a pressured append is accepted or carries the wrong public code,
/// when admitted rows are lost while the pod is refusing, when ingest does not
/// recover after retirement reclaims WAL bytes, or when either tenant does not
/// read back exactly what it acknowledged.
#[tokio::test]
#[ignore = "requires Postgres and object storage"]
async fn scribe_backpressure_disk_pressure_and_fairness_recover() {
    let geometry = ScribeGeometry::for_uniform_shard_rotation(
        WAL_SEGMENT_BYTES,
        DEFAULT_GENERATION_ROTATION_CEILING_BYTES as usize,
        std::time::Duration::from_secs(60),
    )
    .expect("the scaled WAL segment size is a coherent geometry");
    let server = start_scribe_server_with_geometry(geometry).await;
    let first_tenant = server.data_tenant_id();
    let second_tenant = server
        .seed_tenant("scribe-backpressure-neighbour")
        .await
        .expect("the neighbouring tenant is seeded");
    let first_name = unique_table("backpressure_first");
    let second_name = unique_table("backpressure_second");
    let first_table = register_table(
        &server,
        first_tenant,
        BifrostNamespace::Datasets,
        &first_name,
    )
    .await;
    let second_table = register_table(
        &server,
        second_tenant,
        BifrostNamespace::Datasets,
        &second_name,
    )
    .await;
    let first = tenant_client(&server, first_tenant).await;
    let second = tenant_client(&server, second_tenant).await;

    // Admitted before pressure: this is the work the pod must not lose, and it
    // is also what closes the WAL segments retirement later reclaims.
    let mut admitted: Vec<i64> = Vec::with_capacity(BATCHES_BEFORE_PRESSURE * ROWS_PER_BATCH);
    for batch in 0..BATCHES_BEFORE_PRESSURE {
        let first_row = (batch * ROWS_PER_BATCH) as i64;
        let rows: Vec<i64> = (first_row..first_row + ROWS_PER_BATCH as i64).collect();
        append_values(&first, &first_table, uuid::Uuid::now_v7(), &rows)
            .await
            .unwrap_or_else(|error| {
                panic!("first tenant batch {batch} is acknowledged: {error:?}")
            });
        append_values(&second, &second_table, uuid::Uuid::now_v7(), &rows)
            .await
            .unwrap_or_else(|error| {
                panic!("second tenant batch {batch} is acknowledged: {error:?}")
            });
        admitted.extend_from_slice(&rows);
    }
    admitted.sort_unstable();

    server
        .trip_bifrost_wal_disk_full_for_test()
        .expect("the WAL breaker is reachable");

    // Refusal is pod-wide and typed, not one tenant's punishment.
    let refused: Vec<i64> = (1_000_000..1_000_000 + ROWS_PER_BATCH as i64).collect();
    for (label, client, table) in [
        ("first", &first, &first_table),
        ("second", &second, &second_table),
    ] {
        let error = append_values(client, table, uuid::Uuid::now_v7(), &refused)
            .await
            .expect_err("an append under WAL disk pressure must be refused");
        assert_eq!(
            error.code(),
            WAL_DISK_FULL,
            "the {label} tenant must receive the WAL-full refusal, got {error:?}"
        );
    }

    // Admitted work survives the refusal: pressure is backpressure, not loss.
    assert_eq!(
        sorted_values(&first, &first_table).await,
        admitted,
        "the first tenant's acknowledged rows must stay readable under pressure"
    );
    assert_eq!(
        sorted_values(&second, &second_table).await,
        admitted,
        "the second tenant's acknowledged rows must stay readable under pressure"
    );

    // Publication retires the closed WAL segments, which is the event that
    // frees space and therefore the only event allowed to clear the breaker.
    server
        .flush_bifrost()
        .await
        .expect("the staged members publish");

    // Recovery, and it reaches both tenants rather than only the one that was
    // writing when the disk drained.
    let recovered: Vec<i64> = (2_000_000..2_000_000 + ROWS_PER_BATCH as i64).collect();
    append_values(&first, &first_table, uuid::Uuid::now_v7(), &recovered)
        .await
        .expect("the first tenant is served again once WAL space is reclaimed");
    append_values(&second, &second_table, uuid::Uuid::now_v7(), &recovered)
        .await
        .expect("the second tenant is served again once WAL space is reclaimed");

    server
        .flush_bifrost()
        .await
        .expect("the recovered rows publish");
    let mut expected = admitted.clone();
    expected.extend_from_slice(&recovered);
    expected.sort_unstable();
    assert_eq!(
        sorted_values(&first, &first_table).await,
        expected,
        "the first tenant must read back exactly what it acknowledged either side of pressure"
    );
    assert_eq!(
        sorted_values(&second, &second_table).await,
        expected,
        "the second tenant must read back exactly what it acknowledged either side of pressure"
    );
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

    server.shutdown().await.expect("the server drains cleanly");
}
