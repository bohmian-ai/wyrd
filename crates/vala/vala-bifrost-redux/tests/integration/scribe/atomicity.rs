//! A generation is all-or-nothing: cancellation, SQL failure, and a staged
//! publication race must leave no partial durable state.
//!
//! Module of the `scribe` group; shared fixtures live in `persistence_support.rs`.

use arrow::array::{Int64Array, RecordBatch, TimestampMicrosecondArray};
use arrow::datatypes::{DataType, Field, Schema, TimeUnit};
use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;
use std::sync::Arc;
use std::time::{Duration, Instant};
use vala_bifrost_redux::catalog::{TableRef, TenantTableBinding};
use vala_bifrost_redux::contracts::ScribeAppend;
use vala_bifrost_redux::maintenance::staging_file_channel;
use vala_bifrost_redux::schema::fingerprint::SchemaFingerprint;
use vala_bifrost_redux::scribe::ScribePersistenceConfig;
use vala_bifrost_redux::scribe::file_list_writer::PublicationFenceBarrier;
use vala_bifrost_redux::scribe::memory::MemoryCategory;
use vala_bifrost_redux::scribe::persistence::PersistenceFaults;
use vala_bifrost_redux::scribe::tail_rpc::FetchLiveTailRequest;
use vala_bifrost_redux::scribe::wal::{WalConfig, WalLsn, WalWriter};
use wyrd_spec::DataTenantId;
use wyrd_spec::request_id::RequestId;

use super::persistence_support::*;

/// Chooses a distinct batch identity routing a second tenant-qualified key to one shard.
fn batch_id_for_shard(
    tenant: DataTenantId,
    table: &TableRef,
    target_shard: usize,
    excluded: uuid::Uuid,
) -> uuid::Uuid {
    (0_u128..1_000_000)
        .map(uuid::Uuid::from_u128)
        .find(|candidate| {
            *candidate != excluded
                && vala_bifrost_redux::scribe::routing::shard_for(tenant, table, *candidate)
                    == target_shard
        })
        .expect("a second tenant-qualified key maps to the selected shard")
}

/// Reads one table through the production shard snapshot and returns `(LSN, value)` rows.
///
/// # Panics
///
/// Panics when the production tail service cannot bind, snapshot, or project
/// the requested table, or when the fixture's `value` column is not `Int64`.
async fn hot_values_for_cut(
    fixture: &PersistenceFixture,
    table_name: &str,
    persisted_ranges: Vec<(WalLsn, WalLsn)>,
) -> Vec<(u64, i64)> {
    let service = fixture
        .scribe
        .tail_service()
        .expect("production tail service");
    // These fixtures register the hourly omission default, so the cut must be
    // expressed in hours. The range spans the previous hour as well as the
    // current one so a batch appended just before an hour boundary is still
    // inside the requested cut.
    let now = chrono::Utc::now();
    let granularity = vala_bifrost_redux::catalog::layout::TimeGranularity::Hour;
    let start_partition = granularity
        .bucket(now - chrono::Duration::hours(1))
        .expect("a truncated instant is an exact hourly boundary");
    let end_partition = granularity
        .bucket(now)
        .expect("a truncated instant is an exact hourly boundary");
    service
        .fetch_hot_batches(FetchLiveTailRequest {
            binding: TenantTableBinding::resolve((fixture.tenant, table(table_name)))
                .expect("tenant table binding"),
            target_stream: service.stream(),
            start_partition,
            end_partition,
            after_lsn: WalLsn::ZERO,
            persisted_lsn_ranges: persisted_ranges,
            required_columns: vec!["value".to_owned()],
            predicates: Vec::new(),
            max_batches: 16,
            max_retained_bytes: 16 * 1024 * 1024,
        })
        .await
        .expect("production provider cut")
        .into_iter()
        .flat_map(|batch| {
            let lsn = batch.wal_lsn.as_u64();
            batch
                .rows
                .column_by_name("value")
                .expect("value projection")
                .as_any()
                .downcast_ref::<Int64Array>()
                .expect("value remains Int64")
                .values()
                .iter()
                .map(move |value| (lsn, *value))
                .collect::<Vec<_>>()
        })
        .collect()
}

/// Reads exact `value` rows from one published Parquet object.
///
/// # Panics
///
/// Panics when the object is absent, malformed Parquet, or carries a non-Int64
/// `value` column.
async fn persisted_values(fixture: &PersistenceFixture, path: &str) -> Vec<i64> {
    let bytes = fixture
        .operator
        .read(path)
        .await
        .expect("published object reads");
    ParquetRecordBatchReaderBuilder::try_new(bytes::Bytes::from(bytes.to_vec()))
        .expect("published object is parquet")
        .build()
        .expect("published parquet reader")
        .flat_map(|batch| {
            let batch = batch.expect("published parquet batch");
            batch
                .column_by_name("value")
                .expect("persisted value column")
                .as_any()
                .downcast_ref::<Int64Array>()
                .expect("persisted value remains Int64")
                .values()
                .to_vec()
        })
        .collect()
}

/// Asserts the exact persisted-plus-hot union for one actual provider cut.
///
/// Independently published cohort members contribute fenced WAL ranges while
/// every absent or unretired member remains available from the production hot
/// provider. The combined values must contain every expected row once.
async fn assert_provider_union(fixture: &PersistenceFixture, tables: &[&str], expected: &[i64]) {
    let mut published = Vec::new();
    for table_name in tables {
        published.extend(rows_for_table(fixture, table_name).await);
    }
    let ranges = published
        .iter()
        .map(|row| {
            (
                WalLsn::new(u64::try_from(row.0).expect("positive minimum LSN")),
                WalLsn::new(u64::try_from(row.1).expect("positive maximum LSN")),
            )
        })
        .collect::<Vec<_>>();
    let mut union = Vec::new();
    for table_name in tables {
        union.extend(
            hot_values_for_cut(fixture, table_name, ranges.clone())
                .await
                .into_iter()
                .map(|(_, value)| value),
        );
    }
    for row in &published {
        union.extend(persisted_values(fixture, &row.2).await);
    }
    union.sort_unstable();
    assert_eq!(union, expected);
    assert_eq!(
        union
            .iter()
            .copied()
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        union.len(),
        "persisted-plus-hot provider cut contains no duplicate row"
    );
}

/// Proves the persisted-plus-hot provider union on both sides of real publication.
///
/// Two distinct tenant-qualified keys are placed on one production shard. The
/// later-LSN member is deliberately published first, proving that the provider
/// cut excludes an independently visible non-prefix range while retaining the
/// earlier immutable member. Both deterministic pauses execute inside the
/// actual staged mover and writer-v2 file-list reconciler.
#[tokio::test]
async fn query_cut_is_atomic_across_active_cohort_and_manifest() {
    let fixture = PersistenceFixture::start().await;
    let prefix_table = "z_cut_prefix_events";
    let non_prefix_table = "a_cut_non_prefix_events";
    let prefix_batch_id = uuid::Uuid::now_v7();
    let target_shard = vala_bifrost_redux::scribe::routing::shard_for(
        fixture.tenant,
        &table(prefix_table),
        prefix_batch_id,
    );
    let non_prefix_batch_id = batch_id_for_shard(
        fixture.tenant,
        &table(non_prefix_table),
        target_shard,
        prefix_batch_id,
    );
    assert_eq!(
        vala_bifrost_redux::scribe::routing::shard_for(
            fixture.tenant,
            &table(non_prefix_table),
            non_prefix_batch_id,
        ),
        target_shard,
        "both tenant-qualified keys must enter one production shard cohort"
    );

    append_with_batch_id(&fixture, prefix_table, 11, prefix_batch_id).await;
    append_with_batch_id(&fixture, non_prefix_table, 22, non_prefix_batch_id).await;
    let barrier = PublicationFenceBarrier::for_table(non_prefix_table);
    fixture.faults.pause_next_publication(barrier.clone());
    let flushing_scribe = Arc::clone(&fixture.scribe);
    let flush = tokio::spawn(async move { flushing_scribe.flush_writable_for_test().await });

    tokio::time::timeout(Duration::from_secs(10), barrier.wait_before_publication())
        .await
        .expect("pre-publication barrier timeout");
    assert!(
        rows_for_table(&fixture, non_prefix_table).await.is_empty(),
        "selected local manifest is pinned before its file-list publication"
    );
    assert_provider_union(&fixture, &[prefix_table, non_prefix_table], &[11, 22]).await;
    assert_eq!(
        fixture
            .scribe
            .memtable_stats()
            .expect("pre-publication immutable state")
            .immutable_rows,
        2
    );

    barrier.release_before_publication();
    tokio::time::timeout(Duration::from_secs(10), barrier.wait_after_publication())
        .await
        .expect("post-publication barrier timeout");
    let published = rows_for_table(&fixture, non_prefix_table).await;
    assert_eq!(published.len(), 1, "selected member publishes exactly once");
    assert_provider_union(&fixture, &[prefix_table, non_prefix_table], &[11, 22]).await;
    assert_eq!(
        fixture
            .scribe
            .memtable_stats()
            .expect("post-publication immutable state")
            .immutable_rows,
        2,
        "SQL visibility precedes local immutable retirement"
    );

    barrier.release_after_publication();
    flush
        .await
        .expect("flush task joins")
        .expect("production cohort flush");
    wait_for_state(&fixture, 0).await;
    assert_eq!(rows(&fixture).await.len(), 2);
    fixture
        .scribe
        .retire_committed_for_test()
        .await
        .expect("cohort retirement");
    fixture.stop().await;
}

#[tokio::test]
async fn pg_later_candidate_failure_and_cancellation_leave_no_partial_generation() {
    let fixture = PersistenceFixture::start().await;
    let baseline = fixture.memory.snapshot().expect("baseline root snapshot");
    fixture.faults.fail_object_write_attempt_for_test(2);
    let table_name = "failed_later_candidate_events";
    let front_batch_id = uuid::Uuid::from_u128(0x00FA_11ED);
    let successor_batch_id =
        colocated_distinct_batch_id(fixture.tenant, &table(table_name), front_batch_id);
    for (value, batch_id) in [(1_i64, front_batch_id), (2_i64, successor_batch_id)] {
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
        fixture
            .scribe
            .append(ScribeAppend {
                principal: principal(fixture.tenant),
                table: table(table_name),
                schema_fingerprint: SchemaFingerprint::from_arrow_schema(rows.schema().as_ref()),
                request_id: RequestId::now_v7(),
                batch_id,
                measured_wire_bytes: 0,
                rows,
            })
            .await
            .expect("candidate append");
    }
    fixture
        .scribe
        .flush_writable_for_test()
        .await
        .expect("failed later-candidate flush");
    wait_for_state(&fixture, 1).await;
    assert!(
        rows(&fixture).await.is_empty(),
        "one failed candidate must publish none of its generation"
    );
    assert_eq!(
        object_paths(&fixture).await.len(),
        1,
        "the first candidate remains durable for deterministic retry"
    );
    assert_eq!(
        fixture.scribe.memory_snapshot().categories[MemoryCategory::Persistence as usize],
        0,
        "the failed later candidate releases all incremental producer workspace"
    );

    retry_and_wait(&fixture).await;
    assert_eq!(
        rows(&fixture).await.len(),
        2,
        "retry publishes the complete two-candidate generation"
    );
    assert_eq!(object_paths(&fixture).await.len(), 2);
    fixture
        .scribe
        .retire_committed_for_test()
        .await
        .expect("published generation retirement");
    let settled = fixture.memory.snapshot().expect("settled root snapshot");
    assert_eq!(
        settled.scribe_memory_used_bytes,
        baseline.scribe_memory_used_bytes
    );
    assert_eq!(
        settled.elastic_memory_used_bytes,
        baseline.elastic_memory_used_bytes
    );
    fixture.stop().await;
}

#[tokio::test]
async fn failed_generation_reuses_deterministic_object_id() {
    let fixture = PersistenceFixture::start().await;
    fixture.faults.fail_next_sql_commit();
    append_one(&fixture, "fresh_retry_events", 1).await;
    let wal_before = fixture.scribe.wal_bytes_on_disk();
    fixture
        .scribe
        .flush_writable_for_test()
        .await
        .expect("SQL fault flush");
    wait_for_state(&fixture, 1).await;
    assert!(
        rows(&fixture).await.is_empty(),
        "SQL failure must roll back rows"
    );
    assert_eq!(fixture.scribe.wal_bytes_on_disk(), wal_before);
    let retained_paths = object_paths(&fixture).await;
    assert_eq!(retained_paths.len(), 1);
    let retained_bytes = fixture
        .operator
        .read(&retained_paths[0])
        .await
        .expect("retained remote bytes")
        .to_bytes();

    retry_and_wait(&fixture).await;
    let paths = object_paths(&fixture).await;
    assert_eq!(rows(&fixture).await.len(), 1);
    assert_eq!(paths.len(), 1, "retry reuses the exact generation identity");
    assert_eq!(paths, retained_paths);
    assert_eq!(
        fixture
            .operator
            .read(&paths[0])
            .await
            .expect("reconciled remote bytes")
            .to_bytes(),
        retained_bytes
    );
    assert_eq!(audit_counts(&fixture).await, (1, 1));
    fixture.stop().await;
}

/// A failed publication transaction retains its WAL while preserving only the prior ingest audit.
#[tokio::test]
async fn sql_failure_keeps_wal_and_file_list_unchanged() {
    let fixture = PersistenceFixture::start().await;
    append_one(&fixture, "sql_failure_events", 1).await;
    let wal_before = fixture.scribe.wal_bytes_on_disk();
    fixture.faults.fail_next_sql_commit();
    fixture
        .scribe
        .flush_writable_for_test()
        .await
        .expect("SQL failure flush");
    wait_for_state(&fixture, 1).await;

    assert_eq!(fixture.scribe.wal_bytes_on_disk(), wal_before);
    assert!(rows(&fixture).await.is_empty());
    assert_eq!(audit_counts(&fixture).await, (1, 0));
    fixture.stop().await;
}

/// Restarts the fixture's Scribe at the next epoch and replays its WAL.
///
/// The stream fence is advanced first so the replacement writer is the only
/// authorized owner of the WAL — a replacement that replayed under the old
/// epoch would race the writer it replaced. Fresh fault and hint channels are
/// installed on the fixture so post-restart assertions observe only the
/// replacement's behavior, never residue from the first owner.
///
/// # Panics
///
/// Panics if the fence cannot be registered, the replacement WAL writer or
/// Scribe resources cannot be created, or replay fails.
async fn restart_fixture_scribe_and_replay(fixture: &mut PersistenceFixture) {
    let node_id = fixture.node_id;
    PersistenceFixture::register_scribe_fence(&fixture.database, node_id, 2).await;
    let wal = Arc::new(
        WalWriter::new(
            fixture.wal_root.path(),
            *node_id.as_bytes(),
            2,
            WalConfig::default(),
        )
        .expect("replacement WAL writer"),
    );
    let scribe_resources = fixture
        .memory
        .scribe()
        .expect("replacement Scribe resources");
    let (_, output_scratch) = scribe_resources
        .volume_capabilities()
        .expect("replacement output scratch");
    let replacement_faults = PersistenceFaults::default();
    let (publisher, hint_inbox) = staging_file_channel(16).expect("replacement hints");
    let persistence =
        ScribePersistenceConfig::new(Arc::new(fixture.database.vala_postgres().clone()), 16, 2)
            .with_operator_pool(fixture.database.operator_pool().clone())
            .with_output_scratch(output_scratch)
            .with_test_faults(replacement_faults.clone());
    fixture.scribe = restarted_scribe(RestartedScribeConfig {
        operator: Arc::clone(&fixture.operator),
        wal,
        node_id,
        admission: vala_bifrost_redux::scribe::admission::AdmissionConfig::default(),
        persistence,
        resources: scribe_resources,
        publisher,
        wal_io_threads: 2,
    });
    fixture.faults = replacement_faults;
    fixture.hint_inbox = hint_inbox;
    fixture
        .scribe
        .replay_wal_async()
        .await
        .expect("replacement staged publication recovery");
}

/// Proves the durable stage survives competing creators, catalog failure, and restart.
#[tokio::test]
async fn pg_staged_publication_race_preserves_published_remote_bytes() {
    let mut fixture = PersistenceFixture::start().await;
    let table = "staged_publication_race_events";
    append_one(&fixture, table, 1).await;
    fixture.faults.fail_next_sql_commit();
    fixture
        .scribe
        .flush_writable_for_test()
        .await
        .expect("first creator flush");
    wait_for_state(&fixture, 1).await;

    let first_paths = object_paths(&fixture).await;
    assert_eq!(
        first_paths.len(),
        1,
        "ambiguous upload retains one remote key"
    );
    let remote_key = first_paths[0].clone();
    let remote_before = fixture
        .operator
        .read(&remote_key)
        .await
        .expect("remote bytes after catalog failure")
        .to_vec();
    assert!(!remote_before.is_empty());
    assert!(rows_for_table(&fixture, table).await.is_empty());
    assert_eq!(audit_counts(&fixture).await, (1, 0));

    fixture.faults.fail_next_sql_commit();
    fixture.scribe.check_age(Instant::now());
    wait_for_state(&fixture, 1).await;
    assert_eq!(
        fixture
            .operator
            .read(&remote_key)
            .await
            .expect("remote bytes after competing creator")
            .to_vec(),
        remote_before,
        "losing creator and catalog failure never delete remote bytes"
    );

    fixture.scribe.shutdown(Instant::now()).await;
    restart_fixture_scribe_and_replay(&mut fixture).await;

    let published = rows_for_table(&fixture, table).await;
    assert_eq!(published.len(), 1, "restart publishes one file-list row");
    assert_eq!(audit_counts(&fixture).await, (1, 1));
    assert_eq!(object_paths(&fixture).await, vec![remote_key.clone()]);
    assert_eq!(
        fixture
            .operator
            .read(&remote_key)
            .await
            .expect("committed remote bytes")
            .to_vec(),
        remote_before
    );
    let staged_root = fixture.wal_root.path().join("staged");
    let remaining = std::fs::read_dir(staged_root)
        .map(std::iter::Iterator::count)
        .unwrap_or_default();
    assert_eq!(
        remaining, 0,
        "only committed publication permits local cleanup"
    );
    fixture.stop().await;
}

/// The visibility row and publication audit roll back together after ingest was audited.
#[tokio::test]
async fn file_list_and_audit_commit_atomically() {
    let fixture = PersistenceFixture::start().await;
    fixture.faults.fail_next_sql_commit();
    append_one(&fixture, "atomic_events", 1).await;
    fixture
        .scribe
        .flush_writable_for_test()
        .await
        .expect("atomic SQL fault flush");
    wait_for_state(&fixture, 1).await;
    assert!(rows(&fixture).await.is_empty());
    assert_eq!(audit_counts(&fixture).await, (1, 0));

    retry_and_wait(&fixture).await;
    assert_eq!(rows(&fixture).await.len(), 1);
    assert_eq!(audit_counts(&fixture).await, (1, 1));
    fixture.stop().await;
}
