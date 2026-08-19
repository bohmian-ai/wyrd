use std::sync::Arc;

use crate::catalog::{TableRef, TenantTableBinding};
use crate::contracts::ScribeError;
use crate::namespaces::BifrostNamespace;
use crate::schema::SchemaFingerprint;
use crate::scribe::ScribeAppend;
use crate::scribe::ScribeImpl;
use crate::scribe::seal_key::{EventDay, SealKey};
use crate::scribe::stream_identity::{NodeId, StreamIdentity, WriterEpoch};
use crate::scribe::tail_rpc::FetchLiveTailRequest;
use crate::scribe::wal::{WalConfig, WalLsn, WalWriter};
use arrow::array::{ArrayRef, Int64Array, TimestampMicrosecondArray};
use arrow::datatypes::{DataType, Field, Schema, TimeUnit};
use arrow::record_batch::RecordBatch;
use chrono::NaiveDate;
use tempfile::TempDir;
use uuid::Uuid;
use wyrd_runtime::{PermissionSet, Principal, PrincipalKind};
use wyrd_spec::auth::PrincipalId;
use wyrd_spec::ids::DataTenantId;
use wyrd_spec::request_id::RequestId;

/// Build the fixed one-row event-time batch for Scribe path tests.
///
/// # Panics
/// Panics when static time or Arrow fixture construction fails.
fn batch(day: NaiveDate) -> RecordBatch {
    let timestamp = day
        .and_hms_opt(12, 0, 0)
        .expect("valid test time")
        .and_utc()
        .timestamp_micros();
    RecordBatch::try_new(
        Arc::new(Schema::new(vec![
            Field::new(
                "wyrd_event_time",
                DataType::Timestamp(TimeUnit::Microsecond, Some("UTC".into())),
                false,
            ),
            Field::new("value", DataType::Int64, false),
        ])),
        vec![
            Arc::new(TimestampMicrosecondArray::from(vec![timestamp]).with_timezone("UTC")),
            Arc::new(Int64Array::from(vec![42])),
        ],
    )
    .expect("scribe persistence batch")
}

/// Build the tenant principal used by public-shaped append cases.
fn principal(tenant: DataTenantId) -> Principal {
    Principal {
        id: PrincipalId::new(Uuid::now_v7()),
        kind: PrincipalKind::User,
        tenant_id: tenant,
        roles: Vec::new(),
        effective_permissions: PermissionSet::new(),
    }
}

#[tokio::test]
/// Production shards expose exact projections and WAL bounds to tail readers.
async fn production_shard_snapshot_serves_exact_projection_and_lsn_range() {
    let tenant = DataTenantId::new_v7();
    let table = TableRef::new(BifrostNamespace::Bifrost, "scribe_tail");
    let day = NaiveDate::from_ymd_opt(2026, 7, 24).expect("test day");
    let rows = batch(day);
    let batch_id = Uuid::now_v7();
    let temp_dir = TempDir::new().expect("WAL temp dir");
    let operator = Arc::new(
        opendal::Operator::new(opendal::services::Memory::default())
            .expect("memory operator")
            .finish(),
    );
    let wal = Arc::new(
        WalWriter::new(
            temp_dir.path(),
            *Uuid::nil().as_bytes(),
            1,
            WalConfig::default(),
        )
        .expect("WAL writer"),
    );
    let scribe = ScribeImpl::new_for_embedded_with_deps(operator, wal, &Uuid::nil().to_string(), 1);

    let admission = scribe
        .append_durable(ScribeAppend {
            principal: principal(tenant),
            table: table.clone(),
            schema_fingerprint: SchemaFingerprint::from_arrow_schema(rows.schema().as_ref()),
            request_id: RequestId::now_v7(),
            batch_id,
            measured_wire_bytes: 0,
            rows,
        })
        .await
        .expect("durable append");
    assert_eq!(admission.batch_id, batch_id);
    assert_eq!(admission.rows_accepted, 1);
    let inspection = scribe
        .inspection_snapshot()
        .expect("exact ownership inspection");
    assert_eq!(inspection.shard_task_count, 16);
    assert_eq!(inspection.shard_channel_count, 16);
    assert_eq!(
        inspection.memory_by_shard.iter().sum::<usize>(),
        inspection.total_accounted_memory
    );
    assert_eq!(
        inspection
            .memory_by_bucket
            .iter()
            .map(|bucket| bucket.writable_bytes + bucket.immutable_bytes)
            .sum::<usize>(),
        inspection.memory_by_category[4] + inspection.memory_by_category[5]
    );

    let binding = TenantTableBinding::resolve((tenant, table.clone())).expect("binding");
    let stream = StreamIdentity::new(NodeId::new(Uuid::nil()), WriterEpoch::new(1));
    let request = FetchLiveTailRequest {
        binding,
        target_stream: stream,
        start_day: EventDay::new(day),
        end_day: EventDay::new(day),
        after_lsn: WalLsn::ZERO,
        persisted_lsn_ranges: Vec::new(),
        required_columns: vec!["value".to_owned()],
        max_batches: 64,
        max_retained_bytes: 64 * 1024 * 1024,
    };
    let hot = scribe
        .tail_service()
        .expect("tail service")
        .fetch_hot_batches(request.clone())
        .await
        .expect("hot snapshot");
    assert_eq!(hot.len(), 1);
    assert_eq!(hot[0].batch_id, *batch_id.as_bytes());
    assert_eq!(hot[0].rows.schema().fields().len(), 1);
    assert_eq!(hot[0].rows.schema().field(0).name(), "value");

    scribe
        .shutdown(std::time::Instant::now() + std::time::Duration::from_secs(1))
        .await;
}

#[tokio::test]
/// Oracle hot snapshots retain Arrow identity and isolate event days.
async fn oracle_hot_snapshot_preserves_pointer_identity_and_day_isolation() {
    let tenant = DataTenantId::new_v7();
    let pointer_table = TableRef::new(BifrostNamespace::Bifrost, "scribe_pointer_identity");
    let day_table = TableRef::new(BifrostNamespace::Bifrost, "scribe_day_isolation");
    let day_one = NaiveDate::from_ymd_opt(2026, 7, 24).expect("day one");
    let day_two = NaiveDate::from_ymd_opt(2026, 7, 25).expect("day two");
    let source = batch(day_one);
    let source_value = source.column(1).clone();
    let temp_dir = TempDir::new().expect("WAL temp dir");
    let operator = Arc::new(
        opendal::Operator::new(opendal::services::Memory::default())
            .expect("memory operator")
            .finish(),
    );
    let wal = Arc::new(
        WalWriter::new(
            temp_dir.path(),
            *Uuid::nil().as_bytes(),
            1,
            WalConfig::default(),
        )
        .expect("WAL writer"),
    );
    let scribe = ScribeImpl::new_for_embedded_with_deps(operator, wal, &Uuid::nil().to_string(), 1);
    scribe
        .append_durable(ScribeAppend {
            principal: principal(tenant),
            table: pointer_table.clone(),
            schema_fingerprint: SchemaFingerprint::from_arrow_schema(source.schema().as_ref()),
            request_id: RequestId::now_v7(),
            batch_id: Uuid::now_v7(),
            measured_wire_bytes: 0,
            rows: source.clone(),
        })
        .await
        .expect("pointer identity append");
    let stream = StreamIdentity::new(NodeId::new(Uuid::nil()), WriterEpoch::new(1));
    assert_pointer_identity(
        &scribe,
        tenant,
        &pointer_table,
        day_one,
        &source_value,
        stream,
    )
    .await;

    let cross_day = cross_day_batch(source.schema(), day_one, day_two);
    scribe
        .append_durable(ScribeAppend {
            principal: principal(tenant),
            table: day_table.clone(),
            schema_fingerprint: SchemaFingerprint::from_arrow_schema(cross_day.schema().as_ref()),
            request_id: RequestId::now_v7(),
            batch_id: Uuid::now_v7(),
            measured_wire_bytes: 0,
            rows: cross_day,
        })
        .await
        .expect("cross-day append");
    assert_cross_day_materialization(&scribe, tenant, &day_table, day_one, day_two, stream).await;
    assert_other_tenant_isolated(&scribe, &pointer_table, day_one, stream).await;
    scribe
        .shutdown(std::time::Instant::now() + std::time::Duration::from_secs(1))
        .await;
}

/// Assert a returned hot batch shares the expected Arrow allocation.
async fn assert_pointer_identity(
    scribe: &ScribeImpl,
    tenant: DataTenantId,
    table: &TableRef,
    day: NaiveDate,
    source_value: &ArrayRef,
    stream: StreamIdentity,
) {
    let hot = scribe
        .tail_service()
        .expect("tail service")
        .fetch_hot_batches(FetchLiveTailRequest {
            binding: TenantTableBinding::resolve((tenant, table.clone())).expect("pointer binding"),
            target_stream: stream,
            start_day: EventDay::new(day),
            end_day: EventDay::new(day),
            after_lsn: WalLsn::ZERO,
            persisted_lsn_ranges: Vec::new(),
            required_columns: vec!["value".to_owned()],
            max_batches: 64,
            max_retained_bytes: 64 * 1024 * 1024,
        })
        .await
        .expect("pointer hot snapshot");
    assert_eq!(hot.len(), 1);
    assert!(Arc::ptr_eq(source_value, hot[0].rows.column(0)));
}

/// Build one batch spanning two event-day partitions.
///
/// # Panics
/// Panics when the static cross-day Arrow fixture cannot be built.
fn cross_day_batch(schema: Arc<Schema>, day_one: NaiveDate, day_two: NaiveDate) -> RecordBatch {
    RecordBatch::try_new(
        schema,
        vec![
            Arc::new(
                TimestampMicrosecondArray::from(vec![
                    day_one
                        .and_hms_opt(12, 0, 0)
                        .expect("day one time")
                        .and_utc()
                        .timestamp_micros(),
                    day_two
                        .and_hms_opt(12, 0, 0)
                        .expect("day two time")
                        .and_utc()
                        .timestamp_micros(),
                ])
                .with_timezone("UTC"),
            ),
            Arc::new(Int64Array::from(vec![101_i64, 202_i64])),
        ],
    )
    .expect("cross-day batch")
}

/// Assert cross-day materialization preserves row ownership and ordering.
async fn assert_cross_day_materialization(
    scribe: &ScribeImpl,
    tenant: DataTenantId,
    table: &TableRef,
    day_one: NaiveDate,
    day_two: NaiveDate,
    stream: StreamIdentity,
) {
    let read_day = |day: NaiveDate| FetchLiveTailRequest {
        binding: TenantTableBinding::resolve((tenant, table.clone())).expect("day binding"),
        target_stream: stream,
        start_day: EventDay::new(day),
        end_day: EventDay::new(day),
        after_lsn: WalLsn::ZERO,
        persisted_lsn_ranges: Vec::new(),
        required_columns: vec!["value".to_owned()],
        max_batches: 64,
        max_retained_bytes: 64 * 1024 * 1024,
    };
    let day_one_hot = scribe
        .tail_service()
        .expect("tail service")
        .fetch_hot_batches(read_day(day_one))
        .await
        .expect("day one snapshot");
    let day_two_hot = scribe
        .tail_service()
        .expect("tail service")
        .fetch_hot_batches(read_day(day_two))
        .await
        .expect("day two snapshot");
    assert_eq!(day_one_hot.len(), 1);
    assert_eq!(day_two_hot.len(), 1);
    assert_eq!(day_one_hot[0].rows.num_rows(), 1);
    assert_eq!(day_two_hot[0].rows.num_rows(), 1);
    assert_eq!(hot_value(&day_one_hot[0].rows), 101);
    assert_eq!(hot_value(&day_two_hot[0].rows), 202);
}

/// Read the fixture's single hot value.
///
/// # Panics
/// Panics when the fixture column is not the expected Int64 shape.
fn hot_value(rows: &RecordBatch) -> i64 {
    rows.column(0)
        .as_any()
        .downcast_ref::<Int64Array>()
        .expect("hot values")
        .value(0)
}

/// Assert a distinct tenant cannot observe the retained hot batch.
async fn assert_other_tenant_isolated(
    scribe: &ScribeImpl,
    table: &TableRef,
    day: NaiveDate,
    stream: StreamIdentity,
) {
    let other = scribe
        .tail_service()
        .expect("tail service")
        .fetch_hot_batches(FetchLiveTailRequest {
            binding: TenantTableBinding::resolve((DataTenantId::new_v7(), table.clone()))
                .expect("other tenant binding"),
            target_stream: stream,
            start_day: EventDay::new(day),
            end_day: EventDay::new(day),
            after_lsn: WalLsn::ZERO,
            persisted_lsn_ranges: Vec::new(),
            required_columns: vec!["value".to_owned()],
            max_batches: 64,
            max_retained_bytes: 64 * 1024 * 1024,
        })
        .await
        .expect("other tenant snapshot");
    assert!(other.is_empty(), "hot snapshots must be tenant isolated");
}

#[test]
/// A concrete WAL disk fault rejects before mutating the segment.
fn concrete_wal_disk_failure_rejects_before_file_mutation() {
    let temp_dir = TempDir::new().expect("WAL temp dir");
    let writer = WalWriter::new(
        temp_dir.path(),
        *Uuid::nil().as_bytes(),
        1,
        WalConfig::default(),
    )
    .expect("WAL writer");
    writer.trip_disk_full_for_test();
    let seal_key = SealKey::new(
        DataTenantId::new_v7(),
        TableRef::new(BifrostNamespace::Bifrost, "scribe_wal_failure"),
        EventDay::new(NaiveDate::from_ymd_opt(2026, 7, 24).expect("test day")),
    );

    let error = writer
        .append_and_fsync_for_test(&seal_key, [7_u8; 16], b"audit", b"data")
        .expect_err("injected WAL disk failure");
    assert!(matches!(error, ScribeError::WalDiskFull));
    assert_eq!(writer.bytes_on_disk(), 0);
}

#[tokio::test]
/// A shard WAL failure reaches the caller's durable completion boundary.
async fn shard_wal_failure_reaches_the_durable_completion() {
    let tenant = DataTenantId::new_v7();
    let table = TableRef::new(BifrostNamespace::Bifrost, "scribe_wal_failure");
    let day = NaiveDate::from_ymd_opt(2026, 7, 24).expect("test day");
    let temp_dir = TempDir::new().expect("WAL temp dir");
    let operator = Arc::new(
        opendal::Operator::new(opendal::services::Memory::default())
            .expect("memory operator")
            .finish(),
    );
    let wal = Arc::new(
        WalWriter::new(
            temp_dir.path(),
            *Uuid::nil().as_bytes(),
            1,
            WalConfig::default(),
        )
        .expect("WAL writer"),
    );
    wal.trip_disk_full_for_test();
    let scribe = ScribeImpl::new_for_embedded_with_deps(
        operator,
        Arc::clone(&wal),
        &Uuid::nil().to_string(),
        1,
    );
    let rows = batch(day);
    let error = scribe
        .append_durable(ScribeAppend {
            principal: principal(tenant),
            table,
            schema_fingerprint: SchemaFingerprint::from_arrow_schema(rows.schema().as_ref()),
            request_id: RequestId::now_v7(),
            batch_id: Uuid::now_v7(),
            measured_wire_bytes: 0,
            rows,
        })
        .await
        .expect_err("WAL failure must fail the durable completion");
    assert!(matches!(error, ScribeError::WalDiskFull));
    scribe
        .shutdown(std::time::Instant::now() + std::time::Duration::from_secs(1))
        .await;
}
