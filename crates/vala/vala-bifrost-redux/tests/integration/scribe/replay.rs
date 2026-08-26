//! WAL replay after restart: durability, row identity, cross-epoch artifact
//! identity, and exactly-once publication of a large record.
//!
//! Module of the `scribe` group; shared fixtures live in `persistence_support.rs`.

use arrow::array::Int32Array;
use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;
use std::time::{Duration, Instant};

use super::persistence_support::*;

#[tokio::test]
async fn replayed_generation_publishes_durably_after_restart() {
    let fixture = PersistenceFixture::start_after_wal_restart(false).await;
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        if fixture.scribe.persistence_queue_depth_for_test() == 0 && rows(&fixture).await.len() == 3
        {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "replay persistence stalled: {:?}",
            fixture.faults.last_error_for_test()
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert_eq!(rows(&fixture).await.len(), 3);
    // The three replayed same-key generations must publish as an ordered chain
    // of disjoint WAL-LSN ranges (durable FIFO order lives in the WAL range, not
    // in `created_at`, which ties across the batched replay commits).
    assert_wal_lsn_chain(&rows_for_table(&fixture, "restart_publish_events").await, 3);
    assert_eq!(audit_counts(&fixture).await, (0, 3));
    assert_eq!(object_paths(&fixture).await.len(), 3);
    fixture.stop().await;
}

/// Automatic persistence keeps old- and new-epoch objects distinct and exact.
#[tokio::test]
async fn automatic_scribe_artifact_identity_survives_cross_epoch_restart() {
    let fixture = PersistenceFixture::start_after_wal_restart(false).await;
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        if fixture.scribe.persistence_queue_depth_for_test() == 0
            && artifact_rows_for_table(&fixture, "restart_publish_events")
                .await
                .len()
                == 3
        {
            break;
        }
        assert!(Instant::now() < deadline, "epoch-one replay stalled");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    append_one(&fixture, "restart_publish_events", 99).await;
    fixture
        .scribe
        .flush_writable_for_test()
        .await
        .expect("epoch-two generation flush");
    wait_for_state(&fixture, 0).await;

    let rows = artifact_rows_for_table(&fixture, "restart_publish_events").await;
    assert_eq!(rows.len(), 4);
    assert_eq!(rows.iter().filter(|row| row.0 == 1).count(), 3);
    assert_eq!(rows.iter().filter(|row| row.0 == 2).count(), 1);
    assert_eq!(
        rows.iter()
            .map(|row| row.3.as_str())
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        rows.len(),
        "cross-epoch durable paths must be distinct: {rows:?}"
    );
    assert_catalog_object_parity(&fixture, &rows).await;
    assert_eq!(audit_counts(&fixture).await, (1, 4));
    fixture.stop().await;
}

/// Preserves immutable row identity from a replayed WAL Arrow batch into Parquet publication.
#[tokio::test]
async fn wal_replay_preserves_row_identity() {
    let fixture = PersistenceFixture::start_after_wal_restart(false).await;
    let deadline = Instant::now() + Duration::from_secs(15);
    let path = loop {
        let persisted = rows(&fixture).await;
        if fixture.scribe.persistence_queue_depth_for_test() == 0 && !persisted.is_empty() {
            break persisted[0].2.clone();
        }
        assert!(Instant::now() < deadline, "replay publication stalled");
        tokio::time::sleep(Duration::from_millis(50)).await;
    };
    let bytes = fixture
        .operator
        .read(&path)
        .await
        .expect("published parquet reads");
    let builder = ParquetRecordBatchReaderBuilder::try_new(bytes::Bytes::from(bytes.to_vec()))
        .expect("published object is parquet");
    let reader = builder.build().expect("parquet reader builds");
    let ordinals = reader
        .flat_map(|batch| {
            let batch = batch.expect("parquet batch decodes");
            batch
                .column_by_name("wyrd_row_ordinal")
                .expect("row identity column persists")
                .as_any()
                .downcast_ref::<Int32Array>()
                .expect("row identity remains Int32")
                .values()
                .to_vec()
        })
        .collect::<Vec<_>>();
    assert_eq!(ordinals.len(), 50_000);
    assert!(ordinals.iter().enumerate().all(|(index, ordinal)| {
        *ordinal == i32::try_from(index).expect("test ordinal fits i32")
    }));
    fixture.stop().await;
}

#[tokio::test]
async fn replayed_generation_failure_keeps_replacement_unready() {
    let error = match PersistenceFixture::start_after_wal_restart_with_keys(ReplayRestartSpec {
        fail_replay_write: true,
        table_names: &["restart_publish_events"],
        generations: 3,
        object_write_delays: &[
            Duration::from_millis(300),
            Duration::from_millis(1),
            Duration::from_millis(1),
        ],
        memory: persistence_test_roles(9 * 1024 * 1024 * 1024),
        wal_io_threads: 2,
        rows_per_generation: 25_000,
        wal_segment_bytes: None,
    })
    .await
    {
        Ok(fixture) => {
            fixture.stop().await;
            panic!("replay publication failure must fail replacement startup");
        }
        Err(error) => error,
    };
    assert!(
        error
            .to_string()
            .contains("test object-store write failure")
    );
}

/// Exact-floor restart settles identity ownership and suppresses duplicate replay.
///
/// # Panics
///
/// Panics if mixed-role exact-floor recovery fails, publishes a duplicate,
/// remains unready, or leaves any root-owned Scribe bytes after settlement.
#[tokio::test]
async fn pg_writer_produced_large_record_replays_and_publishes_exactly_once() {
    let memory = persistence_test_roles(768 * 1024 * 1024);
    let baseline = memory.snapshot().expect("baseline root snapshot");
    assert_eq!(baseline.plan.scribe_floor_bytes, 256 * 1024 * 1024);
    assert_eq!(baseline.plan.elastic_memory_bytes, 0);
    let fixture = PersistenceFixture::start_after_wal_restart_with_keys(ReplayRestartSpec {
        fail_replay_write: false,
        table_names: &["exact_floor_replay"],
        generations: 1,
        object_write_delays: &[Duration::from_millis(1)],
        memory,
        wal_io_threads: 1,
        rows_per_generation: 10_000,
        wal_segment_bytes: None,
    })
    .await
    .expect("exact-floor replay");
    assert!(fixture.scribe.is_ready());
    assert_eq!(
        rows_for_table(&fixture, "exact_floor_replay").await.len(),
        1
    );

    fixture
        .scribe
        .replay_wal_async()
        .await
        .expect("duplicate replay settles through the durable fence");
    assert_eq!(
        rows_for_table(&fixture, "exact_floor_replay").await.len(),
        1
    );
    let settled = fixture.memory.snapshot().expect("settled root snapshot");
    assert_eq!(
        settled.scribe_memory_used_bytes,
        baseline.scribe_memory_used_bytes
    );
    assert_eq!(settled.elastic_memory_used_bytes, 0);
    fixture.stop().await;
}
