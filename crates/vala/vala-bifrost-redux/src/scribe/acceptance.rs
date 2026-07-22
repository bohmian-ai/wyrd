//! Task 13 acceptance coverage for the queued Scribe contract.

use std::sync::Arc;
use std::time::{Duration, Instant};

use arrow::array::{ArrayRef, TimestampMicrosecondArray};
use arrow::datatypes::{DataType, Field, Schema, TimeUnit};
use arrow::ipc::writer::StreamWriter;
use arrow::record_batch::RecordBatch;
use uuid::Uuid;
use wyrd_runtime::{Principal, PrincipalKind, permission::PermissionSet};
use wyrd_spec::auth::PrincipalId;
use wyrd_spec::ids::DataTenantId;
use wyrd_spec::request_id::RequestId;

use super::admission::{AdmissionConfig, AdmissionController};
use super::execution_lanes::{
    ScribePostAckCpuOp, ScribePostAckCpuPool, ScribePostAckCpuResult, ScribeWalIoPool,
};
use super::preprocess::{AdmittedAppend, AppendSliceId, prepare_append};
use super::seal_key::{EventDay, SealKey};
use super::wal::{WalWriter, encode_append_frame};
use super::writer::TenantTableWriterRegistry;
use super::{ScribeAppend, ScribeImpl};
use crate::catalog::{TableRef, TenantTableBinding};
use crate::contracts::Scribe;
use crate::namespaces::BifrostNamespace;
use crate::schema::fingerprint::SchemaFingerprint;

fn test_lanes(
    preprocess_delay: Duration,
    sync_delay: Duration,
) -> (ScribePostAckCpuPool, ScribeWalIoPool) {
    (
        ScribePostAckCpuPool::with_delay(1, preprocess_delay),
        ScribeWalIoPool::with_delay(1, sync_delay),
    )
}

fn rows(day: i64, count: usize) -> RecordBatch {
    let schema = Arc::new(Schema::new(vec![Field::new(
        "wyrd_event_time",
        DataType::Timestamp(TimeUnit::Microsecond, None),
        false,
    )]));
    RecordBatch::try_new(
        schema,
        vec![Arc::new(TimestampMicrosecondArray::from(
            (0..count)
                .map(|offset| day.saturating_add(i64::try_from(offset).unwrap_or(i64::MAX)))
                .collect::<Vec<_>>(),
        )) as ArrayRef],
    )
    .expect("acceptance batch")
}

fn rows_at(timestamps: Vec<i64>) -> RecordBatch {
    let schema = Arc::new(Schema::new(vec![Field::new(
        "wyrd_event_time",
        DataType::Timestamp(TimeUnit::Microsecond, None),
        false,
    )]));
    RecordBatch::try_new(
        schema,
        vec![Arc::new(TimestampMicrosecondArray::from(timestamps)) as ArrayRef],
    )
    .expect("acceptance timestamp batch")
}

fn cross_day_rows() -> RecordBatch {
    rows_at(vec![1_719_792_000_000_000, 1_719_878_400_000_000])
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

fn append(tenant: DataTenantId, count: usize) -> ScribeAppend {
    let rows = rows(1_721_003_400_000_000, count);
    let schema_fingerprint = SchemaFingerprint::from_arrow_schema(rows.schema().as_ref());
    ScribeAppend {
        principal: principal(tenant),
        table: TableRef::new(BifrostNamespace::Bifrost, "acceptance_events"),
        rows,
        schema_fingerprint,
        request_id: RequestId::now_v7(),
        batch_id: Uuid::now_v7(),
        measured_wire_bytes: 64 * 1024,
    }
}

fn test_wal(temp: &tempfile::TempDir) -> Arc<WalWriter> {
    Arc::new(
        WalWriter::new(temp.path(), [0; 16], 1, DataTenantId::SYSTEM_OWNER, None)
            .expect("acceptance wal"),
    )
}

fn test_operator() -> Arc<opendal::Operator> {
    Arc::new(
        opendal::Operator::new(opendal::services::Memory::default())
            .expect("memory operator")
            .finish(),
    )
}

fn audit_event(table: &TableRef) -> wyrd_spec::vala::api::AuditEvent {
    wyrd_spec::vala::api::AuditEvent {
        request_id: RequestId::now_v7(),
        trace_id: None,
        operation: "test".to_owned(),
        resource: table.fqn(),
        card_ref: None,
        principal_id: PrincipalId::new(Uuid::now_v7()),
        principal_kind: wyrd_spec::auth::PrincipalKindTag::User,
        auth_method: wyrd_spec::vala::api::AuthMethod::Jwt,
        permission: "test".to_owned(),
        decision: wyrd_spec::vala::api::AuditDecision::Allow,
        result: wyrd_spec::vala::api::AuditResult::Success,
        payload_summary: "1 rows".to_owned(),
        detail: None,
    }
}

fn admitted_append(
    admission: &AdmissionController,
    table: TableRef,
    count: usize,
) -> AdmittedAppend {
    AdmittedAppend {
        request_id: RequestId::now_v7(),
        batch_id: Uuid::now_v7(),
        frame_sequence: 0,
        audit_event: audit_event(&table),
        rows: rows(1_721_003_400_000_000, count),
        measured_wire_bytes: 1,
        admitted_bytes: 1,
        reservation: admission
            .try_reserve(table.fqn(), 1)
            .expect("admission reservation"),
        tenant: DataTenantId::SYSTEM_OWNER,
        table,
        queued_at: Instant::now(),
    }
}

#[tokio::test]
async fn ack_returns_after_queue_admission() {
    let (post_ack_cpu, wal_io) = test_lanes(Duration::ZERO, Duration::from_millis(60));
    let telemetry = Arc::new(super::telemetry::ScribeTelemetry::default());
    let scribe = ScribeImpl::new_with_test_lanes(post_ack_cpu, wal_io).with_telemetry(telemetry);
    let started = Instant::now();
    scribe
        .append(append(DataTenantId::SYSTEM_OWNER, 1))
        .await
        .expect("ack");
    assert!(started.elapsed() < Duration::from_millis(50));
    scribe.shutdown().await;
}

#[tokio::test]
async fn ack_does_not_split_or_encode() {
    let (post_ack_cpu, wal_io) = test_lanes(Duration::from_millis(60), Duration::ZERO);
    let telemetry = Arc::new(super::telemetry::ScribeTelemetry::default());
    let scribe =
        ScribeImpl::new_with_test_lanes(post_ack_cpu, wal_io).with_telemetry(Arc::clone(&telemetry));
    let started = Instant::now();
    scribe
        .append(append(DataTenantId::SYSTEM_OWNER, 2))
        .await
        .expect("ack");
    assert!(started.elapsed() < Duration::from_millis(50));
    assert!(
        telemetry
            .snapshot()
            .iter()
            .all(|sample| sample.stage != "preprocess")
    );
    scribe.shutdown().await;
}

#[tokio::test]
async fn schema_fingerprint_conflict_rejects_before_mutation() {
    let scribe = ScribeImpl::new();
    let mut request = append(DataTenantId::SYSTEM_OWNER, 1);
    request.schema_fingerprint = SchemaFingerprint([0; 32]);
    let error = scribe
        .append(request)
        .await
        .expect_err("schema conflict must reject");
    assert!(matches!(
        error,
        crate::contracts::ScribeError::FingerprintMismatch { .. }
    ));
    assert_eq!(scribe.writer_count(), 0);
    assert_eq!(scribe.admission_snapshot().items, 0);
    scribe.shutdown().await;
}

#[test]
fn queue_full_rejects_before_mutation() {
    let admission = AdmissionController::with_config(AdmissionConfig {
        max_items: 1,
        max_bytes: 64,
        max_writers: 1,
        memory_limit_bytes: 1_000,
    });
    let reservation = admission
        .try_reserve("vala.bifrost.events", 64)
        .expect("first slot");
    let error = admission
        .try_reserve("vala.bifrost.events", 1)
        .expect_err("second slot must be rejected");
    assert!(matches!(
        error,
        crate::contracts::ScribeError::IngestBusy { .. }
    ));
    assert_eq!(admission.snapshot().items, 1);
    reservation.release();
}

#[test]
fn cross_day_enqueue_is_atomic() {
    let temp = tempfile::tempdir().expect("temp dir");
    let tenant = DataTenantId::SYSTEM_OWNER;
    let table = TableRef::new(BifrostNamespace::Bifrost, "acceptance_events");
    let reservation = AdmissionController::new()
        .try_reserve(table.fqn(), 1)
        .expect("reservation");
    let append = AdmittedAppend {
        request_id: RequestId::now_v7(),
        batch_id: Uuid::now_v7(),
        frame_sequence: 0,
        audit_event: wyrd_spec::vala::api::AuditEvent {
            request_id: RequestId::now_v7(),
            trace_id: None,
            operation: "test".to_owned(),
            resource: table.fqn(),
            card_ref: None,
            principal_id: PrincipalId::new(Uuid::now_v7()),
            principal_kind: wyrd_spec::auth::PrincipalKindTag::User,
            auth_method: wyrd_spec::vala::api::AuthMethod::Jwt,
            permission: "test".to_owned(),
            decision: wyrd_spec::vala::api::AuditDecision::Allow,
            result: wyrd_spec::vala::api::AuditResult::Success,
            payload_summary: "2 rows".to_owned(),
            detail: None,
        },
        rows: cross_day_rows(),
        measured_wire_bytes: 1,
        admitted_bytes: 1,
        reservation,
        tenant,
        table,
        queued_at: Instant::now(),
    };
    let prepared = prepare_append(append).expect("post-ack preparation");
    assert!(!prepared.slices.is_empty());
    assert_eq!(prepared.slices[0].id.batch_id, prepared.batch_id);
    drop(temp);
}

#[tokio::test]
async fn fsync_runs_after_ack() {
    let (post_ack_cpu, wal_io) = test_lanes(Duration::ZERO, Duration::from_millis(60));
    let telemetry = Arc::new(super::telemetry::ScribeTelemetry::default());
    let scribe =
        ScribeImpl::new_with_test_lanes(post_ack_cpu, wal_io).with_telemetry(Arc::clone(&telemetry));
    let started = Instant::now();
    scribe
        .append(append(DataTenantId::SYSTEM_OWNER, 1))
        .await
        .expect("ack");
    assert!(started.elapsed() < Duration::from_millis(50));
    let deadline = Instant::now() + Duration::from_secs(2);
    while !telemetry
        .snapshot()
        .iter()
        .any(|sample| sample.stage == "wal_sync_data")
    {
        assert!(Instant::now() < deadline, "consumer did not fsync");
        tokio::task::yield_now().await;
    }
    let samples = telemetry.snapshot();
    let ack = samples
        .iter()
        .position(|sample| sample.stage == "append")
        .expect("ack sample");
    let sync = samples
        .iter()
        .position(|sample| sample.stage == "wal_sync_data")
        .expect("sync sample");
    let write = samples
        .iter()
        .position(|sample| sample.stage == "write_all")
        .expect("write sample");
    let memtable = samples
        .iter()
        .position(|sample| sample.stage == "memtable_insert")
        .expect("memtable sample");
    assert!(ack < write && write < memtable && memtable < sync);
    scribe.shutdown().await;
}

#[tokio::test]
async fn boot_replay_restores_pending_generation() {
    let temp = tempfile::tempdir().expect("temp dir");
    let wal = test_wal(&temp);
    let key = SealKey::new(
        DataTenantId::SYSTEM_OWNER,
        TableRef::new(BifrostNamespace::Bifrost, "acceptance_events"),
        EventDay::new(chrono::NaiveDate::from_ymd_opt(2024, 7, 15).expect("date")),
    );
    let handle = wal.handle_for_seal_key(key).expect("handle");
    let table = TableRef::new(BifrostNamespace::Bifrost, "acceptance_events");
    let audit_payload =
        super::audit_envelope::encode_audit_event(&audit_event(&table)).expect("audit");
    let batch = rows(1_721_003_400_000_000, 1);
    let mut data_payload = Vec::new();
    let mut writer = StreamWriter::try_new(&mut data_payload, &batch.schema()).expect("ipc");
    writer.write(&batch).expect("ipc batch");
    writer.finish().expect("ipc finish");
    let frame = encode_append_frame([8; 16], &audit_payload, &data_payload).expect("frame");
    handle.append_frame(&frame).expect("append");
    handle.sync_data().expect("fsync");

    let recovered = ScribeImpl::new_with_deps(
        test_operator(),
        wal,
        "00000000-0000-0000-0000-000000000000".to_owned(),
        1,
    );
    assert_eq!(recovered.replay_wal().expect("replay"), 1);
    let stats = recovered.memtable_stats().expect("stats");
    assert_eq!(stats.immutable_rows, 1);
    assert_eq!(stats.pending_generations, 1);
    recovered.shutdown().await;
}

#[test]
fn queued_crash_loss_is_explicit() {
    let temp = tempfile::tempdir().expect("temp dir");
    let admission = AdmissionController::new();
    let append = admitted_append(
        &admission,
        TableRef::new(BifrostNamespace::Bifrost, "queued_only"),
        1,
    );
    assert_eq!(admission.snapshot().items, 1);
    drop(append);
    assert_eq!(admission.snapshot().items, 0);
    assert!(
        super::replay::replay_wal_directory(temp.path())
            .expect("replay empty queue")
            .is_empty()
    );
}

#[test]
fn fsynced_frames_replay_exactly() {
    let temp = tempfile::tempdir().expect("temp dir");
    let wal = test_wal(&temp);
    let key = SealKey::new(
        DataTenantId::SYSTEM_OWNER,
        TableRef::new(BifrostNamespace::Bifrost, "acceptance_events"),
        EventDay::new(chrono::NaiveDate::from_ymd_opt(2024, 7, 15).expect("date")),
    );
    let handle = wal.handle_for_seal_key(key).expect("handle");
    let table = TableRef::new(BifrostNamespace::Bifrost, "acceptance_events");
    let audit_payload =
        super::audit_envelope::encode_audit_event(&audit_event(&table)).expect("audit");
    let mut data_payload = Vec::new();
    let batch = rows(1_721_003_400_000_000, 1);
    let mut writer = StreamWriter::try_new(&mut data_payload, &batch.schema()).expect("ipc");
    writer.write(&batch).expect("ipc batch");
    writer.finish().expect("ipc finish");
    let frame = encode_append_frame([7; 16], &audit_payload, &data_payload).expect("frame");
    handle.append_frame(&frame).expect("append");
    handle.sync_data().expect("fsync");
    let replay = super::replay::replay_wal_directory(&temp).expect("replay");
    assert_eq!(
        replay
            .values()
            .map(|entry| entry.data_records.len())
            .sum::<usize>(),
        1
    );
    assert_eq!(
        replay
            .values()
            .map(|entry| entry.audit_events.len())
            .sum::<usize>(),
        1
    );
}

#[tokio::test]
async fn consumer_failure_stops_writer_admission() {
    let scribe = ScribeImpl::new();
    let mut request = append(DataTenantId::SYSTEM_OWNER, 1);
    request.rows = RecordBatch::new_empty(Arc::new(Schema::new(vec![Field::new(
        "value",
        DataType::Int64,
        false,
    )])));
    request.schema_fingerprint =
        SchemaFingerprint::from_arrow_schema(request.rows.schema().as_ref());
    scribe
        .append(request)
        .await
        .expect("post-ack request is admitted");

    let deadline = Instant::now() + Duration::from_secs(2);
    while scribe.runtime_snapshot().writers.unhealthy_writers == 0 {
        assert!(
            Instant::now() < deadline,
            "consumer failure was not observed"
        );
        tokio::task::yield_now().await;
    }

    let error = scribe
        .append(append(DataTenantId::SYSTEM_OWNER, 1))
        .await
        .expect_err("unhealthy writer must reject later admission");
    assert!(matches!(
        error,
        crate::contracts::ScribeError::IngestBusy { .. }
    ));
    scribe.shutdown().await;
}

#[test]
fn recordbatch_is_not_redecoded_normally() {
    let temp = tempfile::tempdir().expect("temp dir");
    let admission = AdmissionController::new();
    let table = TableRef::new(BifrostNamespace::Bifrost, "acceptance_events");
    let reservation = admission.try_reserve(table.fqn(), 1).expect("reservation");
    let batch_id = Uuid::now_v7();
    let prepared = prepare_append(AdmittedAppend {
        request_id: RequestId::now_v7(),
        batch_id,
        frame_sequence: 0,
        audit_event: wyrd_spec::vala::api::AuditEvent {
            request_id: RequestId::now_v7(),
            trace_id: None,
            operation: "test".into(),
            resource: table.fqn(),
            card_ref: None,
            principal_id: PrincipalId::new(Uuid::now_v7()),
            principal_kind: wyrd_spec::auth::PrincipalKindTag::User,
            auth_method: wyrd_spec::vala::api::AuthMethod::Jwt,
            permission: "test".into(),
            decision: wyrd_spec::vala::api::AuditDecision::Allow,
            result: wyrd_spec::vala::api::AuditResult::Success,
            payload_summary: "1 rows".into(),
            detail: None,
        },
        rows: rows(1_721_003_400_000_000, 1),
        measured_wire_bytes: 1,
        admitted_bytes: 1,
        reservation,
        tenant: DataTenantId::SYSTEM_OWNER,
        table,
        queued_at: Instant::now(),
    })
    .expect("prepare");
    assert_eq!(
        prepared.slices[0].id,
        AppendSliceId {
            batch_id,
            frame_sequence: 0,
            seal_key: prepared.slices[0].seal_key.clone()
        }
    );
    drop(temp);
}

#[tokio::test]
async fn server_runtime_remains_responsive() {
    let (post_ack_cpu, wal_io) = test_lanes(Duration::ZERO, Duration::from_millis(60));
    let scribe = ScribeImpl::new_with_test_lanes(post_ack_cpu, wal_io);
    let started = Instant::now();
    let (append_result, ()) = tokio::join!(
        scribe.append(append(DataTenantId::SYSTEM_OWNER, 1)),
        tokio::time::sleep(Duration::from_millis(1)),
    );
    append_result.expect("append");
    assert!(started.elapsed() < Duration::from_millis(50));
    scribe.shutdown().await;
}

#[tokio::test]
async fn blocking_executor_saturation_preserves_cross_writer_progress() {
    let admission = AdmissionController::new();
    let executor = ScribePostAckCpuPool::with_delay(1, Duration::from_millis(2));
    let timer = tokio::time::timeout(
        Duration::from_millis(50),
        tokio::time::sleep(Duration::from_millis(1)),
    );
    let mut tasks = Vec::new();
    for _ in 0..300 {
        let executor = executor.clone();
        let table = TableRef::new(BifrostNamespace::Bifrost, "saturated");
        let append = admitted_append(&admission, table, 1);
        tasks.push(tokio::spawn(async move {
            match executor
                .submit(ScribePostAckCpuOp::Preprocess(Box::new(append)))
                .await
            {
                Ok(ScribePostAckCpuResult::Prepared(_)) => Ok(()),
                Err(error) => Err(error.to_string()),
            }
        }));
    }
    timer.await.expect("Tokio timer was starved");
    for task in tasks {
        task.await.expect("executor task join").expect("preprocess");
    }
    assert!(executor.snapshot().saturation_events > 0);
    assert_eq!(executor.snapshot().depth, 0);
    assert_eq!(admission.snapshot().items, 0);
}

#[tokio::test]
async fn concurrent_first_write_creates_one_writer() {
    let temp = tempfile::tempdir().expect("temp dir");
    let admission = AdmissionController::new();
    let registry = TenantTableWriterRegistry::new(
        admission,
        Arc::new(super::memtable::Memtable::new()),
        test_wal(&temp),
        ScribePostAckCpuPool::new(1),
        ScribeWalIoPool::new(1),
        None,
    );
    let binding = TenantTableBinding::resolve((
        DataTenantId::SYSTEM_OWNER,
        TableRef::new(BifrostNamespace::Bifrost, "racing"),
    ))
    .expect("binding");
    let mut tasks = Vec::new();
    for _ in 0..16 {
        let registry = Arc::clone(&registry);
        let binding = binding.clone();
        tasks.push(tokio::spawn(async move {
            registry.get_or_create(binding).expect("writer")
        }));
    }
    for task in tasks {
        let _ = task.await.expect("join");
    }
    assert_eq!(registry.writer_count(), 1);
    registry.shutdown().await;
}

#[test]
fn active_writer_limit_rejects_new_cardinality() {
    let admission = AdmissionController::with_config(AdmissionConfig {
        max_items: 10,
        max_bytes: 10_000,
        max_writers: 1,
        memory_limit_bytes: 10_000,
    });
    let first = admission.try_reserve_writer("one").expect("first writer");
    assert!(admission.try_reserve_writer("two").is_err());
    drop(first);
    assert!(admission.try_reserve_writer("two").is_ok());
}

#[tokio::test]
async fn known_and_dynamic_tables_share_writer_lifecycle() {
    let temp = tempfile::tempdir().expect("temp dir");
    let admission = AdmissionController::new();
    let registry = TenantTableWriterRegistry::new(
        admission,
        Arc::new(super::memtable::Memtable::new()),
        test_wal(&temp),
        ScribePostAckCpuPool::new(1),
        ScribeWalIoPool::new(1),
        None,
    );
    let known = TenantTableBinding::resolve((
        DataTenantId::SYSTEM_OWNER,
        TableRef::parse_fqn("vala.bifrost.known").expect("known table"),
    ))
    .expect("known binding");
    let dynamic = TenantTableBinding::resolve((
        DataTenantId::SYSTEM_OWNER,
        TableRef::parse_fqn("vala.bifrost.dynamic").expect("dynamic table"),
    ))
    .expect("dynamic binding");
    let (known_writer, _) = registry.get_or_create(known.clone()).expect("known writer");
    let (dynamic_writer, _) = registry
        .get_or_create(dynamic.clone())
        .expect("dynamic writer");
    assert_eq!(known_writer.binding, known);
    assert_eq!(dynamic_writer.binding, dynamic);
    assert_eq!(registry.writer_count(), 2);
    registry.shutdown().await;
}

#[tokio::test]
async fn writer_retirement_drains_reserved_send() {
    let temp = tempfile::tempdir().expect("temp dir");
    let admission = AdmissionController::new();
    let memtable = Arc::new(super::memtable::Memtable::new());
    let table = TableRef::new(BifrostNamespace::Bifrost, "reserved");
    let registry = TenantTableWriterRegistry::new(
        admission.clone(),
        Arc::clone(&memtable),
        test_wal(&temp),
        ScribePostAckCpuPool::new(1),
        ScribeWalIoPool::new(1),
        None,
    );
    let binding =
        TenantTableBinding::resolve((DataTenantId::SYSTEM_OWNER, table.clone())).expect("binding");
    let (writer, _) = registry.get_or_create(binding).expect("writer");
    writer
        .close_with_reserved_send_for_test(admitted_append(&admission, table.clone(), 1))
        .await
        .expect("reserved send drained");
    let key = SealKey::new(
        DataTenantId::SYSTEM_OWNER,
        table,
        EventDay::new(chrono::NaiveDate::from_ymd_opt(2024, 7, 15).expect("date")),
    );
    assert_eq!(memtable.row_count(&key).expect("row count"), 1);
}

#[tokio::test]
async fn writer_recreates_after_idle_retirement() {
    let temp = tempfile::tempdir().expect("temp dir");
    let registry = TenantTableWriterRegistry::new(
        AdmissionController::new(),
        Arc::new(super::memtable::Memtable::new()),
        test_wal(&temp),
        ScribePostAckCpuPool::new(1),
        ScribeWalIoPool::new(1),
        None,
    );
    let binding = TenantTableBinding::resolve((
        DataTenantId::SYSTEM_OWNER,
        TableRef::new(BifrostNamespace::Bifrost, "retire"),
    ))
    .expect("binding");
    let (old, _) = registry.get_or_create(binding.clone()).expect("old writer");
    old.make_idle_for_test();
    registry.retire_idle(Instant::now()).await;
    assert_eq!(registry.writer_count(), 0);
    let (replacement, created) = registry.get_or_create(binding).expect("replacement");
    assert!(created);
    assert_ne!(old.instance_id, replacement.instance_id);
    registry.shutdown().await;
}

#[tokio::test]
async fn retiring_writer_never_overlaps_replacement() {
    let temp = tempfile::tempdir().expect("temp dir");
    let registry = TenantTableWriterRegistry::new(
        AdmissionController::new(),
        Arc::new(super::memtable::Memtable::new()),
        test_wal(&temp),
        ScribePostAckCpuPool::new(1),
        ScribeWalIoPool::new(1),
        None,
    );
    let binding = TenantTableBinding::resolve((
        DataTenantId::SYSTEM_OWNER,
        TableRef::new(BifrostNamespace::Bifrost, "exclusive"),
    ))
    .expect("binding");
    let (old, _) = registry.get_or_create(binding.clone()).expect("old writer");
    old.stop_accepting_for_test();
    let error = registry
        .get_or_create(binding.clone())
        .expect_err("replacement must wait for exact old instance removal");
    assert!(matches!(
        error,
        crate::contracts::ScribeError::IngestBusy { .. }
    ));
    registry.remove_if(&old.key, old.instance_id);
    let (replacement, _) = registry.get_or_create(binding).expect("replacement");
    assert_ne!(old.instance_id, replacement.instance_id);
    registry.shutdown().await;
}

#[tokio::test]
async fn pod_global_item_limit_rejects_tiny_batches() {
    let scribe = ScribeImpl::new_with_test_config(
        ScribePostAckCpuPool::with_delay(1, Duration::from_millis(60)),
        ScribeWalIoPool::new(1),
        AdmissionConfig {
            max_items: 2,
            max_bytes: 1024 * 1024,
            max_writers: 10,
            memory_limit_bytes: 10_000,
        },
    );
    let mut first = append(DataTenantId::SYSTEM_OWNER, 0);
    first.table = TableRef::new(BifrostNamespace::Bifrost, "tiny-one");
    let mut second = append(DataTenantId::SYSTEM_OWNER, 0);
    second.table = TableRef::new(BifrostNamespace::Bifrost, "tiny-two");
    scribe.append(first).await.expect("first tiny batch");
    scribe.append(second).await.expect("second tiny batch");
    let error = scribe
        .append(append(DataTenantId::SYSTEM_OWNER, 0))
        .await
        .expect_err("third tiny batch must hit the global item limit");
    assert!(matches!(
        error,
        crate::contracts::ScribeError::IngestBusy { .. }
    ));
    assert_eq!(scribe.admission_snapshot().items, 2);
    scribe.shutdown().await;
}
