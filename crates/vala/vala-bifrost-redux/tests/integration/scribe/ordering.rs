//! FIFO publication within a seal key, concurrency across keys, and
//! deterministic ordinals within a fenced generation.
//!
//! Module of the `scribe` group; shared fixtures live in `persistence_support.rs`.

use arrow::array::{Int64Array, RecordBatch, TimestampMicrosecondArray};
use arrow::datatypes::{DataType, Field, Schema, TimeUnit};
use std::sync::Arc;
use std::time::{Duration, Instant};

use super::persistence_support::*;

#[tokio::test]
async fn same_seal_key_generations_publish_in_fifo_order() {
    let fixture = PersistenceFixture::start().await;
    fixture
        .faults
        .set_object_write_delay_for_test(Duration::from_millis(50));
    append_one(&fixture, "fifo_events", 1).await;
    fixture
        .scribe
        .flush_writable_for_test()
        .await
        .expect("first flush");
    append_one(&fixture, "fifo_events", 2).await;
    fixture
        .scribe
        .flush_writable_for_test()
        .await
        .expect("second flush");
    wait_for_state(&fixture, 0).await;

    let persisted = rows(&fixture).await;
    assert_eq!(persisted.len(), 2);
    assert!(
        persisted[0].0 < persisted[1].0,
        "same-key FIFO was preserved"
    );
    fixture.stop().await;
}

#[tokio::test]
async fn different_seal_keys_persist_concurrently() {
    let fixture = PersistenceFixture::start().await;
    fixture
        .faults
        .set_encode_delay_for_test(Duration::from_millis(100));
    fixture
        .faults
        .set_object_write_delay_for_test(Duration::from_millis(100));
    append_one(&fixture, "concurrent_events_a", 1).await;
    append_one(&fixture, "concurrent_events_b", 2).await;
    fixture
        .scribe
        .flush_writable_for_test()
        .await
        .expect("cross-key flush");
    wait_for_state(&fixture, 0).await;

    assert!(
        fixture.faults.max_concurrent_object_writes_for_test() >= 2,
        "different SealKey generations must overlap across persistence workers"
    );
    assert!(
        fixture.faults.max_concurrent_encodes_for_test() >= 2,
        "different SealKey generations must overlap inside actual Parquet encode intervals"
    );
    fixture.stop().await;
}

/// Two whole stored batches that cross the 100K candidate target publish as
/// one fenced generation with consecutive physical ordinals.
#[tokio::test]
async fn multi_candidate_generation_publishes_one_fenced_set_with_deterministic_ordinals() {
    let fixture = PersistenceFixture::start().await;
    let table_name = "multi_candidate_fenced_events";
    let first_id = uuid::Uuid::now_v7();
    let second_id = colocated_distinct_batch_id(fixture.tenant, &table(table_name), first_id);
    for (value, batch_id) in [(1_i64, first_id), (2_i64, second_id)] {
        let schema = Arc::new(Schema::new(vec![
            Field::new(
                "wyrd_event_time",
                DataType::Timestamp(TimeUnit::Microsecond, Some("UTC".into())),
                false,
            ),
            Field::new("value", DataType::Int64, false),
        ]));
        let rows = RecordBatch::try_new(
            schema,
            vec![
                Arc::new(
                    TimestampMicrosecondArray::from(vec![
                        chrono::Utc::now().timestamp_micros();
                        51_201
                    ])
                    .with_timezone("UTC"),
                ),
                Arc::new(Int64Array::from(vec![value; 51_201])),
            ],
        )
        .expect("whole candidate batch");
        register_control_row(&fixture, table_name, &rows).await;
        let admission = ingest_projected_rows(
            &fixture.scribe,
            fixture.tenant,
            table(table_name),
            rows,
            batch_id,
            0,
        )
        .await
        .expect("candidate append");
        assert_eq!(admission.batch_id, batch_id);
    }
    fixture
        .scribe
        .flush_writable_for_test()
        .await
        .expect("one generation flush");
    wait_for_state(&fixture, 0).await;
    let artifacts = artifact_rows_for_table(&fixture, table_name).await;
    assert_eq!(
        artifacts.len(),
        2,
        "both candidates become one complete set"
    );
    let mut conn = fixture
        .database
        .tenant_conn_for(fixture.tenant)
        .await
        .expect("ordinal tenant connection");
    let ordinals: Vec<i16> = sqlx::query_scalar(
        "SELECT file_ordinal FROM vala.file_list WHERE data_tenant_id=$1 AND table_name=$2 ORDER BY file_ordinal",
    )
    .bind(fixture.tenant.as_uuid())
    .bind(table_name)
    .fetch_all(&mut **conn.transaction())
    .await
    .expect("candidate ordinals");
    assert_eq!(ordinals, vec![0, 1]);
    drop(conn);
    assert_eq!(rows_for_table(&fixture, table_name).await.len(), 2);
    assert_eq!(audit_counts(&fixture).await, (2, 1));
    fixture.stop().await;
}

#[tokio::test]
async fn three_generations_same_key_remain_fifo_and_file_paths_are_object_keys() {
    let fixture = PersistenceFixture::start().await;
    fixture
        .faults
        .set_object_write_delay_for_test(Duration::from_millis(50));
    for value in 1..=3 {
        append_one(&fixture, "three_generation_events", value).await;
        fixture
            .scribe
            .flush_writable_for_test()
            .await
            .expect("generation flush");
    }

    wait_for_state(&fixture, 0).await;
    let persisted = rows(&fixture).await;
    let objects = object_paths(&fixture).await;
    assert_eq!(persisted.len(), 3);
    assert_eq!(objects.len(), 3);
    assert!(persisted.windows(2).all(|pair| pair[0].0 < pair[1].0));
    for (_, _, path) in persisted {
        assert_eq!(path.matches("partition_granularity=").count(), 1);
        assert!(
            objects.contains(&path),
            "file_list path must be an object key"
        );
    }
    fixture.stop().await;
}

/// Replays distinct keys in deterministic order and retires the retained WAL
/// only after every reconstructed generation has reached publication terminality.
///
/// # Panics
///
/// Panics when the real PostgreSQL/object-store fixture cannot reconstruct or
/// publish the keys, or when retirement releases the WAL before all rows are
/// visible exactly once.
#[tokio::test]
async fn replayed_distinct_keys_publish_in_replay_order_and_fifo() {
    let fixture = PersistenceFixture::start_after_wal_restart_with_keys(ReplayRestartSpec {
        fail_replay_write: false,
        table_names: &["restart_key_a", "restart_key_b"],
        generations: 2,
        object_write_delays: &[Duration::from_millis(100)],
        memory: persistence_test_roles(9 * 1024 * 1024 * 1024),
        wal_io_threads: 2,
        rows_per_generation: 50_000,
        wal_segment_bytes: None,
    })
    .await
    .expect("replay");
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        if fixture.scribe.persistence_queue_depth_for_test() == 0
            && rows_for_table(&fixture, "restart_key_a").await.len() == 2
            && rows_for_table(&fixture, "restart_key_b").await.len() == 2
        {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "cross-key replay stalled: error={:?} rows_a={} rows_b={} objects={:?}",
            fixture.faults.last_error_for_test(),
            rows_for_table(&fixture, "restart_key_a").await.len(),
            rows_for_table(&fixture, "restart_key_b").await.len(),
            object_paths(&fixture).await,
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert_eq!(fixture.faults.max_concurrent_object_writes_for_test(), 1);
    // Replay deliberately waits for each exact generation's publication and
    // retirement before advancing. Within each key the generations therefore
    // form an ordered chain of disjoint WAL-LSN ranges.
    for table_name in ["restart_key_a", "restart_key_b"] {
        assert_wal_lsn_chain(&rows_for_table(&fixture, table_name).await, 2);
    }
    assert_eq!(audit_counts(&fixture).await, (0, 4));
    assert_eq!(object_paths(&fixture).await.len(), 4);
    assert!(
        fixture.scribe.wal_bytes_on_disk() > 0,
        "the shared recovered WAL remains the sole replay source until every key publishes"
    );
    fixture
        .scribe
        .retire_committed_for_test()
        .await
        .expect("final cohort retirement");
    assert_eq!(
        fixture.scribe.wal_bytes_on_disk(),
        0,
        "only the final multi-key retirement may release the recovered WAL"
    );
    fixture.stop().await;
}
