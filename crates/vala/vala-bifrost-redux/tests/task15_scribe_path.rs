use std::sync::Arc;

use arrow::array::{Int64Array, TimestampMicrosecondArray};
use arrow::datatypes::{DataType, Field, Schema, TimeUnit};
use arrow::record_batch::RecordBatch;
use chrono::NaiveDate;
use tempfile::TempDir;
use uuid::Uuid;
use vala_bifrost_redux::catalog::{TableRef, TenantTableBinding};
use vala_bifrost_redux::contracts::Scribe;
use vala_bifrost_redux::contracts::ScribeError;
use vala_bifrost_redux::namespaces::BifrostNamespace;
use vala_bifrost_redux::schema::SchemaFingerprint;
use vala_bifrost_redux::scribe::ScribeAppend;
use vala_bifrost_redux::scribe::ScribeImpl;
use vala_bifrost_redux::scribe::seal_key::{EventDay, SealKey};
use vala_bifrost_redux::scribe::stream_identity::{NodeId, StreamIdentity, WriterEpoch};
use vala_bifrost_redux::scribe::tail_rpc::{FetchLiveTailRequest, TailFrame};
use vala_bifrost_redux::scribe::wal::{WalConfig, WalLsn, WalWriter};
use wyrd_runtime::{PermissionSet, Principal, PrincipalKind};
use wyrd_spec::auth::PrincipalId;
use wyrd_spec::ids::DataTenantId;
use wyrd_spec::request_id::RequestId;

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
                DataType::Timestamp(TimeUnit::Microsecond, None),
                false,
            ),
            Field::new("value", DataType::Int64, false),
        ])),
        vec![
            Arc::new(TimestampMicrosecondArray::from(vec![timestamp])),
            Arc::new(Int64Array::from(vec![42])),
        ],
    )
    .expect("task 15 batch")
}

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
async fn production_shard_snapshot_serves_exact_projection_and_lsn_range() {
    let tenant = DataTenantId::new_v7();
    let table = TableRef::new(BifrostNamespace::Bifrost, "task15_tail");
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
    let scribe = ScribeImpl::new_for_embedded_with_deps(operator, wal, Uuid::nil().to_string(), 1);

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
        required_columns: vec!["value".to_owned()],
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

    let mut encoded_request = request;
    encoded_request
        .required_columns
        .push("data_tenant_id".to_owned());
    let frames = scribe
        .fetch_live_tail(encoded_request)
        .await
        .expect("encoded tail");
    assert!(matches!(frames.last(), Some(TailFrame::Complete)));
    assert!(
        matches!(frames.first(), Some(TailFrame::Batch(frame)) if frame.batch_id == *batch_id.as_bytes())
    );

    scribe.shutdown().await;
}

#[test]
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
        TableRef::new(BifrostNamespace::Bifrost, "task15_wal_failure"),
        EventDay::new(NaiveDate::from_ymd_opt(2026, 7, 24).expect("test day")),
    );

    let error = writer
        .append_and_fsync_for_test(&seal_key, [7_u8; 16], b"audit", b"data")
        .expect_err("injected WAL disk failure");
    assert!(matches!(error, ScribeError::WalDiskFull));
    assert_eq!(writer.bytes_on_disk(), 0);
}

#[tokio::test]
async fn shard_wal_failure_reaches_the_durable_completion() {
    let tenant = DataTenantId::new_v7();
    let table = TableRef::new(BifrostNamespace::Bifrost, "task15_wal_failure");
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
        Uuid::nil().to_string(),
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
    scribe.shutdown().await;
}
