//! Bounded persistence: the writer's exact floor, replay ceilings and their
//! retirement, and progress on a near-full root.
//!
//! Module of the `scribe` group; shared fixtures live in `persistence_support.rs`.

use arrow::array::{Int64Array, RecordBatch, TimestampMicrosecondArray};
use arrow::datatypes::{DataType, Field, Schema, TimeUnit};
use arrow::ipc::writer::StreamWriter;
use std::sync::Arc;
use std::time::Duration;
use vala_bifrost_redux::catalog::TableRef;
use vala_bifrost_redux::contracts::{ScribeAppend, ScribeError};
use vala_bifrost_redux::resources::OracleWorkerClass;
use vala_bifrost_redux::schema::fingerprint::SchemaFingerprint;
use vala_bifrost_redux::scribe::NativeIngressTestFrame;
use vala_bifrost_redux::scribe::memory::MemoryCategory;
use wyrd_spec::DataTenantId;
use wyrd_spec::request_id::RequestId;

use super::persistence_support::*;

/// Derives a row count whose managed Arrow ownership strictly exceeds `limit_bytes`.
///
/// The managed fixture schema has linear buffer growth. Two adjacent fixed-size
/// samples therefore expose its exact per-sample growth without hard-coding an
/// assumed byte-per-row ratio into the capacity test.
///
/// # Panics
///
/// Panics when the managed fixture stops growing linearly or the derived row
/// count exceeds the platform's addressable size.
fn replay_rows_exceeding(limit_bytes: usize, tenant: DataTenantId) -> usize {
    const SAMPLE_ROWS: usize = 1024;
    let request_id = RequestId::now_v7();
    let (_, one_sample) = managed_batch_bytes(1, tenant, [0_u8; 16], &request_id, SAMPLE_ROWS);
    let (_, two_samples) = managed_batch_bytes(1, tenant, [0_u8; 16], &request_id, 2 * SAMPLE_ROWS);
    let sample_growth = two_samples
        .checked_sub(one_sample)
        .expect("managed Arrow ownership grows between fixture samples");
    let sample_count = limit_bytes
        .saturating_sub(one_sample)
        .checked_div(sample_growth)
        .expect("fixture sample growth is positive")
        .checked_add(2)
        .expect("fixture sample count fits usize");
    SAMPLE_ROWS
        .checked_mul(sample_count)
        .expect("fixture row count fits usize")
}

/// Proves the automatic persistence producer emits writer-v2 at the exact mixed-role floor.
#[tokio::test]
async fn scribe_persistence_writer_v2_is_bounded_at_exact_floor() {
    let fixture =
        PersistenceFixture::start_with_memory(persistence_test_roles(768 * 1024 * 1024)).await;
    append_one(&fixture, "exact_floor_events", 1).await;
    fixture
        .scribe
        .flush_writable_for_test()
        .await
        .expect("exact-floor flush");
    wait_for_state(&fixture, 0).await;

    let mut conn = fixture
        .database
        .tenant_conn_for(fixture.tenant)
        .await
        .expect("exact-floor tenant connection");
    let (path, file_size, ordinal, checksum): (String, i64, i16, Option<String>) =
        sqlx::query_as(
            "SELECT file_path,file_size,file_ordinal,file_checksum FROM vala.file_list WHERE data_tenant_id=wyrd.current_tenant() AND table_name='exact_floor_events'",
        )
        .fetch_one(&mut **conn.transaction())
        .await
        .expect("exact-floor file-list row");
    assert_eq!(ordinal, 0);
    let checksum = checksum.expect("writer-v2 checksum");
    assert_eq!(checksum.len(), 64);
    assert!(checksum.bytes().all(|byte| byte.is_ascii_hexdigit()));
    let bytes = fixture
        .operator
        .read(&path)
        .await
        .expect("exact-floor writer-v2 object")
        .to_bytes();
    assert_eq!(
        i64::try_from(bytes.len()).expect("object size fits i64"),
        file_size
    );
    let metadata = parquet::file::metadata::ParquetMetaDataReader::new()
        .parse_and_finish(&bytes)
        .expect("standard Parquet decoder accepts persistence output");
    vala_bifrost_redux::parquet::memory::validate_writer_v2_structure(&metadata)
        .expect("persistence output respects structural caps");
    let writer_fields = metadata
        .file_metadata()
        .key_value_metadata()
        .into_iter()
        .flatten()
        .filter(|entry| entry.key.starts_with("wyrd.bifrost."))
        .count();
    assert_eq!(
        writer_fields, 9,
        "writer-v2 publishes the closed metadata set"
    );
    let scratch = fixture.scratch_root.path().join("scribe-output");
    assert_eq!(
        std::fs::read_dir(scratch)
            .expect("Scribe scratch root")
            .count(),
        0,
        "known committed publication cleans its exact generation scratch"
    );
    assert_eq!(
        fixture.scribe.memory_snapshot().categories[MemoryCategory::Persistence as usize],
        0
    );
    drop(conn);
    fixture.stop().await;
}

/// Replay applies publication-driven backpressure when aggregate WAL ownership exceeds memory.
#[tokio::test]
async fn replay_total_above_ceiling_is_bounded_by_persistence_retirement() {
    let memory = persistence_test_roles(1024 * 1024 * 1024);
    let baseline = memory.snapshot().expect("baseline root snapshot");
    let replay_ceiling = baseline
        .plan
        .scribe_floor_bytes
        .checked_add(baseline.plan.elastic_memory_bytes)
        .expect("test Scribe ceiling fits usize");
    let rows_per_generation = 10_000;
    let request_id = RequestId::now_v7();
    let (_, generation_bytes) = managed_batch_bytes(
        1,
        DataTenantId::SYSTEM_OWNER,
        [0_u8; 16],
        &request_id,
        rows_per_generation,
    );
    let table_count = replay_ceiling
        .checked_div(generation_bytes)
        .expect("replay generation owns positive memory")
        .saturating_add(1);
    let tables = (1..=table_count)
        .map(|index| format!("bounded_replay_{index}"))
        .collect::<Vec<_>>();
    let tables = tables.iter().map(String::as_str).collect::<Vec<_>>();
    let fixture = PersistenceFixture::start_after_wal_restart_with_keys(ReplayRestartSpec {
        fail_replay_write: false,
        table_names: &tables,
        generations: 1,
        object_write_delays: &[Duration::from_millis(1)],
        memory,
        wal_io_threads: 1,
        rows_per_generation,
        wal_segment_bytes: Some(16 * 1024 * 1024),
    })
    .await
    .expect("bounded replay completes with one WAL worker");
    assert!(
        fixture.replay_decoded_bytes > replay_ceiling,
        "aggregate decoded WAL ownership {} must exceed the Scribe ceiling {}",
        fixture.replay_decoded_bytes,
        replay_ceiling,
    );
    assert!(fixture.scribe.is_ready());
    for table_name in &tables {
        assert_eq!(rows_for_table(&fixture, table_name).await.len(), 1);
    }
    let restored = fixture.memory.snapshot().expect("restored root snapshot");
    assert_eq!(
        restored.scribe_memory_used_bytes,
        baseline.scribe_memory_used_bytes
    );
    assert_eq!(
        restored.oracle_memory_used_bytes,
        baseline.oracle_memory_used_bytes
    );
    assert_eq!(
        restored.forge_memory_used_bytes,
        baseline.forge_memory_used_bytes
    );
    fixture.stop().await;
}

/// One indivisible replay generation fails closed without advancing readiness.
#[tokio::test]
async fn replay_indivisible_generation_over_ceiling_stays_unready() {
    let memory = persistence_test_roles(1024 * 1024 * 1024);
    let baseline = memory.snapshot().expect("baseline root snapshot");
    let replay_ceiling = baseline
        .plan
        .scribe_floor_bytes
        .checked_add(baseline.plan.elastic_memory_bytes)
        .expect("test Scribe ceiling fits usize");
    let oversized_rows = replay_rows_exceeding(replay_ceiling, DataTenantId::SYSTEM_OWNER);
    let error = match PersistenceFixture::start_after_wal_restart_with_keys(ReplayRestartSpec {
        fail_replay_write: false,
        table_names: &["oversized_replay"],
        generations: 1,
        object_write_delays: &[Duration::from_millis(1)],
        memory: memory.clone(),
        wal_io_threads: 1,
        rows_per_generation: oversized_rows,
        wal_segment_bytes: None,
    })
    .await
    {
        Ok(fixture) => {
            fixture.stop().await;
            panic!("an indivisible generation above the remaining ceiling must fail");
        }
        Err(error) => error,
    };
    match &error {
        ScribeError::IngestBusy { table } => assert!(
            matches!(table.as_str(), "WAL recovery segment" | "WAL replay batch"),
            "replay refusal must name its bounded WAL owner"
        ),
        ScribeError::Internal { detail } => {
            assert!(
                matches!(
                    detail.as_str(),
                    "ingest busy for table: WAL recovery segment"
                        | "ingest busy for table: WAL replay batch"
                ),
                "replay refusal must retain its structural capacity error"
            );
        }
        _ => panic!("replay refusal must retain its structural capacity error: {error:?}"),
    }
    assert_eq!(
        memory
            .snapshot()
            .expect("restored root snapshot")
            .scribe_memory_used_bytes,
        baseline.scribe_memory_used_bytes
    );
    assert_eq!(
        memory
            .snapshot()
            .expect("restored root snapshot")
            .elastic_memory_used_bytes,
        baseline.elastic_memory_used_bytes
    );
}

/// Builds a native Arrow IPC frame carrying a caller-supplied `wyrd_event_time`
/// column whose two rows fall on two distinct UTC partition days.
///
/// The `wyrd_event_time` field uses the managed physical type
/// (`Timestamp(Microsecond, UTC)`) so the native ingest contract accepts it as
/// the authoritative event time instead of stamping the server receipt time.
fn native_event_time_frame(
    tenant: DataTenantId,
    table_ref: &TableRef,
    first_day_micros: i64,
    second_day_micros: i64,
) -> NativeIngressTestFrame {
    let schema = Arc::new(Schema::new(vec![
        Field::new("value", DataType::Int64, false),
        Field::new(
            "wyrd_event_time",
            DataType::Timestamp(TimeUnit::Microsecond, Some("UTC".into())),
            false,
        ),
    ]));
    let batch = RecordBatch::try_new(
        Arc::clone(&schema),
        vec![
            Arc::new(Int64Array::from(vec![1_i64, 2_i64])),
            Arc::new(
                TimestampMicrosecondArray::from(vec![first_day_micros, second_day_micros])
                    .with_timezone("UTC"),
            ),
        ],
    )
    .expect("native caller event-time batch is valid");
    let mut bytes = Vec::new();
    let mut writer = StreamWriter::try_new(&mut bytes, schema.as_ref()).expect("IPC writer");
    writer.write(&batch).expect("IPC batch");
    writer.finish().expect("IPC finish");

    let user_only = Schema::new(vec![Field::new("value", DataType::Int64, false)]);
    let request_id = RequestId::now_v7();
    NativeIngressTestFrame {
        principal: principal(tenant),
        table: table_ref.clone(),
        expected_schema_fingerprint: SchemaFingerprint::from_arrow_schema(&user_only),
        request_id: request_id.clone(),
        batch_id: uuid::Uuid::now_v7(),
        audit_event: audit_event("bifrost.append", request_id),
        payload: bytes.into(),
    }
}

/// Persistence workspace admission remains available while a governed Oracle
/// range overlaps, and both role counters return to their exact baseline.
#[tokio::test]
async fn near_full_root_immutable_owner_still_allows_incremental_candidate_progress() {
    let memory = persistence_test_roles(1024 * 1024 * 1024);
    let fixture = PersistenceFixture::start_with_memory(memory).await;
    let baseline = fixture.memory.snapshot().expect("baseline root snapshot");
    let oracle = fixture.memory.oracle().expect("composed Oracle capability");
    let range = oracle
        .try_acquire_worker(OracleWorkerClass::Analytical)
        .expect("analytical Oracle worker occupancy");
    let schema = Arc::new(Schema::new(vec![
        Field::new(
            "wyrd_event_time",
            DataType::Timestamp(TimeUnit::Microsecond, Some("UTC".into())),
            false,
        ),
        Field::new("payload", DataType::Utf8, false),
    ]));
    let rows = RecordBatch::try_new(
        Arc::clone(&schema),
        vec![
            Arc::new(
                TimestampMicrosecondArray::from(vec![chrono::Utc::now().timestamp_micros(); 4])
                    .with_timezone("UTC"),
            ),
            Arc::new(arrow::array::StringArray::from(vec![
                "x".repeat(
                    (20 * 1024 * 1024) / 4
                );
                4
            ])),
        ],
    )
    .expect("near-target persistence batch");
    register_control_row(&fixture, "persistence_range_overlap", &rows).await;
    let request_id = RequestId::now_v7();
    let measured_wire_bytes =
        vala_bifrost_redux::gate::limits::BIFROST_TRANSPORT_MESSAGE_LIMIT_BYTES;
    fixture
        .scribe
        .append(ScribeAppend {
            principal: principal(fixture.tenant),
            table: table("persistence_range_overlap"),
            schema_fingerprint: SchemaFingerprint::from_arrow_schema(schema.as_ref()),
            request_id,
            batch_id: uuid::Uuid::now_v7(),
            measured_wire_bytes,
            rows,
        })
        .await
        .expect("near-target generation admits under Oracle overlap");
    fixture
        .scribe
        .flush_writable_for_test()
        .await
        .expect("flush while Oracle range is retained");
    wait_for_state(&fixture, 0).await;
    assert_eq!(
        rows_for_table(&fixture, "persistence_range_overlap")
            .await
            .len(),
        1
    );
    fixture
        .scribe
        .retire_committed_for_test()
        .await
        .expect("retire committed generation while Oracle range is retained");
    let overlapped = fixture.memory.snapshot().expect("overlapped root snapshot");
    assert_eq!(overlapped.oracle_memory_used_bytes, range.memory_bytes());
    assert!(overlapped.oracle_memory_used_bytes <= overlapped.plan.managed_memory_bytes);
    drop(range);
    let restored = fixture.memory.snapshot().expect("restored root snapshot");
    assert_eq!(
        restored.oracle_memory_used_bytes,
        baseline.oracle_memory_used_bytes
    );
    assert_eq!(
        restored.scribe_memory_used_bytes, baseline.scribe_memory_used_bytes,
        "scribe ownership did not restore: {restored:?}"
    );
    assert_eq!(
        restored.elastic_memory_used_bytes, baseline.elastic_memory_used_bytes,
        "parent ownership did not restore: {restored:?}"
    );
    fixture.stop().await;
}

/// End-to-end proof that native ingest honours a caller-supplied event time:
/// two rows stamped with event times on two distinct UTC days land in two
/// distinct partition-day objects, matching the projected (OTLP) contract.
#[tokio::test]
async fn native_caller_event_time_lands_on_two_partition_days() {
    let fixture = PersistenceFixture::start().await;
    let second_day = chrono::Utc::now().date_naive();
    let first_day = second_day
        .pred_opt()
        .expect("current date has a predecessor");
    let first_day_micros = first_day
        .and_hms_opt(12, 0, 0)
        .expect("valid time")
        .and_utc()
        .timestamp_micros();
    let second_day_micros = second_day
        .and_hms_opt(12, 0, 0)
        .expect("valid time")
        .and_utc()
        .timestamp_micros();
    let table_ref = table("native_event_time_events");
    let frame = native_event_time_frame(
        fixture.tenant,
        &table_ref,
        first_day_micros,
        second_day_micros,
    );
    register_control_row(
        &fixture,
        "native_event_time_events",
        &RecordBatch::new_empty(Arc::new(Schema::new(vec![Field::new(
            "value",
            DataType::Int64,
            false,
        )]))),
    )
    .await;
    fixture
        .scribe
        .ingest_native_for_test(frame)
        .await
        .expect("native caller event time is admitted");
    fixture
        .scribe
        .flush_writable_for_test()
        .await
        .expect("flush native event-time rows");
    wait_for_state(&fixture, 0).await;

    let persisted = rows_for_table(&fixture, "native_event_time_events").await;
    assert_eq!(
        persisted.len(),
        2,
        "two distinct event days must publish two file-list entries"
    );
    // Object keys carry the exact partition, not a bare day: the table resolves
    // to the hourly omission default, so two event times a day apart land in two
    // distinct `partition_start=` segments under one `partition_granularity`.
    let partition_paths: Vec<String> = object_paths(&fixture)
        .await
        .into_iter()
        .filter(|path| path.contains("partition_start="))
        .collect();
    assert_eq!(
        partition_paths.len(),
        2,
        "two distinct event days must land in two partition objects: {partition_paths:?}"
    );
    let mut partitions: Vec<String> = partition_paths
        .iter()
        .filter_map(|path| {
            path.split('/')
                .find(|segment| segment.starts_with("partition_start="))
                .map(str::to_owned)
        })
        .collect();
    partitions.sort();
    partitions.dedup();
    assert_eq!(
        partitions.len(),
        2,
        "partitions must be distinct: {partitions:?}"
    );
    assert!(
        partitions
            .iter()
            .any(|partition| partition.contains(&first_day.to_string()))
    );
    assert!(
        partitions
            .iter()
            .any(|partition| partition.contains(&second_day.to_string()))
    );
    fixture.stop().await;
}
